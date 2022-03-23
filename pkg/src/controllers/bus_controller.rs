#[allow(unused_imports)]
use crate::sys::{
    DefaultScheduler, Router, System, WRD_EMPTY, Scheduler
};
use crate::devices::Device;
use crate::event_handlers::{DefaultEventHandler, EventHandler};
use crate::primitive_types::{ErrMsg, Mode, Word};
#[allow(unused_imports)]
use std::time::{Instant, Duration};
use priority_queue::DoublePriorityQueue;
use crate::primitive_types::{Address, MsgPri};

#[allow(unused)]
pub const ALERT_ON_COMPETING_CONTROLLER: bool = false; //if there is a competing bus controller, raise an alert;
#[allow(unused)]
pub const ALERT_ON_UNPROMPTED_DATA: bool = false; // Alert if we see a data word that we didn't solicit
#[allow(unused)]
pub const ALERT_ON_UNPROMPTED_STATUS: bool = false; // Alert if we see a status word that we didn't solicit
#[allow(unused)]
pub const ALERT_INTERUPTED_BEFORE_TRANSMIT: bool = false; // Alert if another RT sent data on the bus when we were asked to


#[derive(Hash, PartialEq, Eq, Copy, Clone)]
pub struct Event {
    source: Address,
    // source sub address?
    destination: Address,
    // destination sub address?
    priority: MsgPri,
    repeating: bool,
    word_count: u8,
}

#[allow(unused)]
pub struct HercScheduler { // Herc for Hercules which is a CC-130
    pub priority_list: DoublePriorityQueue<(Address, Address), u128>, // instantiate with <(Address, Address)>
    #[allow(unused)]
    timeout: u128,
}

#[allow(unused)]
impl HercScheduler {
    pub fn new() -> Self {
        let mut scheduler = HercScheduler{
            priority_list: DoublePriorityQueue::new(),
            timeout: 0,
        };
        use Address::*;
        scheduler.priority_list.push((FlightControls, Trim), 0);
        scheduler.priority_list.push((FlightControls, Ailerons), 0);
        scheduler.priority_list.push((FlightControls, Elevators), 0);
        scheduler.priority_list.push((FlightControls, Engine), 0);
        scheduler.priority_list.push((Engine, FlightControls), 0);
        scheduler.priority_list.push((LandingGear, FlightControls), 0);
        return scheduler;
    }

    pub fn on_bc_ready(&mut self, d: &mut Device) -> Option<String> {
        // We pop the next message and wait until we should send it. This cannot be preempted, but that shouldn't be a problem.  
        // SR bits should only come during a message requested by the bus controller.
        let spin_sleeper = spin_sleep::SpinSleeper::new(1_000_000);
        let message = self.priority_list.pop_min();
        match message {
            Some(((src, dst), mut time)) => { 
                let wc = src.word_count(&dst);
                time = if time > self.timeout {time} else {self.timeout};
                let mut current = d.clock.elapsed().as_nanos();
                if time > current {
                    //TODO account for possible overflow if we're very close to execution time.
                    let wait = time - current;
                    spin_sleeper.sleep_ns(wait.try_into().unwrap());
                    current = d.clock.elapsed().as_nanos();
                }
                match (src, dst) {
                    (source, _) if source as u8 == d.address => {// BC to RT
                        if src.repeat_function(&dst) {
                            self.update_priority((src, dst), time);
                        }
                        // d.act_bc2rt(dst as u8, wc); // Can't be wordcount, must be data.  We don't know what data we want to send, that's the Device itself.
                        let bus_available = current + (2+wc as u128) * 20_000;
                        self.timeout = bus_available;
                        Some(format!("[{:0>6?}] from {src:?} to {dst:?} with {src:?} as BC\n[{:0>6?}] - message finished\n", current/1000, bus_available/1000))
                    }
                    (_, destination) if destination as u8 == d.address => {// RT to BC
                        if src.repeat_function(&dst) {
                            self.update_priority((src, dst), time);
                        }
                        d.act_rt2bc(src as u8, wc);
                        let bus_available = current + (2+wc as u128) * 20_000;
                        self.timeout = bus_available;
                        Some(format!("[{:0>6?}] from {src:?} to {dst:?} with {dst:?} as BC\n[{:0>6?}] - message finished\n", current/1000, bus_available/1000))
                    }
                    _ => {// RT to RT
                        if src.repeat_function(&dst) {
                            self.update_priority((src, dst), time);
                        }
                        d.act_rt2rt(src as u8, dst as u8, wc);
                        let bus_available = current + (4+wc as u128) * 20_000;
                        self.timeout = bus_available;
                        Some(format!("[{:0>6?}] from {src:?} to {dst:?}\n[{:0>6?}] - message finished\n", current/1000, bus_available/1000))
                    }
                }
            }
            None => {None}
        }
        // self.timeout = bus_available; // d.clock.elapsed().as_nanos() + 20_000 + 16_000; // 20us for word transmission and 16us for timeout between messages
    }

    pub fn update_priority(&mut self, (src, dst): (Address, Address), time: u128) {
        let delay = src.priority(&dst).delay() as u128;
        let next_time = time + delay;
        self.priority_list.push((src, dst), next_time);
    }
}

#[derive(Clone)]
pub enum SystemState {
    Inactive,
    Active,
}

#[derive(Clone)]
pub struct FighterScheduler {
    pub priority_list: DoublePriorityQueue<Event, u128>,
    timeout: u128,
    landing_gear_state: SystemState,
    landing_gear_events: Vec<Event>,
    radar_state: SystemState,
    radar_events: Vec<Event>,
    rover_state: SystemState,
    rover_events: Vec<Event>,
    current_event: Option<Event>,
}

impl FighterScheduler {
    pub fn new() -> Self {
        use Address::*;
        use MsgPri::*;
        let fighter_schedule: Vec<Event> = vec![ //F-18 schedule
            // With feedback
            // Event {source: FlightControls, destination: Trim,  priority: Low,  repeating: true,
            //     word_count: 2, //one float32 should carry sufficient data
            // },
            // Event {source: Trim,   destination: FlightControls,    priority: Low,  repeating: true,
            //     word_count: 2,//one float32 should carry sufficient data
            // },
            Event {source: FlightControls, destination: Flaps, priority: Low,  repeating: true,
                word_count: 1,
            },
            // Event {source: Flaps,  destination: FlightControls,    priority: Low,  repeating: true,
            //     word_count: 1,
            // },
            Event {source: FlightControls, destination: Engine,    priority: VeryHigh, repeating: true,
                word_count: 16, //We'll estimate a float32 for each of the engines (up to four engines) and 2 words per float32
            },
            Event {source: Engine, destination: FlightControls,    priority: High, repeating: true,
                word_count: 16, //Temperature, speed, 
            },
            // Event {source: FlightControls, destination: LandingGear,   priority: Low,  repeating: true,
            //     word_count: 1, //Binary message, but we'll send a whole word
            // },
            // Event {source: LandingGear,    destination: FlightControls,    priority: Lowest,   repeating: true,
            //     word_count: 1, //Binary message, but we'll send a whole word
            // },
            // Event {source: FlightControls, destination: Weapons,   priority: VeryHigh, repeating: true,
            //     word_count: 4, //Targeting information along with the weapon selected and whether or not to open the compartment
            // },
            // Event {source: Weapons,    destination: BusControl,    priority: Medium,   repeating: true,
            //     word_count: 0, //Check for an SR and then activate the "service request" message.
            // },
            // Without feedback
            Event {source: FlightControls, destination: Rudder,    priority: VeryHigh, repeating: true,
                word_count: 2,//float32 for degree
            },
            Event {source: FlightControls, destination: Ailerons,  priority: VeryHigh, repeating: true,
                word_count: 4,//float32 for degree on each wing
            },
            Event {source: FlightControls, destination: Elevators, priority: VeryHigh, repeating: true,
                word_count: 4,//float32 for degree on each wing
            },
            // Event {source: FlightControls, destination: Slats, priority: VeryHigh, repeating: true,
            //     word_count: 4,//float32 for degree on each wing
            // },
            Event {source: FlightControls, destination: Spoilers,  priority: VeryHigh, repeating: true,
                word_count: 4,//float32 for degree on each wing
            },
            // Sensors
            Event {source: Fuel,   destination: FlightControls,    priority: Lowest,   repeating: true,    
                word_count: 4, //one float32 for quantity, one float32 for flow
            },
            Event {source: Gyro, destination: FlightControls,    priority: Medium,   repeating: true,    
                word_count: 6, //one float32 for heading
            },
            // Event {source: Altimeter,  destination: FlightControls,    priority: Medium,   repeating: true,    
            //     word_count: 1,
            // },
            Event {source: Positioning,    destination: FlightControls,    priority: Lowest,   repeating: true,    
                word_count: 3, // Lat, Long, Alt
            },
            // Event {source: Pitch,   destination: FlightControls,    priority: Medium,   repeating: true,    
            //     word_count: 6, //float32 for pitch, bank, and roll
            // },
            // Event {source: Rover, destination: BusControl, priority: Medium, repeating: true,
            //     word_count: 1, // int of the number of words to send
            // },
            // Event {source: Radar, destination: BusControl, priority: Medium, repeating: true,
            //     word_count: 1, // int of the number of words to send
            // },
        ];
        let mut scheduler = FighterScheduler{
            priority_list: DoublePriorityQueue::new(),
            timeout: 0,
            landing_gear_state: SystemState::Active, // Enable or disable landing_gear_events based on this value
            landing_gear_events: Vec::new(),
            radar_state: SystemState::Inactive, // Enable or disable radar updates based on this value.  This will save updates when there is no data to be sent.
            radar_events: Vec::new(),
            rover_state: SystemState::Inactive, // Enable or disable rover updates based on this value.  This will save updates when there is no data to be sent.
            rover_events: Vec::new(),
            current_event: None,
        };
        for event in fighter_schedule { // should we randomize the events?  This would make the time series analysis a little different
            scheduler.priority_list.push(event, 0);
        }
        scheduler.landing_gear_events.push(
            Event {source: FlightControls, destination: Brakes,    priority: High, repeating: true,
                word_count: 4,  //float32 for degree on each side
            }
        );
        // scheduler.landing_gear_events.push(
        //     Event {source: Brakes, destination: FlightControls, priority: Medium, repeating: true,
        //         word_count: 12,  // float32 for torque (all three points of contact)
        //                         // float32 for wheel load (all three points of contact)
        //     }
        // );
        // scheduler.landing_gear_events.push( // We may want this to work, even with the landing gear up.  Maybe we slide the plane on its belly.
        //     Event {source: FlightControls, destination: Tailhook, priority: Medium, repeating: true,
        //     word_count: 1,}
        // );
        // scheduler.landing_gear_events.push(
        //     Event {source: Tailhook, destination: FlightControls, priority: Medium, repeating: true,
        //     word_count: 1,}
        // );
        // scheduler.radar_events.push(
        //     Event {source: Radar, destination: FlightControls, priority: High, repeating: true, word_count: 0}
        // );
        // scheduler.rover_events.push(
        //     Event {source: Rover, destination: FlightControls, priority: High, repeating: true, word_count: 0}
        // );
        for event in &scheduler.landing_gear_events {
            scheduler.priority_list.push(*event, 0);
        }
        return scheduler;
    }

    pub fn on_bc_ready(&mut self, d: &mut Device) {}

    fn update_priority(&mut self, event: Event, time: u128) {
        let delay = event.priority.delay() as u128;
        let next_time = time + delay;
        self.priority_list.push(event, next_time);
    }
}

impl Scheduler for FighterScheduler {
    fn on_bc_ready(&mut self, d: &mut Device) /*-> Option<String>*/ {
        // We pop the next message and wait until we should send it. This cannot be preempted, but that shouldn't be a problem.  
        // SR bits should only come during a message requested by the bus controller.
        let spin_sleeper = spin_sleep::SpinSleeper::new(1_000_000);
        let message = self.priority_list.pop_min();
        match message {
            Some((Event{source: src, destination: dst, priority: pri, repeating: repeat, word_count: wc}, mut time)) => {
                self.current_event = Some(Event{source: src, destination: dst, priority: MsgPri::Immediate, repeating: false, word_count: wc});
                time = if time > self.timeout {time} else {self.timeout};
                let mut current = d.clock.elapsed().as_nanos();
                if time >= current {
                    let wait = time - current;
                    spin_sleeper.sleep_ns(wait.try_into().unwrap());
                    current = d.clock.elapsed().as_nanos();
                }
                match (src, dst) {
                    (source, _) if source as u8 == d.address => {// BC to RT
                        if repeat {
                            self.update_priority(Event{source:src, destination: dst, priority: pri, repeating: repeat, word_count: wc}, time);
                        }
                        // d.act_bc2rt(dst as u8, wc); // Can't be wordcount, must be data.  We don't know what data we want to send, that's the Device itself.
                        let bus_available = current + (2+wc as u128) * 20_000;
                        self.timeout = bus_available;
                        //Some(format!("[{:0>6?}] from {src:?} to {dst:?} with {src:?} as BC\n[{:0>6?}] - message finished\n", current/1000, bus_available/1000))
                    }
                    (_, destination) if destination as u8 == d.address => {// RT to BC
                        if repeat {
                            self.update_priority(Event{source:src, destination: dst, priority: pri, repeating: repeat, word_count: wc}, time);
                        }
                        d.act_rt2bc(src as u8, wc);
                        let bus_available = current + (2+wc as u128) * 20_000;
                        self.timeout = bus_available;
                        //Some(format!("[{:0>6?}] from {src:?} to {dst:?} with {dst:?} as BC\n[{:0>6?}] - message finished\n", current/1000, bus_available/1000))
                    }
                    _ => {// RT to RT
                        if repeat {
                            self.update_priority(Event{source:src, destination: dst, priority: pri, repeating: repeat, word_count: wc}, time);
                        }
                        d.act_rt2rt(src as u8, dst as u8, wc);
                        let bus_available = current + (4+wc as u128) * 20_000;
                        self.timeout = bus_available;
                        //Some(format!("[{:0>6?}] from {src:?} to {dst:?}\n[{:0>6?}] - message finished\n", current/1000, bus_available/1000))
                    }
                }
            }
            None => {
                // None
            }
        }
        // self.timeout = bus_available; // d.clock.elapsed().as_nanos() + 20_000 + 16_000; // 20us for word transmission and 16us for timeout between messages
    }

    fn request_sr(&mut self, rt: u8) {
        let (dest, wc) = Address::from(rt).on_sr();
        let item = Event {
            source: Address::from(rt),
            destination: dest,
            priority: MsgPri::Immediate,
            repeating: false,
            word_count: wc,
        };
        self.priority_list.push(item, 0);
    }

    fn error_bit(&mut self) {
        match self.current_event {
            Some(event) => {
                self.priority_list.push(event, 0);
            }
            _ => {}
        }
    }
}

#[cfg(tests)]
mod tests {
    use super::*;

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
}
