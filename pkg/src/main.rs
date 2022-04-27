mod attacks;
mod risk;
mod sys_bus;
mod sys_flight;
#[allow(unused_imports)]
use attacks::eval_attack_controller;
use risk::eval_all;
use std::fs;
use std::path::Path;
use sys_bus::{eval_sys, AttackType, Proto};
use sys_flight::fighter_md::eval_fighter_sim;

fn main() {
    // eval_all();
    eval_sys(4_000, 6, Proto::RT2RT, false);
    // eval_fighter_sim("sample_data.sqlite", 4_000, 10_000, AttackType::Benign);
    // eval_fighter_sim("sample_data.sqlite", 4_000, 10_000, AttackType::AtkCollisionAttackAgainstTheBus);
    let paths = fs::read_dir("./msf/").unwrap();
    for path in paths {
        let p = path
            .unwrap()
            .path()
            .into_os_string()
            .to_str()
            .unwrap()
            .to_owned();
        let ds_name = Path::new(&p[..])
            .with_extension("")
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        for attack_index in 0..11 {
            println!(
                "Proessing file: {:?} with Attack {:?}",
                p,
                AttackType::from(attack_index),
            );
            eval_fighter_sim(
                &p[..],
                4_000,
                // 10_000,
                0, // max run time
                attack_index.into(),
                format!("{}_{}", ds_name, attack_index),
            );
        }
        break;
    }

    // for attack_index in 1..2 {
    //     // eval_fighter_sim("sample_data.sqlite", 0, 200_000, attack_index.into());
    //     eval_fighter_sim(
    //         "sample_data.sqlite",
    //         4_000,
    //         60_000,
    //         AttackType::Benign,
    //     );
    //     // eval_attack_controller(0, 5, Proto::RT2RT, false, attack_index);
    // }

    // test_attack0();
    // test_attack1();
    // test_attack2();
    // test_attack3();
    // test_attack4();
    // test_attack5();
    // test_attack6();
    // test_attack7();
    // test_attack8();
    // test_attack9();
    // eval_attack9();
}
