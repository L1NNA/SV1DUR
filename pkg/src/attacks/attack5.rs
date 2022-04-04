use crate::sys_bus::{
    AttackType, DefaultBCEventHandler, DefaultEventHandler, Device, ErrMsg, EventHandler,
    EventHandlerEmitter, Mode, Proto, State, System, Word, TR, WRD_EMPTY,
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
        let mode_code = 4;
        let tr = TR::Receive;
        self.attack_times.push(d.clock.elapsed().as_nanos());
        let mut w = Word::new_cmd(self.target, mode_code, tr);
        w.set_mode(1);
        d.write(w);
        self.success = true;
        // d.set_state(State::Off); // Not sure what's going on here yet.  TODO come back to this.
    }
}

impl EventHandler for ShutdownAttackRT {
    fn get_attk_type(&self) -> AttackType {
        AttackType::AtkShutdownAttackRT
    }
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
    fn verify(&mut self, system: &System) -> bool {
        for d in &system.devices {
            let device = d.lock().unwrap();
            println!("{}", device);
            if device.state == State::Off {
                return true;
            }
        }
        return false;
    }
}

pub fn eval_attack5(w_delays: u128, proto: Proto) -> bool {
    // let mut delays_single = Vec::new();
    let n_devices = 4;
    // normal device has 4ns delays (while attacker has zero)
    // let w_delays = 40000;
    let mut sys_bus = System::new(n_devices as u32, w_delays);

    // the last device is kept for attacker
    for m in 0..n_devices - 1 {
        if m == 0 {
            sys_bus.run_d(
                m as u8,
                Mode::BC,
                Arc::new(Mutex::new(EventHandlerEmitter {
                    handler: Box::new(DefaultBCEventHandler {
                        total_device: n_devices - 1,
                        target: 0,
                        data: vec![1, 2, 3],
                        proto: proto,
                        proto_rotate: false,
                    }),
                })),
                false,
            );
        } else {
            sys_bus.run_d(
                m as u8,
                Mode::RT,
                Arc::new(Mutex::new(EventHandlerEmitter {
                    handler: Box::new(DefaultEventHandler {}),
                })),
                false,
            );
        }
    }
    let attk = ShutdownAttackRT {
        attack_times: Vec::new(),
        word_count: 0u8,
        success: false,
        target: 2, // attacking RT address @4
        target_found: false,
    };
    let attacker_router = Arc::new(Mutex::new(EventHandlerEmitter {
        handler: Box::new(attk),
    }));

    sys_bus.run_d(n_devices - 1, Mode::RT, Arc::clone(&attacker_router), true);
    sys_bus.go();
    sys_bus.sleep_ms(100);
    sys_bus.stop();
    sys_bus.join();
    let l_router = Arc::clone(&attacker_router);
    return l_router.lock().unwrap().handler.verify(&sys_bus);
    // let l_attk = l_router.unwrap().handler;
    // .lock().unwr();
    // return l_router.unwrap().handler.verify(&devices, &logs);
    // return handler.verify
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn test_attk_5_r2r_succeed() {
        // 12_000 based on the protocol but there is a probability
        // of success. so here we made it higher
        assert!(eval_attack5(40_000, Proto::RT2RT) == true);
    }
    #[test]
    fn test_attk_5_b2r_succeed() {
        assert!(eval_attack5(40_000, Proto::BC2RT) == true);
    }
    #[test]
    fn test_attk_5_r2b_succeed() {
        assert!(eval_attack5(40_000, Proto::RT2BC) == true);
    }
}
