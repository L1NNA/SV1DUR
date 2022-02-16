use crate::sys::{
    AttackType, DefaultEventHandler, DefaultScheduler, Device, ErrMsg, EventHandler, Mode, Proto,
    Router, System, Word, WRD_EMPTY,
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
    let mut sys = System::new(n_devices as u32, w_delays);

    // the last device is kept for attacker
    for m in 0..n_devices - 1 {
        let default_router = Router {
            // control all communications (bc only)
            scheduler: DefaultScheduler {
                total_device: n_devices - 1,
                target: 0,
                data: vec![1, 2, 3],
                proto: Proto::BC2RT,
                proto_rotate: true,
            },
            // control device-level response
            handler: DefaultEventHandler {},
        };

        if m == 0 {
            sys.run_d(
                m as u8,
                Mode::BC,
                Arc::new(Mutex::new(default_router)),
                AttackType::Benign,
            );
        } else {
            sys.run_d(
                m as u8,
                Mode::RT,
                Arc::new(Mutex::new(default_router)),
                AttackType::Benign,
            );
        }
    }
    let attacker_router = Router {
        // control all communications (bc only)
        scheduler: DefaultScheduler {
            total_device: n_devices - 1,
            target: 0,
            data: vec![1, 2, 3],
            proto: Proto::BC2RT,
            proto_rotate: true,
        },
        // control device-level response
        handler: CollisionAttackAgainstTheBus {
            nwords_inj: 5,
            started: 0,
            success: false,
        },
    };

    sys.run_d(
        n_devices - 1,
        Mode::RT,
        Arc::new(Mutex::new(attacker_router)),
        AttackType::AtkCollisionAttackAgainstTheBus,
    );
    sys.go();
    sys.sleep_ms(10);
    sys.stop();
    sys.join();
}
