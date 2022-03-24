use crate::sys::{Router, System};
use crate::schedulers::{DefaultScheduler, EmptyScheduler, Proto};
use crate::devices::Device;
use crate::primitive_types::{AttackType, ErrMsg, Mode, State, Word, TR, WRD_EMPTY, BROADCAST_ADDRESS};
use crate::event_handlers::{EventHandler, DefaultEventHandler};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct FakeStatusTrcmd {
    pub attack_times: Vec<u128>,
    pub success: bool,
    pub target: u8,         // the target RT
    pub target_found: bool, // target found in traffic
    pub warm_up: u128,
}

impl FakeStatusTrcmd {
    fn fake_status(&mut self, d: &mut Device) {
        self.attack_times.push(d.clock.elapsed().as_nanos());
        let w = Word::new_malicious_status(self.target);
        d.write(w);
        d.log(
            WRD_EMPTY,
            ErrMsg::MsgAttk(format!("Fake status injected!").to_string()),
        );
        // attack only once
        // self.target_found = false;
        self.success = true;
    }

    pub fn verify(&self, system: &System) -> bool {
        let mut attk_session = false;
        for l in &system.logs {
            if matches!(l.6, ErrMsg::MsgAttk { .. }) {
                attk_session = true;
            }
            // dropped message during attack session
            if attk_session {
                if l.6 == ErrMsg::MsgEntSteDrop {
                    if l.5.attk() == (AttackType::AtkFakeStatusTrcmd as u32) {
                        return false;
                    } else {
                        return true;
                    }
                }
            }
            if l.6 == ErrMsg::MsgBCReady {
                attk_session = false;
            }
        }
        return false;
    }
}

impl EventHandler for FakeStatusTrcmd {
    fn on_cmd(&mut self, d: &mut Device, w: &mut Word) {
        if d.clock.elapsed().as_nanos() > self.warm_up && !self.target_found {
            let destination = w.address();
            // if w.address() != self.address {} //This line won't work yet.  TODO: Get our address.
            if self.target != BROADCAST_ADDRESS {
                if destination == self.target && w.tr() == TR::Transmit && !self.target_found {
                    d.log(
                        WRD_EMPTY,
                        ErrMsg::MsgAttk(
                            format!("Attacker>> Target detected (RT{})", self.target).to_string(),
                        ),
                    );
                    self.target_found = true;
                    d.log(
                        WRD_EMPTY,
                        ErrMsg::MsgAttk(
                            format!("Sending fake status word (after tr_cmd_word)").to_string(),
                        ),
                    );
                    self.fake_status(d);
                }
            } else {
                if w.tr() == TR::Transmit && destination != BROADCAST_ADDRESS {
                    d.log(
                        WRD_EMPTY,
                        ErrMsg::MsgAttk(format!("Sending fake status word (after tr_cmd_word)")),
                    );
                    self.fake_status(d);
                }
            }
        }
        self.default_on_cmd(d, w);
    }
}

pub fn eval_attack7(w_delays: u128, proto: Proto) -> bool {
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
    let attk = FakeStatusTrcmd {
        attack_times: Vec::new(),
        success: false,
        target: 1, // attacking RT address @4
        target_found: false,
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
        AttackType::AtkFakeStatusTrcmd,
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
    fn test_attk_7_r2b_succeed() {
        // 12_000 based on the protocol but there is a probability
        // of success. so here we made it higher
        assert!(eval_attack7(40_000, Proto::RT2BC) == true);
    }
    #[test]
    fn test_attk_7_r2b_failed() {
        assert!(eval_attack7(0, Proto::RT2BC) == false);
    }
}
