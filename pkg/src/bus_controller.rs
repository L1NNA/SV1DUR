#[allow(unused_imports)]
use crate::sys::{
    DefaultEventHandler, DefaultScheduler, Device, ErrMsg, EventHandler, Mode, Router, System,
    Word, WRD_EMPTY
};
#[allow(unused_imports)]
use std::time::{Instant, Duration};
use priority_queue::DoublePriorityQueue;

#[allow(unused)]
pub const ALERT_ON_COMPETING_CONTROLLER: bool = false; //if there is a competing bus controller, raise an alert;
#[allow(unused)]
pub const ALERT_ON_UNPROMPTED_DATA: bool = false; // Alert if we see a data word that we didn't solicit
#[allow(unused)]
pub const ALERT_ON_UNPROMPTED_STATUS: bool = false; // Alert if we see a status word that we didn't solicit
#[allow(unused)]
pub const ALERT_INTERUPTED_BEFORE_TRANSMIT: bool = false; // Alert if another RT sent data on the bus when we were asked to

#[allow(unused)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Address {
    BusControl,
    FlightControls,
    Trim,
    Engine,
    Flaps,
    LandingGear,
    Weapons,
    Rudder,
    Ailerons,
    Elevators,
    Slats,
    Spoilers,
    Brakes,
    Fuel,
    Heading,
    Altimeter,
    Positioning, //GPS
    Pitch,
}

#[allow(unused)]
impl Address {
    pub fn priority(&self, destination: &Address) -> MsgPri {
        match (self, destination) {
            // With Feedback
            (Address::FlightControls,   Address::Trim)              => MsgPri::Low,
            (Address::Trim,             Address::FlightControls)    => MsgPri::Lowest,
            (Address::FlightControls,   Address::Flaps)             => MsgPri::Low,
            (Address::Flaps,            Address::FlightControls)    => MsgPri::Lowest,
            (Address::FlightControls,   Address::Engine)            => MsgPri::VeryHigh,
            (Address::Engine,           Address::FlightControls)    => MsgPri::High,
            (Address::FlightControls,   Address::LandingGear)       => MsgPri::Low,
            (Address::LandingGear,      Address::FlightControls)    => MsgPri::Lowest,
            (Address::FlightControls,   Address::Weapons)           => MsgPri::VeryHigh,
            (Address::Weapons,          Address::FlightControls)    => MsgPri::Medium,
            // Without Feedback
            (Address::FlightControls, Address::Rudder)      => MsgPri::VeryHigh,
            (Address::FlightControls, Address::Ailerons)    => MsgPri::VeryHigh,
            (Address::FlightControls, Address::Elevators)   => MsgPri::VeryHigh,
            (Address::FlightControls, Address::Slats)       => MsgPri::VeryHigh,
            (Address::FlightControls, Address::Spoilers)    => MsgPri::VeryHigh,
            (Address::FlightControls, Address::Brakes)      => MsgPri::High,
            //Sensors
            (Address::Fuel,         Address::FlightControls) => MsgPri::Lowest,
            (Address::Heading,      Address::FlightControls) => MsgPri::Medium,
            (Address::Altimeter,    Address::FlightControls) => MsgPri::Medium,
            (Address::Positioning,  Address::FlightControls) => MsgPri::Lowest,
            (Address::Pitch,        Address::FlightControls) => MsgPri::Medium,
            _ => MsgPri::VeryHigh,
        }
    }

    pub fn repeat_function(&self, destination: &Address) -> bool {
        match (self, destination) {
            // With Feedback
            (Address::FlightControls,   Address::Trim)              => true,
            (Address::Trim,             Address::FlightControls)    => true,
            (Address::FlightControls,   Address::Flaps)             => true,
            (Address::Flaps,            Address::FlightControls)    => true,
            (Address::FlightControls,   Address::Engine)            => true,
            (Address::Engine,           Address::FlightControls)    => true,
            (Address::FlightControls,   Address::LandingGear)       => true,
            (Address::LandingGear,      Address::FlightControls)    => true,
            (Address::FlightControls,   Address::Weapons)           => true,
            (Address::Weapons,          Address::FlightControls)    => true,
            // Without Feedback
            (Address::FlightControls, Address::Rudder)      => true,
            (Address::FlightControls, Address::Ailerons)    => true,
            (Address::FlightControls, Address::Elevators)   => true,
            (Address::FlightControls, Address::Slats)       => true,
            (Address::FlightControls, Address::Spoilers)    => true,
            (Address::FlightControls, Address::Brakes)      => true,
            //Sensors
            (Address::Fuel,         Address::FlightControls) => true,
            (Address::Heading,      Address::FlightControls) => true,
            (Address::Altimeter,    Address::FlightControls) => true,
            (Address::Positioning,  Address::FlightControls) => true,
            (Address::Pitch,        Address::FlightControls) => true,
            _ => false,
        }
    }

    pub fn word_count(&self, destination: &Address) -> u8 {
        match (self, destination) {
            // With Feedback
            (Address::FlightControls,   Address::Trim)              => 2, //one float32 should carry sufficient data
            (Address::Trim,             Address::FlightControls)    => 2,
            (Address::FlightControls,   Address::Flaps)             => 1, //A single u4 could do it, but we're going to send a whole word
            (Address::Flaps,            Address::FlightControls)    => 1,
            (Address::FlightControls,   Address::Engine)            => 8, //We'll estimate a float32 for each of the engines (up to four engines) and 2 words per float32
            (Address::Engine,           Address::FlightControls)    => 8,
            (Address::FlightControls,   Address::LandingGear)       => 1, //Binary message, but we'll send a whole word
            (Address::LandingGear,      Address::FlightControls)    => 1,
            (Address::FlightControls,   Address::Weapons)           => 4, //Targeting information along with the weapon selected and whether or not to open the compartment
            (Address::Weapons,          Address::FlightControls)    => 20, //confirmation data of currently configured system
            // Without Feedback
            (Address::FlightControls, Address::Rudder)      => 2, //float32 for degree
            (Address::FlightControls, Address::Ailerons)    => 4, //float32 for degree on each wing
            (Address::FlightControls, Address::Elevators)   => 4, //float32 for degree on each wing
            (Address::FlightControls, Address::Slats)       => 4, //float32 for degree on each wing
            (Address::FlightControls, Address::Spoilers)    => 4, //float32 for degree on each wing
            (Address::FlightControls, Address::Brakes)      => 4, //float32 for degree on each side
            //Sensors
            (Address::Fuel,         Address::FlightControls) => 4, //one float32 for quantity, one float32 for flow
            (Address::Heading,      Address::FlightControls) => 2, //one float32 for heading
            (Address::Altimeter,    Address::FlightControls) => 1,
            (Address::Positioning,  Address::FlightControls) => 3, // Lat, Long, Alt
            (Address::Pitch,        Address::FlightControls) => 6, //float32 for pitch, bank, and roll
            _ => 2, //2 words for anything unlisted
        }
    }
}

#[allow(unused)]
#[derive(Debug)]
pub enum MsgPri {
    Immediate,
    VeryHigh,
    High,
    Medium,
    Low,
    VeryLow,
    Lowest,
}

#[allow(unused)]
impl MsgPri {
    pub fn delay(&self) -> u32 {
        // delays will be harmonic frequencies that double at each drop in priority
        // 50Hz -- 1/50 = 0.02s -- 0.02 * 1000 * 1000 * 1000 = 20_000_000ns
        match self {
            MsgPri::Immediate   =>           0, // send this immediately
            MsgPri::VeryHigh    =>  20_000_000, // 50Hz
            MsgPri::High        =>  40_000_000, // 25Hz
            MsgPri::Medium      =>  80_000_000, // 12.5Hz
            MsgPri::Low         => 160_000_000, // 6.25Hz
            MsgPri::VeryLow     => 320_000_000, // 3.125Hz
            MsgPri::Lowest      => 640_000_000, // 1.5625Hz
            _ => 0, // /infty Hz
        }
    }
}

pub struct HercScheduler { // Herc for Hercules which is a CC-130
    pub priority_list: DoublePriorityQueue<(Address, Address), u128>, // instantiate with <(Address, Address)>
    #[allow(unused)]
    timeout: u128,
}

impl HercScheduler {
    pub fn new() -> Self {
        let mut scheduler = HercScheduler{
            priority_list: DoublePriorityQueue::new(),
            timeout: 0,
        };
        scheduler.priority_list.push((Address::FlightControls, Address::Trim), 0);
        scheduler.priority_list.push((Address::FlightControls, Address::Ailerons), 0);
        scheduler.priority_list.push((Address::FlightControls, Address::Elevators), 0);
        scheduler.priority_list.push((Address::FlightControls, Address::Engine), 0);
        scheduler.priority_list.push((Address::Engine, Address::FlightControls), 0);
        scheduler.priority_list.push((Address::LandingGear, Address::FlightControls), 0);
        return scheduler;
    }

    pub fn on_bc_ready(&mut self, d: &mut Device) -> Option<String> {
        // The current setup does not allow for a message to be prepended to the list.
        // We pop the next message and wait until we should send it.  
        // If a message is prepended it will be sent immediately after the first one is delivered.
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

// pub struct BCHandler {
//     address: Address,
//     priority_list: PriorityQueue, //A container to select the next device to communicate, and when that device should send its data
//     timeout: u128,
// }

// impl EventHandler for BCHandler {
//     fn on_cmd(&mut self, d: &mut Device, w: &mut Word) {
//         if ALERT_ON_COMPETING_CONTROLLER {
//             //Another terminal is trying to act as the bus controller.
//         }
//         self.default_on_cmd(d, w)
//     }

//     fn on_dat(&mut self, d: &mut Device, w: &mut Word) {
//         // count the number of data words so we know when the current message is finished.
//         if d.state == State::AwtData {
//             // perform any processing on this data word
//         } else if ALERT_ON_UNPROMPTED_DATA {
//             // We didn't ask for this many data words
//         } else {
//             // ignore the data word
//         }
//         self.default_on_dat(d, w)
//     }

//     fn on_sts(&mut self, d: &mut Device, w: &mut Word) {
//         // check for SR bit and include in the priority list.
//         if State::AwtStsRcvB2R <= d.state <= State::AwtStsTrxR2R {
//             if w.service_request_bit() {
//                 // Add requested item to the queue
//             }
//         } else if ALERT_ON_UNPROMPTED_STATUS {
//             // We didn't ask for this status word.
//         } else {

//         }
//         self.default_on_sts(d, w)
//     }

//     fn on_err_parity(&mut self, d: &mut Device, w: &mut Word) {
//         // We should implement an error correction mechanism here.
//         self.default_on_err_parity(d, w)
//     }
// }