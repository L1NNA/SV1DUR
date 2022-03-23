use crate::sys::{
    format_log, AttackType, DefaultEventHandler, DefaultScheduler, Device, EmptyScheduler, ErrMsg,
    EventHandler, Mode, Proto, Router, System, Word, BROADCAST_ADDRESS, TR, WRD_EMPTY,
};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct FakeStatusReccmd {
    pub attack_times: Vec<u128>,
    pub success: bool,
    pub word_count: u8,
    pub target: u8,         // the target RT
    pub target_found: bool, // target found in traffic
    pub destination: u8,
    pub warm_up: u128,
}

impl FakeStatusReccmd {
    fn fake_status(&mut self, d: &mut Device) {
        // attack only once
        // self.target_found = false;
        self.attack_times.push(d.clock.elapsed().as_nanos());
        let w = Word::new_malicious_status(self.target);
        d.write(w);
        d.log(
            WRD_EMPTY,
            ErrMsg::MsgAttk(format!("Fake status injected!").to_string()),
        );
        self.success = true;
    }
    pub fn verify(&self, system: &System) -> bool {
        let mut attk_session = false;
        for l in &system.logs {
            if matches!(l.6, ErrMsg::MsgAttk { .. }) {
                attk_session = true;
            }

            if attk_session {
                if l.6 == ErrMsg::MsgEntSte
                    && l.5.attk() == (AttackType::AtkFakeStatusReccmd as u32)
                {
                    println!("{}", format_log(l));
                    return true;
                }
            }

            if l.6 == ErrMsg::MsgBCReady {
                attk_session = false;
            }
        }
        return false;
    }
}

impl EventHandler for FakeStatusReccmd {
    fn on_cmd(&mut self, d: &mut Device, w: &mut Word) {
        if d.clock.elapsed().as_nanos() > self.warm_up && !self.target_found {
            let destination = w.address();
            // if w.address() != self.address {} //This line won't work yet.  TODO: Get our address.
            if self.target != BROADCAST_ADDRESS
                && destination == self.target
                && w.tr() == TR::Receive
            {
                d.log(
                    WRD_EMPTY,
                    ErrMsg::MsgAttk(
                        format!("Attacker>> Target detected (RT{:02})", self.target).to_string(),
                    ),
                );
                self.target_found = true;
                self.word_count = w.dword_count();
                self.destination = w.address();
                d.log(
                    WRD_EMPTY,
                    ErrMsg::MsgAttk(format!("Fake status triggered (after cmd_word)").to_string()),
                );
            } else {
                if w.tr() == TR::Receive && destination != BROADCAST_ADDRESS {
                    d.log(
                        WRD_EMPTY,
                        ErrMsg::MsgAttk(
                            format!("Attacker>> Target detected (RT{:02})", w.address())
                                .to_string(),
                        ),
                    );
                    d.log(
                        WRD_EMPTY,
                        ErrMsg::MsgAttk(
                            format!("Fake status triggered (after cmd_word)").to_string(),
                        ),
                    );
                }
            }
        }
        self.default_on_cmd(d, w);
    }

    fn on_dat(&mut self, d: &mut Device, w: &mut Word) {
        // This takes the place of "intercept_dw" in Michael's code
        if self.target_found && w.data() != 0xffff && self.word_count > 0 {
            self.word_count -= 1;
            if self.word_count == 0 {
                self.fake_status(d);
            }
        }
    }
}

pub fn eval_attack6(w_delays: u128, proto: Proto) -> bool {
    // let mut delays_single = Vec::new();
    let n_devices = 3;
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
    let attk = FakeStatusReccmd {
        attack_times: Vec::new(),
        word_count: 0u8,
        success: false,
        target: 1, // attacking RT address @4
        target_found: false,
        destination: 0u8,
        warm_up: 1_000_000,
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
        AttackType::AtkFakeStatusReccmd,
    );
    sys.go();
    sys.sleep_ms(100);
    sys.stop();
    sys.join();
    let l_router = Arc::clone(&attacker_router);
    return l_router.lock().unwrap().handler.verify(&sys);
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn test_attk_6_b2r_succeed() {
        // 12_000 based on the protocol but there is a probability
        // of success. so here we made it higher
        assert!(eval_attack6(12_000, Proto::BC2RT) == true);
    }
    #[test]
    fn test_attk_6_b2r_failed() {
        assert!(eval_attack6(0, Proto::BC2RT) == false);
    }
}
