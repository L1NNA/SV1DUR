use crate::sys::{
    AttackType, DefaultEventHandler, DefaultScheduler, Device, EmptyScheduler, ErrMsg,
    EventHandler, Mode, Proto, Router, System, Word, TR, WRD_EMPTY,
};
use std::sync::{Arc, Mutex};

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
        let mode_code = 30;
        let tr = TR::Receive;
        let mut w = Word::new_cmd(self.target, mode_code, tr);
        w.set_mode(1);
        d.write(w);
        self.success = true;
    }
    pub fn verify(&self, system: &System) -> bool {
        let mut bc_ready_times = 0;

        for l in &(system.devices[0].lock().unwrap().logs) {
            if l.6 == ErrMsg::MsgBCReady {
                bc_ready_times += 1;
                if bc_ready_times > 2{
                    // no more than twice
                    return false;
                }
            }
        }

        for d in &system.devices {
            let local_d = d.lock().unwrap();
            if local_d.address == self.target {
                for l in &local_d.logs {
                    if matches!(l.6, ErrMsg::MsgMCXClr { .. }) {
                        // the target's memory has been cleared
                        return true;
                    }
                }
            }
        }

        return false;
    }
}

impl EventHandler for DataThrashingAgainstRT {
    fn on_cmd(&mut self, d: &mut Device, w: &mut Word) {
        // This replaces 'jam_cmdwords' from Michael's code
        // attack only once
        if w.address() == self.target && w.tr() == TR::Receive && !self.target_found {
            self.target_found = true;
            self.word_count = w.dword_count();
            d.log(
                WRD_EMPTY,
                ErrMsg::MsgAttk(format!(">>> Thrashing triggered (after cmd_word)").to_string()),
            );
        }
        self.default_on_cmd(d, w);
    }

    fn on_dat(&mut self, d: &mut Device, w: &mut Word) {
        // This replaces 'jam_datawords' from Michael's code
        if self.target_found && self.word_count >= 1 {
            self.word_count -= 1;
            if self.word_count == 0 {
                // attack only once
                // self.target_found = false;
                d.log(
                    WRD_EMPTY,
                    ErrMsg::MsgAttk(format!(">>> Fake command injected!").to_string()),
                );
                self.inject_words(d);
            }
        }
        self.default_on_dat(d, w);
    }
}

pub fn eval_attack3(w_delays: u128, proto: Proto) -> bool {
    // let mut delays_single = Vec::new();
    let n_devices = 4;
    // normal device has 4ns delays (while attacker has zero)
    // let w_delays = 40000;
    let mut sys = System::new(n_devices as u32, w_delays);

    // the last device is kept for attacker
    for m in 0..n_devices - 1 {
        let default_router = Router {
            // control all communications (bc only)
            scheduler: DefaultScheduler {
                total_device: n_devices - 1,
                target: 0,
                data: vec![1, 2, 3],
                proto: proto,
                proto_rotate: false,
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
    let attk = DataThrashingAgainstRT {
        attack_times: Vec::new(),
        word_count: 0u8,
        success: false,
        target: 2, // attacking RT address @4
        target_found: false,
    };
    let attacker_router = Arc::new(Mutex::new(Router {
        // control all communications (bc only)
        scheduler: EmptyScheduler {},
        // control device-level response
        handler: attk,
    }));

    sys.run_d(
        n_devices - 1,
        Mode::RT,
        Arc::clone(&attacker_router),
        AttackType::AtkDataThrashingAgainstRT,
    );
    sys.go();
    sys.sleep_ms(50);
    sys.stop();
    sys.join();
    let l_router = Arc::clone(&attacker_router);
    return l_router.lock().unwrap().handler.verify(&sys);
    // let l_attk = l_router.unwrap().handler;
    // .lock().unwrap();
    // return l_router.unwrap().handler.verify(&devices, &logs);
    // return handler.verify
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn test_attk_3_r2r_succeed() {
        // 12_000 based on the protocol but there is a probability
        // of success. so here we made it higher
        assert!(eval_attack3(80_000, Proto::RT2RT) == true);
    }
    #[test]
    fn test_attk_3_r2r_failed() {
        assert!(eval_attack3(0, Proto::RT2RT) == false);
    }
    #[test]
    fn test_attk_3_r2b_succeed() {
        assert!(eval_attack3(80_000, Proto::BC2RT) == true);
    }
    #[test]
    fn test_attk3_r2b_failed() {
        assert!(eval_attack3(0, Proto::BC2RT) == false);
    }
}
