use crate::sys_bus::{
    AttackType, DefaultBCEventHandler, DefaultEventHandler, Device, ErrMsg, EventHandler,
    EventHandlerEmitter, Mode, Proto, System, Word, WRD_EMPTY,
};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct CollisionAttackAgainstTheBus {
    pub nwords_inj: u32,
    pub started: u128,
    pub success: bool,
}

impl CollisionAttackAgainstTheBus {
    pub fn inject(&mut self, d: &mut Device) {
        if self.started == 0 {
            self.started = d.clock.elapsed().as_nanos();
            self.success = true;
        }
        for i in 0..self.nwords_inj {
            let w = Word::new_data(i as u32);
            d.log(
                WRD_EMPTY,
                ErrMsg::MsgAttk(format!("Sent Fake Data {} ", w).to_string()),
            );
            d.write(w);
        }
    }
}

impl EventHandler for CollisionAttackAgainstTheBus {
    fn get_attk_type(&self) -> AttackType {
        AttackType::AtkCollisionAttackAgainstTheBus
    }
    fn on_cmd(&mut self, d: &mut Device, w: &mut Word) {
        d.log(
            *w,
            ErrMsg::MsgAttk("Jamming launched (after cmd)".to_string()),
        );
        self.inject(d);
        self.default_on_cmd(d, w);
    }
    fn on_dat(&mut self, d: &mut Device, w: &mut Word) {
        d.log(
            *w,
            ErrMsg::MsgAttk("Jamming launched (after data)".to_string()),
        );
        self.inject(d);
        self.default_on_dat(d, w);
    }
    fn on_sts(&mut self, d: &mut Device, w: &mut Word) {
        d.log(
            *w,
            ErrMsg::MsgAttk("Jamming launched (after status)".to_string()),
        );
        self.inject(d);
        self.default_on_dat(d, w);
    }
}

#[allow(dead_code)]
pub fn test_attack1() {
    // let mut delays_single = Vec::new();
    let n_devices = 8;
    // normal device has 4ns delays (while attacker has zero)
    let w_delays = 4000;
    let mut sys_bus = System::new(n_devices as u32, w_delays);

    // the last device is kept for attacker
    for m in 0..n_devices - 1 {
        if m == 0 {
            sys_bus.run_d(
                m as u8,
                Mode::BC,
                Arc::new(Mutex::new(EventHandlerEmitter {
                    handler: Box::new(DefaultBCEventHandler {
                        total_device: n_devices - 1,
                        target: 0,
                        data: vec![1, 2, 3],
                        proto: Proto::BC2RT,
                        proto_rotate: true,
                    }),
                })),
                false,
            );
        } else {
            sys_bus.run_d(
                m as u8,
                Mode::RT,
                Arc::new(Mutex::new(EventHandlerEmitter {
                    handler: Box::new(DefaultEventHandler {}),
                })),
                false,
            );
        }
    }
    sys_bus.run_d(
        n_devices - 1,
        Mode::RT,
        Arc::new(Mutex::new(EventHandlerEmitter {
            handler: Box::new(CollisionAttackAgainstTheBus {
                nwords_inj: 5,
                started: 0,
                success: false,
            }),
        })),
        true,
    );
    sys_bus.go();
    sys_bus.sleep_ms(10);
    sys_bus.stop();
    sys_bus.join();
}
