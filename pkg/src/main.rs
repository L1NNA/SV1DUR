mod attack0;
mod attack1;
mod attack9;
mod sys;
use attack0::test_attack0;
use attack1::test_attack1;
use attack9::test_attack9;
use sys::test_default;

fn main() {
    test_default();
    test_attack0();
    test_attack1();
    test_attack9();
}
