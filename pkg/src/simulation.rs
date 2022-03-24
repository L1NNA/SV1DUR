use rusqlite::{Connection, Result};
use crate::sys::{System, Router};
use crate::primitive_types::{Mode, AttackType, Address};
use crate::event_handlers::{EventHandler, DefaultEventHandler, OfflineHandler, OfflineFlightControlsHandler};
use crate::schedulers::{FighterScheduler, Proto, Scheduler};
use crate::devices::Device;
use crate::terminals::SplitInt;
use std::sync::{Arc, Mutex};
use std::collections::LinkedList;


pub fn extract_contents(database: &str, component: Address) -> Option<LinkedList<(u32, Vec<u16>)>> {
    use Address::*;
    let fields = match component {
        BusControl => "",
        FlightControls => ", yoke_x_position, yoke_y_position, yoke_x_indicator, yoke_y_indicator, rudder_pedal_position, rudder_pedal_indicator, brakes_right_position, brakes_left_position, throttle_level_position1, spoiler_handle_position, flaps_handle_percent",
        Flaps => "",
        Engine => "",
        Rudder => ", rudder_position",
        Ailerons => "",
        Elevators => "",
        Spoilers => "",
        Fuel => ", fuel_total_quantity, estimated_fuel_flow",
        Positioning => ", gps_latitude, gps_longitude, gps_altitude",
        Gyro => ", plane_pitch, plane_bank, incidence_alpha, incidence_beta, plane_heading_gyro",
        Brakes => "",
        _ => "",
    };
    let conn = Connection::open(database).unwrap();
    let num_fields = fields.matches(",").count();
    let command = format!("SELECT elapsed_ms, delta_ms, absolute_time{:} FROM sensor_data", fields);
    // println!("Executing command: {:?}", command);
    let mut results = conn.prepare(&command[..]).unwrap();
    let mut field_content: f32;
    let data_iter = results.query_map([], |row| {
        let time: u32 = row.get(0)?;
        let mut data: Vec<u16> = Vec::new();
        for i in 0..num_fields {
        let field_content: f32 = row.get(3+i)?;
            for slice in SplitInt::new(field_content.to_bits()).extract() { // extract 16bits ints from 32bit floats
                data.push(slice);
            }
        }
        Ok((time, data))
    });
    let mut data_vec: LinkedList<(u32, Vec<u16>)> = LinkedList::new();
    let data_iter_unwrap = data_iter.unwrap();
    for entry in data_iter_unwrap {
        match entry {
            Ok(content) => data_vec.push_back(content),
            _ => {},
        }
    }
    Some(data_vec)
}


pub fn fighter_simulation(w_delays: u128) -> Result<()> {
    // let n_devices = 3;
    // let w_delays = w_delays;
    let database = "flight_data_2022-03-11-22-39-41.sqlite";
    let devices = vec![Address::BusControl, Address::FlightControls, Address::Flaps, Address::Engine, Address::Rudder, Address::Ailerons, 
                        Address::Elevators, Address::Spoilers, Address::Fuel, Address::Positioning, Address::Gyro, Address::Brakes];
    let mut sys = System::new(devices.len() as u32, w_delays);
    for m in devices {
        // let (s1, r1) = bounded(64);
        // s_vec.lock().unwrap().push(s1);
        if m != Address::FlightControls {
            let router = Router {
                // control all communications
                scheduler: FighterScheduler::new(),
                // control device-level response
                handler: OfflineHandler::new(extract_contents(database, m).unwrap()),
            };
            if m == Address::BusControl {
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
        } else {
            let router = Router {
                // control all communications
                scheduler: FighterScheduler::new(),
                // control device-level response
                handler: OfflineFlightControlsHandler::new(extract_contents(database, m).unwrap()),
            };
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
    Ok(())
}