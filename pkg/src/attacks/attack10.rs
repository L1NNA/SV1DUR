use crate::sys::{
    format_log, DefaultScheduler, EmptyScheduler,
    Proto, Router, System, WRD_EMPTY, BROADCAST_ADDRESS
};
use crate::devices::Device;
use crate::primitive_types::{AttackType, ErrMsg, Mode, State, Word, TR};
use crate::event_handlers::{EventHandler, DefaultEventHandler};
use std::sync::{Arc, Mutex};

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
        let dword_count = 31; // Maximum number of words.  This will mean the receipient ignores the next 31 messages
        let tr = TR::Receive; // We want to receive because it will sit and wait rather than responding to the BC.
        let w = Word::new_cmd(self.target, dword_count, tr);
        d.log(
            WRD_EMPTY,
            ErrMsg::MsgAttk(format!("Attacker>> Injecting fake command on RT{}", w).to_string()),
        );
        d.write(w);
        self.success = true;
    }

    pub fn verify(&self, system: &System) -> bool {
        let last_log = &system.logs[system.logs.len() - 1];
        // target is waiting for data instead.
        return last_log.3 == self.target && last_log.4 == State::AwtData;
    }
}

impl EventHandler for CommandInvalidationAttack {
    fn on_cmd(&mut self, d: &mut Device, w: &mut Word) {
        // This function replaces "find_RT_tcmd" from Michael's code
        // We cannot use on_cmd_trx here because that only fires after on_cmd verifies that the address is correct.
        let destination = w.address();
        if destination == self.target && self.target_found == false && w.tr() == TR::Transmit {
            d.log(
                *w,
                ErrMsg::MsgAttk(
                    format!("Attacker>> Target detected(RT{})", self.target).to_string(),
                ),
            );
            self.target_found = true;
            self.inject(d);
        }
        self.default_on_cmd(d, w);
    }
}

pub fn eval_attack10(w_delays: u128, proto: Proto) -> bool {
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
    let attk = CommandInvalidationAttack {
        attack_times: Vec::new(),
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
        AttackType::AtkCommandInvalidationAttack,
    );
    sys.go();
    sys.sleep_ms(100);
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
    fn test_attk_10_r2r_succeed() {
        // 12_000 based on the protocol but there is a probability
        // of success. so here we made it higher
        assert!(eval_attack10(40_000, Proto::RT2RT) == true);
    }
    #[test]
    fn test_attk_10_r2r_failed() {
        assert!(eval_attack10(0, Proto::RT2RT) == false);
    }
    #[test]
    fn test_attk_10_r2b_succeed() {
        assert!(eval_attack10(40_000, Proto::RT2BC) == true);
    }
    #[test]
    fn test_attk_10_r2b_failed() {
        assert!(eval_attack10(0, Proto::RT2BC) == false);
    }
}
