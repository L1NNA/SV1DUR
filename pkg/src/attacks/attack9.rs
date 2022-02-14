use crate::sys::{
    AttackType, DefaultEventHandler, DefaultScheduler, Device, ErrMsg, EventHandler, Mode, Proto,
    Router, System, Word, WRD_EMPTY, EmptyScheduler
};

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
        d.log(
            WRD_EMPTY,
            ErrMsg::MsgAttk(
                format!("Attacker>> Injecting a fake status word followed by fake data ...")
                    .to_string(),
            ),
        );
        self.attack_times.push(d.clock.elapsed().as_nanos());
        let w = Word::new_status(self.target);
        d.write(w);
        for _ in 0..self.word_count {
            let w = Word::new_data(0x7171);
            d.log(
                WRD_EMPTY,
                ErrMsg::MsgAttk(format!("Fake Data {} ", w).to_string()),
            );
            d.write(w);
        }
        self.success = true;
    }
}

impl EventHandler for DataCorruptionAttack {
    fn on_cmd(&mut self, d: &mut Device, w: &mut Word) {
        // This function replaces "find_RT_tcmd" from Michael's code
        // We cannot use on_cmd_trx here because that only fires after on_cmd verifies that the address is correct.
        let destination = w.address();
        self.word_count = w.dword_count();
        if destination == self.target && self.target_found == false && w.tr() == 1 {
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

#[allow(dead_code)]
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
                proto: Proto::BC2RT,
                proto_rotate: true,
            },
            // control device-level response
            handler: DefaultEventHandler {},
        };

        if m == 0 {
            sys.run_d(m as u8, Mode::BC, default_router, AttackType::Benign);
        } else {
            sys.run_d(m as u8, Mode::RT, default_router, AttackType::Benign);
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
        handler: DataCorruptionAttack {
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
        attacker_router,
        AttackType::AtkDataCorruptionAttack,
    );
    sys.go();
    sys.sleep_ms(10);
    sys.stop();
    sys.join();
}

#[allow(dead_code)]
pub fn eval_attack9() {
    // let mut delays_single = Vec::new();
    let n_devices = 10;
    // normal device has 4ns delays (while attacker has zero)
    let w_delays = 3000;
    let mut sys = System::new(n_devices as u32, w_delays);

    // the last device is kept for attacker
    for m in 0..n_devices - 1 {
        let default_router = Router {
            // control all communications (bc only)
            scheduler: DefaultScheduler {
                total_device: n_devices - 1,
                target: 0,
                data: vec![1, 2, 3],
                proto: Proto::RT2RT,
                proto_rotate: false,
            },
            // control device-level response
            handler: DefaultEventHandler {},
        };

        if m == 0 {
            sys.run_d(m as u8, Mode::BC, default_router, AttackType::Benign);
        } else {
            sys.run_d(m as u8, Mode::RT, default_router, AttackType::Benign);
        }
    }
    let attacker_router = Router {
        // control all communications (bc only)
        scheduler: EmptyScheduler{},
        // control device-level response
        handler: DataCorruptionAttack {
            attack_times: Vec::new(),
            word_count: 0u8,
            success: false,
            target: 2, // attacking RT address @4
            target_found: false,
        },
    };

    sys.run_d(
        n_devices - 1,
        Mode::RT,
        attacker_router,
        AttackType::AtkDataCorruptionAttack,
    );
    sys.go();
    sys.sleep_ms(10);
    sys.stop();
    sys.join();
}
