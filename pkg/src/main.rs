mod attacks;
mod risk;
mod sim;
mod sys;
#[allow(unused_imports)]
use attacks::eval_attack_controller;
use risk::eval_all;
use sim::eval_fighter_sim;
use sys::{eval_sys, AttackType, Proto};

fn main() {
    // eval_all();
    // eval_sys(0, 6, Proto::RT2RT, false);
    eval_fighter_sim(
        "sample_data.sqlite",
        0,
        500,
        AttackType::AtkDataCorruptionAttack,
    )

    // for attack_index in 9..10 {
    //     eval_attack_controller(0, 5, Proto::RT2RT, false, attack_index);
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
