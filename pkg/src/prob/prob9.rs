use crate::attacks::attack9::DataCorruptionAttack;
use crate::sys::{
    AttackType, DefaultEventHandler, DefaultScheduler, Device, EmptyScheduler, ErrMsg,
    EventHandler, Mode, Proto, Router, System, Word, WRD_EMPTY,
};

#[allow(dead_code)]
pub fn eval_attack9() {
    // let mut delays_single = Vec::new();
    let n_devices = 4;
    // normal device has 4ns delays (while attacker has zero)
    let w_delays = 2000;
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
