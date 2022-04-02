pub mod attack1;
pub mod attack10;
pub mod attack2;
pub mod attack3;
pub mod attack4;
pub mod attack5;
pub mod attack6;
pub mod attack7;
pub mod attack8;
pub mod attack9;

use crate::sys::{
    format_log, AttackType, DefaultBCEventHandler, DefaultEventHandler, Device, ErrMsg,
    EventHandler, EventHandlerEmitter, Mode, Proto, State, System, Word, TR, WRD_EMPTY,
};
use attack1::CollisionAttackAgainstTheBus;
use attack10::CommandInvalidationAttack;
use attack2::CollisionAttackAgainstAnRT;
use attack3::DataThrashingAgainstRT;
use attack4::MITMAttackOnRTs;
use attack5::ShutdownAttackRT;
use attack6::FakeStatusReccmd;
use attack7::FakeStatusTrcmd;
use attack8::DesynchronizationAttackOnRT;
use attack9::DataCorruptionAttack;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub struct AttackController {
    pub current_attack: AttackType,
    pub emitter: Arc<Mutex<EventHandlerEmitter>>,
}

impl AttackController {
    pub fn sabotage(&mut self, attack_type: AttackType, source: u8, target: u8) {
        let attack: Box<dyn EventHandler> = match attack_type {
            AttackType::Benign => Box::new(DefaultEventHandler {}),
            AttackType::AtkCollisionAttackAgainstTheBus => Box::new(CollisionAttackAgainstTheBus {
                nwords_inj: 5,
                started: 0,
                success: false,
            }),
            AttackType::AtkCollisionAttackAgainstAnRT => Box::new(CollisionAttackAgainstAnRT {
                nwords_inj: 0,
                attack_times: Vec::new(),
                success: false,
                target: target, // attacking RT address @5
                wc_n: 0,        // expected word count (intercepted)
                target_found: false,
            }),
            AttackType::AtkDataThrashingAgainstRT => Box::new(DataThrashingAgainstRT {
                attack_times: Vec::new(),
                word_count: 0u8,
                success: false,
                target: target, // attacking RT address @4
                target_found: false,
            }),
            AttackType::AtkMITMAttackOnRTs => Box::new(MITMAttackOnRTs {
                attack_times: Vec::new(),
                word_count: 0u8,
                injected_words: 0u8,
                success: false,
                target_src: source,
                target_dst: target,
                target_dst_found: false, // target found in traffic
                target_src_found: false,
                done: false,
            }),
            AttackType::AtkShutdownAttackRT => Box::new(ShutdownAttackRT {
                attack_times: Vec::new(),
                word_count: 0u8,
                success: false,
                target: target, // attacking RT address @4
                target_found: false,
            }),
            AttackType::AtkFakeStatusReccmd => Box::new(FakeStatusReccmd {
                attack_times: Vec::new(),
                word_count: 0u8,
                success: false,
                target: target, // attacking RT address @4
                target_found: false,
                destination: 0u8,
                warm_up: 1_000_000,
            }),
            AttackType::AtkFakeStatusTrcmd => Box::new(FakeStatusTrcmd {
                attack_times: Vec::new(),
                success: false,
                target: target, // attacking RT address @4
                target_found: false,
                warm_up: 1_000_000,
            }),
            AttackType::AtkDesynchronizationAttackOnRT => Box::new(DesynchronizationAttackOnRT {
                attack_times: Vec::new(),
                word_count: 0u8,
                success: false,
                flag: 0,
                target: target, // attacking RT address @4
                target_found: false,
            }),
            AttackType::AtkDataCorruptionAttack => Box::new(DataCorruptionAttack {
                attack_times: Vec::new(),
                word_count: 0u8,
                success: false,
                target: target, // attacking RT address @4
                target_found: false,
            }),
            AttackType::AtkCommandInvalidationAttack => Box::new(CommandInvalidationAttack {
                attack_times: Vec::new(),
                success: false,
                target: 2, // attacking RT address @4
                target_found: false,
            }),
        };
        self.current_attack = attack_type;
        self.emitter.lock().unwrap().handler = attack;
    }
}

pub fn eval_attack_controller(
    w_delays: u128,
    n_devices: u8,
    proto: Proto,
    proto_rotate: bool,
    attack: i32,
) -> System {
    // let n_devices = 3;
    // let w_delays = w_delays;
    let mut sys = System::new(n_devices as u32, w_delays);
    for m in 0..(n_devices - 2) {
        // let (s1, r1) = bounded(64);
        // s_vec.lock().unwrap().push(s1);
        if m == 0 {
            sys.run_d(
                m as u8,
                Mode::BC,
                Arc::new(Mutex::new(EventHandlerEmitter {
                    handler: Box::new(DefaultBCEventHandler {
                        total_device: n_devices - 2,
                        target: 0,
                        data: vec![1, 2, 3],
                        proto: proto,
                        proto_rotate: proto_rotate,
                    }),
                })),
                false,
            );
        } else {
            sys.run_d(
                m as u8,
                Mode::RT,
                Arc::new(Mutex::new(EventHandlerEmitter {
                    handler: Box::new(DefaultEventHandler {}),
                })),
                false,
            );
        }
    }

    sys.run_d(
        n_devices - 2,
        Mode::BM,
        Arc::new(Mutex::new(EventHandlerEmitter {
            handler: Box::new(DefaultEventHandler {}),
        })),
        false,
    );

    let mut attack_controller = AttackController {
        current_attack: AttackType::Benign,
        emitter: Arc::new(Mutex::new(EventHandlerEmitter {
            handler: Box::new(DefaultEventHandler {}),
        })),
    };

    sys.run_d(
        n_devices - 1,
        Mode::RT,
        Arc::clone(&attack_controller.emitter),
        true,
    );

    sys.go();
    sys.sleep_ms(1);

    // attack_controller.sabotage(1.into(), 1, 2);
    // sys.sleep_ms(50);

    // attack_controller.sabotage(2.into(), 1, 2);
    // sys.sleep_ms(1000);
    attack_controller.sabotage(attack.into(), 1, 2);
    sys.sleep_ms(50);
    // attack_controller.sabotage(1.into(), 1, 2);
    // sys.sleep_ms(50);
    // attack_controller.sabotage(2.into(), 1, 2);
    // sys.sleep_ms(50);
    // for attack_index in 1..11 {
    //     let attack_type: AttackType = (11 - attack_index).into();
    //     println!("Running {:?}...", attack_type);
    //     attack_controller.sabotage(attack_type, 1, 2);
    //     sys.sleep_ms(100);
    // }
    println!("Done...");
    sys.stop();
    sys.join();
    let mut result = HashMap::new();
    for l in &sys.logs {
        if l.5.attk() != 0 {
            // println!("{} {}/{}", format_log(&l), recieved_faked, self.word_count);
            *result.entry(l.5.attk()).or_insert(0) += 1;
        }
    }
    println!("{:?}", result);

    return sys;
}
