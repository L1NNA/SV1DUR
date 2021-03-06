use crate::sys_bus::{
    AttackType, DefaultBCEventHandler, DefaultEventHandler, Device, ErrMsg, EventHandler,
    EventHandlerEmitter, Mode, Proto, System, Word, TR, WRD_EMPTY,
};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct DesynchronizationAttackOnRT {
    pub attack_times: Vec<u128>,
    pub success: bool,
    pub word_count: u8,
    pub flag: u8,
    pub target: u8,         // the target RT
    pub target_found: bool, // target found in traffic
}

impl DesynchronizationAttackOnRT {
    fn desynchronize_rt(&mut self, d: &mut Device) {
        d.log(
            WRD_EMPTY,
            ErrMsg::MsgAttk(
                format!("Attacker>> Desynchronizing RT{} ...", self.target).to_string(),
            ),
        );
        let tr = TR::Receive;
        let word_count = 17;
        self.attack_times.push(d.clock.elapsed().as_nanos());
        let w = Word::new_cmd(self.target, word_count, tr);
        d.write(w);
        let w = Word::new_data(0x000F);
        d.write(w);
        // self.target_found = true;
        // for repeat:
        self.target_found = false;
        self.success = true;
    }
}

impl EventHandler for DesynchronizationAttackOnRT {
    fn get_attk_type(&self) -> AttackType {
        AttackType::AtkDesynchronizationAttackOnRT
    }
    fn on_cmd(&mut self, d: &mut Device, w: &mut Word) {
        // This function replaces "find_RT_tcmd" and "find_RT_rcmd" from Michael's code
        // We cannot use on_cmd_trx here because that only fires after on_cmd verifies that the address is correct.
        let destination = w.address();
        self.word_count = w.dword_count();
        if destination == self.target && self.target_found == false {
            // do we need the sub address?
            if self.flag == 0 {
                let new_flag;
                if w.tr() == TR::Transmit {
                    new_flag = 2;
                    self.word_count = w.dword_count();
                } else {
                    new_flag = 1;
                }
                self.flag = new_flag;
            }
            if w.tr() == TR::Receive {
                self.word_count = w.dword_count();
            }
            self.target_found = true;
            d.log(
                *w,
                ErrMsg::MsgAttk(
                    format!("Attacker>> Target detected(RT{})", self.target).to_string(),
                ),
            );
        }
        self.default_on_cmd(d, w);
    }

    #[allow(unused)]
    fn on_dat(&mut self, d: &mut Device, w: &mut Word) {
        // This replaces "watch_data" in Michael's code.
        if self.word_count > 0 {
            self.word_count -= 1;
        }
        if self.flag == 2 && self.word_count == 0 {
            // sleep(3);
            self.desynchronize_rt(d);
        }
    }

    fn on_sts(&mut self, d: &mut Device, w: &mut Word) {
        if w.address() == self.target {
            if self.flag == 1 && self.word_count == 0 {
                // sleep(3);
                self.desynchronize_rt(d);
            } else if self.flag == 2 {
            }
        }
    }
}

#[allow(dead_code)]
pub fn test_attack8() {
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
            handler: Box::new(DesynchronizationAttackOnRT {
                attack_times: Vec::new(),
                word_count: 0u8,
                success: false,
                flag: 0,
                target: 4, // attacking RT address @4
                target_found: false,
            }),
        })),
        true,
    );
    sys_bus.go();
    sys_bus.sleep_ms(10);
    sys_bus.stop();
    sys_bus.join();
}
