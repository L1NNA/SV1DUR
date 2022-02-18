use crate::sys::{
    format_log, AttackType, DefaultEventHandler, DefaultScheduler, Device, EmptyScheduler, ErrMsg,
    EventHandler, Mode, Proto, Router, State, System, Word, WRD_EMPTY,
};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct DataCorruptionAttack {
    pub attack_times: Vec<u128>,
    pub success: bool,
    pub word_count: u8,
    pub target: u8,         // the target RT
    pub target_found: bool, // target found in traffic
}

impl DataCorruptionAttack {
    pub fn inject(&mut self, d: &mut Device) {
        self.attack_times.push(d.clock.elapsed().as_nanos());
        let w = Word::new_status(self.target);
        d.write(w);
        for _ in 0..self.word_count {
            let w = Word::new_data(0x7171);
            // make it faster
            // d.log(
            //     WRD_EMPTY,
            //     ErrMsg::MsgAttk(format!("Fake Data {} ", w).to_string()),
            // );
            d.write(w);
        }
        self.success = true;
        d.log(
            WRD_EMPTY,
            ErrMsg::MsgAttk(
                format!(
                    "Attacker>> Injected a fake status word followed by {} fake data ...",
                    self.word_count
                )
                .to_string(),
            ),
        );
    }

    pub fn verify(&self, system: &System) -> bool {
        let mut recieved_faked = 0;
        for l in &system.logs {
            if l.6 == ErrMsg::MsgBCReady {
                recieved_faked = 0;
            }
            if l.6 == ErrMsg::MsgEntDat
                && l.5.attk() == (AttackType::AtkDataCorruptionAttack as u32)
            {
                // println!("{} {}/{}", format_log(&l), recieved_faked, self.word_count);
                recieved_faked += 1;
                if recieved_faked == self.word_count {
                    return true;
                }
            }
        }
        return false;
    }
}

impl EventHandler for DataCorruptionAttack {
    fn on_cmd(&mut self, d: &mut Device, w: &mut Word) {
        // This function replaces "find_RT_tcmd" from Michael's code
        // We cannot use on_cmd_trx here because that only fires after on_cmd verifies that the address is correct.
        let destination = w.address();
        if destination == self.target && self.target_found == false && w.tr() == 1 {
            self.word_count = w.dword_count();
            // do we need the sub address?
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

pub fn eval_attack9(w_delays: u128, proto: Proto) -> bool {
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
    let attk = DataCorruptionAttack {
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
        AttackType::AtkDataCorruptionAttack,
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
    fn test_attk_9_r2r_succeed() {
        // 12_000 based on the protocol but there is a probability
        // of success. so here we made it higher
        assert!(eval_attack9(40_000, Proto::RT2RT) == true);
    }
    #[test]
    fn test_attk_9_r2r_failed() {
        assert!(eval_attack9(0, Proto::RT2RT) == false);
    }
    #[test]
    fn test_attk_9_r2b_succeed() {
        assert!(eval_attack9(40_000, Proto::RT2BC) == true);
    }
    #[test]
    fn test_attk_9_r2b_failed() {
        assert!(eval_attack9(0, Proto::RT2BC) == false);
    }
}
