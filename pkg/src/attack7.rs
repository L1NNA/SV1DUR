use crate::sys::{
    DefaultEventHandler, DefaultScheduler, Device, ErrMsg, EventHandler, Mode, Router, System,
    Word, WRD_EMPTY,
};

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
            ErrMsg::MsgAttk(format!("Attacker>> Desynchronizing RT{} ...", self.target).to_string()),
        );
        let tr = 0;
        let word_count = 17;
        self.attack_times.push(d.clock.elapsed().as_nanos());
        let w = Word::new_cmd(self.target, word_count, tr);
        d.write(w);
        let w = Word::new_data(0x000F);
        d.write(w);
        self.target_found = true;
        self.success = true;
    }
}

impl EventHandler for DesynchronizationAttackOnRT {
    fn on_cmd(&mut self, d: &mut Device, w: &mut Word) {
        // This function replaces "find_RT_tcmd" and "find_RT_rcmd" from Michael's code
        // We cannot use on_cmd_trx here because that only fires after on_cmd verifies that the address is correct.
        let destination = w.address();
        self.word_count = w.dword_count();
        if destination == self.target && self.target_found==false { // do we need the sub address?
            if self.flag == 0 {
                let new_flag;
                if w.tr() == 1 {
                    new_flag = 2;
                    self.word_count = w.dword_count();
                } else {
                    new_flag = 1;
                }
                self.flag = new_flag;
            }
            if w.tr() == 0 {
                self.word_count = w.dword_count();
            }
            self.target_found = true;
            d.log(
                *w, 
                ErrMsg::MsgAttk(format!("Attacker>> Target detected(RT{})", self.target).to_string()),
            );
        }
        self.default_on_cmd(d, w);
    }

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

pub fn test_attack7() {
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
                proto: 0,
            },
            // control device-level response
            handler: DefaultEventHandler {},
        };

        if m == 0 {
            sys.run_d(m as u8, Mode::BC, default_router, false, 0);
        } else {
            sys.run_d(m as u8, Mode::RT, default_router, false, 0);
        }
    }
    let attacker_router = Router {
        // control all communications (bc only)
        scheduler: DefaultScheduler {
            total_device: n_devices - 1,
            target: 0,
            data: vec![1, 2, 3],
            proto: 0,
        },
        // control device-level response
        handler: DesynchronizationAttackOnRT {
            attack_times: Vec::new(),
            word_count: 0u8,
            success: false,
            flag: 0,
            target: 4, // attacking RT address @5
            target_found: false,
        },
    };

    sys.run_d(n_devices - 1, Mode::RT, attacker_router, false, 8);
    sys.go();
    sys.sleep_ms(10);
    sys.stop();
    sys.join();
}
