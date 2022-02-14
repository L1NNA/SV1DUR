use crate::sys::{
    AttackType, DefaultEventHandler, DefaultScheduler, Device, ErrMsg, EventHandler, Mode, Proto,
    Router, System, Word, WRD_EMPTY,
};

#[derive(Clone, Debug)]
pub struct DataThrashingAgainstRT {
    pub attack_times: Vec<u128>,
    pub success: bool,
    pub word_count: u8,
    pub target: u8,         // the target RT
    pub target_found: bool, // target found in traffic
}

impl DataThrashingAgainstRT {
    fn inject_words(&mut self, d: &mut Device) {
        self.attack_times.push(d.clock.elapsed().as_nanos());
        let word_count = 30;
        let tr = 0;
        let w = Word::new_cmd(self.target, word_count, tr);
        d.write(w);
        self.success = true;
    }
}

impl EventHandler for DataThrashingAgainstRT {
    fn on_cmd(&mut self, d: &mut Device, w: &mut Word) {
        // This replaces 'jam_cmdwords' from Michael's code
        if w.address() == self.target && w.tr() == 0 {
            self.target_found = true;
            self.word_count = w.dword_count();
            d.log(
                WRD_EMPTY,
                ErrMsg::MsgAttk(format!("Thrashing triggered (after cmd_word)").to_string()),
            );
        }
        self.default_on_cmd(d, w);
    }

    fn on_dat(&mut self, d: &mut Device, w: &mut Word) {
        // This replaces 'jam_datawords' from Michael's code
        if self.target_found {
            self.word_count -= 1;
            if self.word_count == 0 {
                self.target_found = false;
                d.log(
                    WRD_EMPTY,
                    ErrMsg::MsgAttk(format!("Fake command injected!").to_string()),
                );
                self.inject_words(d);
            }
        }
        self.default_on_dat(d, w);
    }
}

#[allow(dead_code)]
pub fn test_attack3() {
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
            sys.run_d(m as u8, Mode::BC, default_router, AttackType::Benign);
        } else {
            sys.run_d(m as u8, Mode::RT, default_router, AttackType::Benign);
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
        handler: DataThrashingAgainstRT {
            attack_times: Vec::new(),
            word_count: 0u8,
            success: false,
            target: 2, // attacking RT address @2
            target_found: false,
        },
    };

    sys.run_d(
        n_devices - 1,
        Mode::RT,
        attacker_router,
        AttackType::AtkDataThrashingAgainstRT,
    );
    sys.go();
    sys.sleep_ms(10);
    sys.stop();
    sys.join();
}
