use crate::sys::{
    DefaultEventHandler, DefaultScheduler, Device, ErrMsg, EventHandler, Mode, Router, System,
    Word, WRD_EMPTY,
};

#[derive(Clone, Debug)]
pub struct FakeStatusReccmd {
    pub attack_times: Vec<u128>,
    pub success: bool,
    pub word_count: u8,
    pub target: u8,         // the target RT
    pub target_found: bool, // target found in traffic
    pub destination: u8,
}

impl FakeStatusReccmd {
    fn fake_status(&mut self, d: &mut Device) {
        self.target_found = false;
        self.attack_times.push(d.clock.elapsed().as_nanos());
        let w = Word::new_status(self.target);
        d.write(w);
        d.log(
            WRD_EMPTY,
            ErrMsg::MsgAttk(format!("Fake status injected!").to_string()),
        );
        self.success = true;
    }
}

impl EventHandler for FakeStatusReccmd {
    fn on_cmd(&mut self, d: &mut Device, w: &mut Word) {
        let destination = w.address();
        // if w.address() != self.address {} //This line won't work yet.  TODO: Get our address.
        if self.target != 2u8.pow(5) - 1 && destination == self.target && w.tr() == 0 {
                d.log(
                    WRD_EMPTY,
                    ErrMsg::MsgAttk(format!("Attacker>> Target detected (RT{})", self.target).to_string()),
                );
                self.target_found = true;
                self.word_count = w.dword_count();
                self.destination = w.address();
                d.log(
                    WRD_EMPTY, 
                    ErrMsg::MsgAttk(format!("Fake status triggered (after cmd_word)").to_string()),
                );
        } else {
            if w.tr() == 0 && destination != 31 {
                d.log(
                    WRD_EMPTY,
                    ErrMsg::MsgAttk(format!("Attacker>> Target detected (RT{})", w.address()).to_string()),
                );
                self.target_found = true;
                self.word_count = w.dword_count();
                self.destination = w.address();
                d.log(
                    WRD_EMPTY,
                    ErrMsg::MsgAttk(format!("Fake status triggered (after cmd_word)").to_string()),
                );
            }
        }
        self.default_on_cmd(d, w);
    }

    fn on_dat(&mut self, d: &mut Device, w: &mut Word) {
        // This takes the place of "intercept_dw" in Michael's code
        if self.target_found && w.data() != 0xffff {
            self.word_count -= 1;
            if self.word_count == 0 {
                self.fake_status(d);
            }
        }
    }
}

#[allow(dead_code)]
pub fn test_attack5() {
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
        handler: FakeStatusReccmd {
            attack_times: Vec::new(),
            word_count: 0u8,
            success: false,
            target: 4, // attacking RT address @4
            target_found: false,
            destination: 0u8,
        },
    };

    sys.run_d(n_devices - 1, Mode::RT, attacker_router, false, 1);
    sys.go();
    sys.sleep_ms(10);
    sys.stop();
    sys.join();
}
