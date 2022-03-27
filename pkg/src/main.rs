#![allow(unused)]
mod attacks;
mod risk;
mod sys;
mod simulation;
mod primitive_types;
mod event_handlers;
mod devices;
mod terminals;
#[allow(unused)]
use risk::eval_all;
#[allow(unused_imports)]
use attacks::{
    test_attack1, eval_attack10, test_attack2, eval_attack3, test_attack4, eval_attack5,
    eval_attack6, eval_attack7, test_attack8, eval_attack9,
};
use crossbeam_channel::{bounded};
#[allow(unused_imports)]
use sys::{System};
use std::time::{Instant};
mod schedulers; //::{Address, MsgPri, HercScheduler};
#[allow(unused_imports)]
use schedulers::{FighterScheduler, Proto, HercScheduler};
use primitive_types::{Address, AttackType, Mode, State, Word, ErrMsg, TR, BROADCAST_ADDRESS};
use devices::{Device, format_log};
use simulation::{fighter_simulation, extract_contents};
use sys::{eval_sys};
use terminals::{ComponentInfo, SplitInt};
use std::collections::LinkedList;
// use libc::nice;

#[allow(unused)]
fn test_address_functions() {
    for (src, dst) in [
            (Address::FlightControls,   Address::Trim),
            (Address::Trim,             Address::FlightControls),
            (Address::FlightControls,   Address::Flaps),
            (Address::Flaps,            Address::FlightControls),
            (Address::FlightControls,   Address::Engine),
            (Address::Engine,           Address::FlightControls),
            (Address::FlightControls,   Address::LandingGear),
            (Address::LandingGear,      Address::FlightControls),
            (Address::FlightControls,   Address::Weapons),
            (Address::Weapons,          Address::FlightControls),
            // Without Feedback
            (Address::FlightControls, Address::Rudder),
            (Address::FlightControls, Address::Ailerons),
            (Address::FlightControls, Address::Elevators),
            (Address::FlightControls, Address::Slats),
            (Address::FlightControls, Address::Spoilers),
            (Address::FlightControls, Address::Brakes),
            //Sensors
            (Address::Fuel,         Address::FlightControls),
            (Address::Heading,      Address::FlightControls),
            (Address::Altimeter,    Address::FlightControls),
            (Address::Positioning,  Address::FlightControls),
            (Address::Pitch,        Address::FlightControls),
        ] {
        println!("src: {:?}, dst: {:?}", src, dst);
        println!("priority: {:?}", src.priority(&dst));
        println!("repeating: {:?}", src.repeat_function(&dst));
        if src.repeat_function(&dst) {
            println!("Repeat freq: {:?}Hz", 1.0/(src.priority(&dst).delay() as f32/1_000_000_000.0));
        }
        println!("word_count: {:?}", src.word_count(&dst));
        println!("------------------")
    }
}

#[allow(unused)]
fn test_herc_scheduler() {
    let (_trans, recv) = bounded(512);
    let mut bc = Device{
        fake: false,
        atk_type: AttackType::Benign,
        ccmd: 0,
        mode: Mode::BC,
        state: State::Idle,
        error_bit: false,
        service_request: false,
        memory: Vec::new(),
        number_of_current_cmd: 0,
        in_brdcst: false,
        address: Address::Engine as u8,
        id: Address::Engine as u32,
        dword_count: 0,
        dword_count_expected: 0,
        clock: Instant::now(),
        logs: Vec::new(),
        transmitters: Vec::new(),
        read_queue: Vec::new(),
        write_queue: LinkedList::new(),
        write_delays: 0,
        receiver: recv,
        delta_t_avg: 0,
        delta_t_start: 0,
        delta_t_count: 0,
    };
    let mut scheduler = HercScheduler::new();
    let mut output: String = String::new();
    for _ in 0..200 {
        if let Some(new_str) = scheduler.on_bc_ready(&mut bc){
            output = format!("{}{}", output, new_str);
        }
    }
    println!("{}", output);
}

fn test_fighter_scheduler() {
    let (_trans, recv) = bounded(512);
    let mut bc = Device{
        fake: false,
        atk_type: AttackType::Benign,
        ccmd: 0,
        mode: Mode::BC,
        state: State::Idle,
        error_bit: false,
        service_request: false,
        memory: Vec::new(),
        number_of_current_cmd: 0,
        in_brdcst: false,
        address: Address::BusControl as u8,  // Currently does not 
        id: Address::BusControl as u32,
        dword_count: 0,
        dword_count_expected: 0,
        clock: Instant::now(),
        logs: Vec::new(),
        transmitters: Vec::new(),
        read_queue: Vec::new(),
        write_queue: LinkedList::new(),
        write_delays: 0,
        receiver: recv,
        delta_t_avg: 0,
        delta_t_start: 0,
        delta_t_count: 0,
    };
    let mut scheduler = FighterScheduler::new();
    // let mut output: String = String::new();
    for _ in 0..200 {
        // if let Some(new_str) = 
            scheduler.on_bc_ready(&mut bc);
            // {
            //     output = format!("{}{}", output, new_str);
            // }

    }
    // println!("{}", output);
}

pub fn test_message_timing() {
    let (_trans, recv) = bounded(512);
    let mut device_obj = Device {
        fake: false,
        atk_type: AttackType::Benign,
        ccmd: 0,
        state: State::Idle,
        error_bit: false,
        service_request: false,
        mode: Mode::RT,
        memory: Vec::new(),
        logs: Vec::new(),
        number_of_current_cmd: 0,
        in_brdcst: false,
        address: Address::FlightControls as u8,
        id: Address::FlightControls as u32,
        dword_count: 0,
        dword_count_expected: 0,
        clock: Instant::now(),
        transmitters: Vec::new(),
        write_queue: LinkedList::new(),
        read_queue: Vec::new(),
        receiver: recv,
        delta_t_avg: 0,
        delta_t_count: 0,
        delta_t_start: 0,
        write_delays: 0,
    };
    let state_start = device_obj.clock.elapsed().as_nanos();
    device_obj.set_state(State::Idle);
    let state_end = device_obj.clock.elapsed().as_nanos();
    let mut logs: Vec<(u128, Mode, u32, u8, State, Word, ErrMsg, u128)> = Vec::new();
    let start = device_obj.clock.elapsed().as_nanos();
    device_obj.act_rt2rt(1, 4, 18);
    let end = device_obj.clock.elapsed().as_nanos();
    println!("State change execution time: {:}-{:}={:}", state_end, state_start, state_end-state_start);
    println!("rt2rt execution time: {:}-{:}={:}", end, start, end-start);
    for log_entry in device_obj.logs {
        println!("{:}", format_log(&log_entry));
    }
}

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
    // test_address_functions();
    
    // test_herc_scheduler();

    // test_fighter_scheduler();
    // unsafe{nice(-20)};
    // #[allow(unused)]
    // let system = eval_sys(0, 4, Proto::RT2RT, true);

    let word_delay = 20_000; // nanoseconds to transmit a word.
    fighter_simulation(word_delay);


}
