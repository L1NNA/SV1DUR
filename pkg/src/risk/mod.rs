use crate::attacks::attack10::eval_attack10;
use crate::attacks::attack3::eval_attack3;
use crate::attacks::attack6::eval_attack6;
use crate::attacks::attack7::eval_attack7;
use crate::attacks::attack9::eval_attack9;
use crate::sys::{
    format_log, AttackType, DefaultEventHandler, DefaultScheduler, Device, EmptyScheduler, ErrMsg,
    EventHandler, Mode, Proto, Router, State, System, Word, WRD_EMPTY,
};
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::thread;
/*
We have 8 attack vectors:
(1) MITM
(2) Fake-command injection
(3) Fake data-word injection
(4) Data Corruption
(5) Command Invalidation
(6) Data trashing
(7) Status-word manipulation, and
(8) random word generation.

Attack_01.py is the collision attack against the bus, which is AV8.
Attack_02.py is the collision attack against an RT, which is AV8 (but against an RT instead of the entire bus).
*Attack_03.py is the data trashing attack, which is AV6 (BC2RT and RT2RT).
Attack_04.py is MITM attack, which is AV1.
Attack_05.py is a shutdown attack on RT, which is AV2.
*Attack_06.py is fake status command against receive command, which is AV7 (BC2RT).
*Attack_07.py is fake status command against transmit command, which is AV7 (RT2BC).
Attack_08.py is desynchronization attack against an RT, which is AV2.
* Attack_09.py is data corruption attack, which is AV4 (RT2BC and RT2RT).
* Attack_10.py is a command invalidation attack, which is AV5 (RT2BC and RT2RT).

*/
#[derive(Debug)]
pub enum AttackVector {
    AV4_RT2BC, //attack 9
    AV4_RT2RT,
    AV5_RT2BC, //attack 10
    AV5_RT2RT,
    AV6_BC2RT, //attack 3
    AV6_RT2RT,
    AV7_BC2RT, //attack 6
    AV7_RT2BC, //attack 7
}

pub fn eval_attack_prob(attack_vector: AttackVector) -> (AttackVector, Vec<u128>, Vec<f32>) {
    let num_sims = 100;
    let start: u128 = 4_000;
    let end: u128 = 12_001;
    let steps = 100;

    let step_size = (end - start - 1) / steps;
    let delays = (start..end).step_by(step_size as usize).collect::<Vec<_>>();
    // println!("{:?}", delays);
    let mut probs = vec![];
    let m = MultiProgress::new();
    let sty = ProgressStyle::default_bar()
        .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")
        .progress_chars("##-");

    let pbs = ProgressBar::new(num_sims as u64);
    pbs.set_style(sty.clone());
    pbs.set_draw_target(ProgressDrawTarget::stdout());

    for delay in delays.clone() {
        let mut prob = 0.0;
        for c in 0..num_sims {
            pbs.set_message(format!(
                "Trial {:03} Prob {:.4} Delays {}/{}",
                c,
                prob / (c as f32),
                delay,
                end,
            ));
            pbs.inc(1);
            let mut success = false;
            match attack_vector {
                AttackVector::AV4_RT2BC => {
                    success = eval_attack9(delay, Proto::RT2BC);
                }
                AttackVector::AV4_RT2RT => {
                    success = eval_attack9(delay, Proto::RT2RT);
                }
                AttackVector::AV5_RT2BC => {
                    success = eval_attack10(delay, Proto::RT2BC);
                }
                AttackVector::AV5_RT2RT => {
                    success = eval_attack10(delay, Proto::RT2RT);
                }
                AttackVector::AV6_BC2RT => {
                    success = eval_attack3(delay, Proto::BC2RT);
                }
                AttackVector::AV6_RT2RT => {
                    success = eval_attack3(delay, Proto::RT2RT);
                }
                AttackVector::AV7_BC2RT => {
                    success = eval_attack6(delay, Proto::BC2RT);
                }
                AttackVector::AV7_RT2BC => {
                    success = eval_attack7(delay, Proto::RT2BC);
                }
            }
            if success {
                prob += 1.0;
            }
        }
        prob /= num_sims as f32;
        probs.push(prob);
        pbs.reset();
    }
    pbs.finish();
    println!("{:?} {:?}", attack_vector, probs);
    return (attack_vector, delays, probs);
}

pub fn eval_all() {
    let mut result = vec![];
    // result.push(eval_attack_prob(AttackVector::AV4_RT2BC));
    // result.push(eval_attack_prob(AttackVector::AV4_RT2RT));
    result.push(eval_attack_prob(AttackVector::AV5_RT2BC));
    result.push(eval_attack_prob(AttackVector::AV5_RT2RT));
    // result.push(eval_attack_prob(AttackVector::AV6_BC2RT));
    // result.push(eval_attack_prob(AttackVector::AV6_RT2RT));
    // result.push(eval_attack_prob(AttackVector::AV7_BC2RT));
    // result.push(eval_attack_prob(AttackVector::AV7_RT2BC));

    let mut delays = vec![];
    for r in result {
        // let joined: String = r.2.iter().map(|&id| id.to_string() + ",").collect();
        println!("\"{:?}\":{:?}", r.0, r.2);
        delays = r.1;
    }
    println!("\"x=\"{:?}", delays)
}
