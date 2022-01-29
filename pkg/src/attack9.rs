use crate::sys::{
    DefaultEventHandler, DefaultScheduler, Device, ErrMsg, EventHandler, Mode, Router, System,
    Word, WRD_EMPTY,
};

#[derive(Clone, Debug)]
pub struct CommandInvalidationAttack {
    pub attack_times: Vec<u128>,
    pub success: bool,
    pub target: u8,         // the target RT
    pub target_found: bool, // target found in traffic
}

impl CommandInvalidationAttack {
    pub fn inject(&mut self, d: &mut Device) {
        self.attack_times.push(d.clock.elapsed().as_nanos());
        let dword_count = 31;    // default taken from Michael's code
        let tr = 0;              // default taken from Michael's code
        let w = Word::new_cmd(self.target, dword_count, tr);
        d.log(
            WRD_EMPTY,
            ErrMsg::MsgAttk(format!("Attacker>> Injecting fake command on RT{}", w).to_string()),
        );
        d.write(w);
        self.success = true;
    }
}

impl EventHandler for CommandInvalidationAttack {
    fn on_cmd_trx(&mut self, d: &mut Device, w: &mut Word) {
        // This function replaces "find_RT_tcmd" from Michael's code
        let destination = w.address();
        if destination == self.target && w.sub_address()==2 && self.target_found==false {
            d.log(
                *w, 
                ErrMsg::MsgAttk(format!("Attacker>> Target detected(RT{})", self.target).to_string()),
            );
            self.target_found = true;
            self.inject(d);
        }
        self.default_on_cmd_trx(d, w)
    }
}

pub fn test_attack9() {
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
        handler: CommandInvalidationAttack {
            attack_times: Vec::new(),
            success: false,
            target: 5, // attacking RT address @5
            target_found: false,
        },
    };

    sys.run_d(n_devices - 1, Mode::RT, attacker_router, false);
    sys.go();
    sys.sleep_ms(10);
    sys.stop();
    sys.join();
}
