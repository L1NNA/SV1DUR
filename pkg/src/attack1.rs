use crate::sys::{
    DefaultEventHandler, DefaultScheduler, Device, ErrMsg, EventHandler, Mode, Router, System,
    Word, WRD_EMPTY,
};

#[derive(Clone, Debug)]
pub struct CollisionAttackAgainstAnRT {
    pub nwords_inj: u8,
    pub attack_times: Vec<u128>,
    pub success: bool,
    pub target: u8,         // the target RT
    pub target_found: bool, // target found in traffic
    pub wc_n: u16,          // words to be injected
}

impl CollisionAttackAgainstAnRT {
    pub fn inject(&mut self, d: &mut Device) {
        self.attack_times.push(d.clock.elapsed().as_nanos());
        self.success = true;
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

impl EventHandler for CollisionAttackAgainstAnRT {
    fn on_cmd(&mut self, d: &mut Device, w: &mut Word) {
        if w.address() == self.target {
            d.log(
                *w,
                ErrMsg::MsgAttk("Jamming launched (after cmd)".to_string()),
            );
            self.nwords_inj = w.dword_count();
            self.target_found = true;
            self.inject(d);
        }
        self.default_on_cmd(d, w);
    }
    fn on_dat(&mut self, d: &mut Device, w: &mut Word) {
        if w.address() == self.target && self.target_found {
            d.log(
                *w,
                ErrMsg::MsgAttk("Jamming launched (after data)".to_string()),
            );
            self.inject(d);
            self.wc_n -= 1;
            if self.wc_n == 0 {
                // attacker has recieved the equivalent number
                // of messages
                self.target_found = false;
            }
        }
        self.default_on_dat(d, w);
    }
    fn on_sts(&mut self, d: &mut Device, w: &mut Word) {
        if w.address() == self.target {
            d.log(
                *w,
                ErrMsg::MsgAttk("Jamming launched (after status)".to_string()),
            );
            self.inject(d);
        }
        self.default_on_dat(d, w);
    }
}

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
                proto: 0,
            },
            // control device-level response
            handler: DefaultEventHandler {},
        };

        if m == 0 {
            sys.run_d(m as u8, Mode::BC, default_router, false);
        } else {
            sys.run_d(m as u8, Mode::RT, default_router, false);
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
        handler: CollisionAttackAgainstAnRT {
            nwords_inj: 0,
            attack_times: Vec::new(),
            success: false,
            target: 5, // attacking RT address @5
            wc_n: 0,   // expected word count (intercepted)
            target_found: false,
        },
    };

    sys.run_d(n_devices - 1, Mode::RT, attacker_router, false);
    sys.go();
    sys.sleep_ms(10);
    sys.stop();
    sys.join();
}