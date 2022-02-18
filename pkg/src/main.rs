mod attacks;
mod risk;
mod sys;
use risk::eval_all;
#[allow(unused_imports)]
use sys::{eval_sys, Proto};

fn main() {
    // eval_all();
    // eval_sys(0, 3, Proto::RT2RT, true);
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
    let system = eval_sys(0, 4, Proto::RT2RT, true);
}
