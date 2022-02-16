use crate::sys::{
    AttackType, DefaultEventHandler, DefaultScheduler, Device, ErrMsg, EventHandler, Mode, Proto,
    Router, State, System, Word, WRD_EMPTY,
};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct ShutdownAttackRT {
    pub attack_times: Vec<u128>,
    pub success: bool,
    pub word_count: u8,
    pub target: u8,         // the target RT
    pub target_found: bool, // target found in traffic
}

impl ShutdownAttackRT {
    fn kill_rt(&mut self, d: &mut Device) {
        d.log(
            WRD_EMPTY,
            ErrMsg::MsgAttk(format!("Attacker>> Killing RT{}", self.target).to_string()),
        );
        let word_count = 4;
        let tr = 1;
        self.attack_times.push(d.clock.elapsed().as_nanos());
        let w = Word::new_cmd(self.target, word_count, tr);
        d.write(w);
        self.success = true;
        d.set_state(State::Off); // Not sure what's going on here yet.  TODO come back to this.
    }
}

impl EventHandler for ShutdownAttackRT {
    fn on_cmd(&mut self, d: &mut Device, w: &mut Word) {
        if w.address() == self.target && self.target_found == false {
            d.log(
                WRD_EMPTY,
                ErrMsg::MsgAttk(format!("Attacker>> Killing RT{}", self.target).to_string()),
            );
            self.target_found = true;
            self.kill_rt(d);
        }
        self.default_on_cmd(d, w);
    }

    fn on_sts(&mut self, d: &mut Device, w: &mut Word) {
        if w.address() == self.target && self.target_found == false {
            d.log(
                WRD_EMPTY,
                ErrMsg::MsgAttk(format!("Attacker>> Killing RT{}", self.target).to_string()),
            );
            self.target_found = true;
            self.kill_rt(d);
        }
        self.default_on_sts(d, w);
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
        handler: ShutdownAttackRT {
            attack_times: Vec::new(),
            word_count: 0u8,
            success: false,
            target: 4, // attacking RT address @4
            target_found: false,
        },
    };

    sys.run_d(
        n_devices - 1,
        Mode::RT,
        Arc::new(Mutex::new(attacker_router)),
        AttackType::AtkShutdownAttackRT,
    );
    sys.go();
    sys.sleep_ms(10);
    sys.stop();
    sys.join();
}
