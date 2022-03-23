use crate::sys::{System, Proto, Router};
use crate::primitive_types::{Mode, AttackType};
use crate::event_handlers::DefaultEventHandler;
use crate::controllers::bus_controller::FighterScheduler;
use crate::devices::Device;
use std::sync::{Arc, Mutex};


pub fn fighter_simulation(w_delays: u128, n_devices: u8) -> System {
    // let n_devices = 3;
    // let w_delays = w_delays;
    let mut sys = System::new(n_devices as u32, w_delays);
    for m in 0..n_devices {
        // let (s1, r1) = bounded(64);
        // s_vec.lock().unwrap().push(s1);
        let router = Router {
            // control all communications
            scheduler: FighterScheduler::new() 
            // {
            //     total_device: n_devices,
            //     target: 0,
            //     data: vec![1, 2, 3],
            //     proto: proto,
            //     proto_rotate: proto_rotate,
            // }
            ,
            // control device-level response
            handler: DefaultEventHandler {},
        };
        if m == 0 {
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