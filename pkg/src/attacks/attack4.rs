use crate::sys::{
    AttackType, DefaultEventHandler, DefaultScheduler, Device, ErrMsg, EventHandler, Mode, Proto,
    Router, State, System, Word, WRD_EMPTY, TR, BROADCAST_ADDRESS
};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct MITMAttackOnRTs {
    pub attack_times: Vec<u128>,
    pub success: bool,
    pub word_count: u8,
    pub injected_words: u8,
    pub target_src: u8,
    pub target_dst: u8,
    pub target_dst_found: bool, // target found in traffic
    pub target_src_found: bool,
    pub done: bool,
}

impl MITMAttackOnRTs {
    fn start_mitm(&mut self, d: &mut Device) {
        d.log(
            WRD_EMPTY,
            ErrMsg::MsgAttk(format!("Starting MITM attack...").to_string()),
        );
        self.attack_times.push(d.clock.elapsed().as_nanos());
        d.set_state(State::Off);
        let word_count = self.injected_words;
        let tr = TR::Receive;
        let mut w = Word::new_cmd(self.target_src, word_count, tr);
        d.write(w);
        w.set_address(self.target_dst);
        w.set_tr(1);
        d.write(w);
        self.done = true;
        //sleep(time_next_attack) // figure out how to add delays // default is 10
        d.set_state(State::Idle);
    }
}

impl EventHandler for MITMAttackOnRTs {
    fn on_cmd(&mut self, d: &mut Device, w: &mut Word) {
        if !(self.target_src_found && self.target_dst_found) && !self.done {
            if w.tr() == TR::Receive && !self.target_dst_found && w.address() != BROADCAST_ADDRESS {
                self.target_dst = w.address();
                self.target_dst_found = true;
                self.word_count = w.dword_count();
                if self.injected_words == 0 {
                    self.injected_words = w.dword_count();
                }
                d.log(
                    WRD_EMPTY,
                    ErrMsg::MsgAttk(
                        format!("Atttacker>> Target dst identified (RT{})", self.target_dst)
                            .to_string(),
                    ),
                );
            } else if w.tr() == TR::Transmit && !self.target_src_found && w.address() != BROADCAST_ADDRESS {
                self.target_src = w.address();
                self.target_src_found = true;
                d.log(
                    WRD_EMPTY,
                    ErrMsg::MsgAttk(
                        format!("Atttacker>> Target src identified (RT{})", self.target_src)
                            .to_string(),
                    ),
                );
            }
        }
        self.default_on_cmd(d, w);
    }

    fn on_sts(&mut self, d: &mut Device, w: &mut Word) {
        // This replaces "getReady_for_MITM" from Michael's code
        if self.target_src == w.address() && !self.done {
            if self.target_dst_found && self.target_src_found {
                //sleep(self.delay);
                self.start_mitm(d);
            }
        } else if self.target_src == w.address() && self.done {
            d.log(
                WRD_EMPTY,
                ErrMsg::MsgAttk(format!(
                    "Attacker>> Man in the Middle Successfully Completed!"
                )),
            );
            self.success = true;
            self.target_src_found = false;
            self.target_dst_found = false;
            self.done = false;
        }
    }
}

#[allow(dead_code)]
pub fn test_attack4() {
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
        handler: MITMAttackOnRTs {
            attack_times: Vec::new(),
            word_count: 0u8,
            injected_words: 0u8,
            success: false,
            target_src: 0u8,
            target_dst: 0u8,
            target_dst_found: false, // target found in traffic
            target_src_found: false,
            done: false,
        },
    };

    sys.run_d(
        n_devices - 1,
        Mode::RT,
        Arc::new(Mutex::new(attacker_router)),
        AttackType::AtkMITMAttackOnRTs,
    );
    sys.go();
    sys.sleep_ms(10);
    sys.stop();
    sys.join();
}
