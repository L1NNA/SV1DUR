mod attack0;
mod attack1;
mod attack6;
mod attack7;
mod attack8;
mod attack9;
mod sys;
use attack0::test_attack0;
use attack1::test_attack1;
use attack6::test_attack6;
use attack7::test_attack7;
use attack8::test_attack8;
use attack9::test_attack9;
use sys::test_default;

fn main() {
    // test_default();
    // test_attack0();
    // test_attack1();
    test_attack6();
    // test_attack7();
    // test_attack8();
    // test_attack9();
}
