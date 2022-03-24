use crate::sys::{System, Router};
use crate::primitive_types::{Mode, AttackType, Address};
use crate::event_handlers::DefaultEventHandler;
use crate::schedulers::{FighterScheduler, Proto};
use crate::devices::Device;
use std::sync::{Arc, Mutex};


pub fn fighter_simulation(w_delays: u128, n_devices: u8) -> System {
    // let n_devices = 3;
    // let w_delays = w_delays;
    let devices = vec![Address::BusControl, Address::FlightControls, Address::Flaps, Address::Engine, Address::Rudder, Address::Ailerons, 
                        Address::Elevators, Address::Spoilers, Address::Fuel, Address::Positioning, Address::Gyro, Address::Brakes];
    let mut sys = System::new(devices.len() as u32, w_delays);
    for m in devices {
        // let (s1, r1) = bounded(64);
        // s_vec.lock().unwrap().push(s1);
        let router = Router {
            // control all communications
            scheduler: FighterScheduler::new(),
            // control device-level response
            handler: DefaultEventHandler {},
        };
        if m as i8 == 0 {
            sys.run_d(
                m as u8,
                Mode::BC,
                Arc::new(Mutex::new(router)),
                AttackType::Benign,
            );
        } else {
            sys.run_d(
                m as u8,
                Mode::RT,
                Arc::new(Mutex::new(router)),
                AttackType::Benign,
            );
        }
    }
    sys.go();
    sys.sleep_ms(1000);
    sys.stop();
    sys.join();
    return sys;
}