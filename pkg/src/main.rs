mod attack0;
mod attack1;
mod sys;
use attack0::test_attack0;
use attack1::test_attack1;
use sys::test_default;

fn main() {
    // test_default();
    test_attack1();
}
