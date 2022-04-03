use crate::attacks::AttackController;
use crate::sys::{
    AttackType, DefaultEventHandler, Device, ErrMsg, EventHandler, EventHandlerEmitter, Mode,
    System, Word, TR, WRD_EMPTY,
};
use bitfield::bitfield;
use priority_queue::DoublePriorityQueue;
use rusqlite::{Connection, Result};
use std::collections::LinkedList;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

bitfield! {
    pub struct SplitInt(u32);
    impl Debug;
    u16;
    pub data1, set_data1: 31, 0;
    pub word1, _: 15, 0;
    pub word2, _: 31, 16;
}

impl SplitInt {
    pub fn new(var: u32) -> SplitInt {
        let int = SplitInt { 0: var };
        int
    }

    pub fn extract(&mut self) -> Vec<u16> {
        vec![self.word1(), self.word2()]
    }
}

#[allow(unused)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Address {
    BusControl,
    FlightControls,
    Trim,
    Engine,
    Flaps,
    LandingGear,
    Weapons,
    Radar,
    Rover,
    Radio,
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
    ClimateControl,
    Tailhook,
    Gyro,
    Climate,
    BusMonitor,
    AttackController,
    Broadcast = 31,
}

#[allow(unused)]
impl Address {
    pub fn priority(&self, destination: &Address) -> MsgPri {
        // Defines the "priority" for each pairing of devices.
        // This priority is used to determine how quickly the next message should be sent.
        use Address::*;
        use MsgPri::*;
        match (self, destination) {
            // With Feedback
            (FlightControls, Trim) => Low,
            (Trim, FlightControls) => Lowest,
            (FlightControls, Flaps) => Low,
            (Flaps, FlightControls) => Lowest,
            (FlightControls, Engine) => VeryHigh,
            (Engine, FlightControls) => High,
            (FlightControls, LandingGear) => Low,
            (LandingGear, FlightControls) => Lowest,
            (FlightControls, Weapons) => VeryHigh,
            (Weapons, FlightControls) => Medium,
            // Without Feedback
            (FlightControls, Rudder) => VeryHigh,
            (FlightControls, Ailerons) => VeryHigh,
            (FlightControls, Elevators) => VeryHigh,
            (FlightControls, Slats) => VeryHigh,
            (FlightControls, Spoilers) => VeryHigh,
            (FlightControls, Brakes) => High,
            //Sensors
            (Fuel, FlightControls) => Lowest,
            (Heading, FlightControls) => Medium,
            (Altimeter, FlightControls) => Medium,
            (Positioning, FlightControls) => Lowest,
            (Pitch, FlightControls) => Medium,
            /*
            Add in steering for the front wheel?
            Climate control?
            Radar
            ROVER -
            Tailhook
            */
            _ => VeryHigh,
        }
    }

    pub fn repeat_function(&self, destination: &Address) -> bool {
        // This dictates whether or not a message will be repeated on a regular frequency.
        use Address::*;
        use MsgPri::*;
        match (self, destination) {
            // With Feedback
            (FlightControls, Trim) => true,
            (Trim, FlightControls) => true,
            (FlightControls, Flaps) => true,
            (Flaps, FlightControls) => true,
            (FlightControls, Engine) => true,
            (Engine, FlightControls) => true,
            (FlightControls, LandingGear) => true,
            (LandingGear, FlightControls) => true,
            (FlightControls, Weapons) => true,
            (Weapons, FlightControls) => true,
            // Without Feedback
            (FlightControls, Rudder) => true,
            (FlightControls, Ailerons) => true,
            (FlightControls, Elevators) => true,
            (FlightControls, Slats) => true,
            (FlightControls, Spoilers) => true,
            (FlightControls, Brakes) => true,
            //Sensors
            (Fuel, FlightControls) => true,
            (Heading, FlightControls) => true,
            (Altimeter, FlightControls) => true,
            (Positioning, FlightControls) => true,
            (Pitch, FlightControls) => true,
            _ => false,
        }
    }

    pub fn word_count(&self, destination: &Address) -> u8 {
        // This dictates the number of words that need to be passed between the devices to transfer all of the data.
        use Address::*;
        use MsgPri::*;
        match (self, destination) {
            // With Feedback
            (FlightControls, Trim) => 2, //one float32 should carry sufficient data
            (Trim, FlightControls) => 2,
            (FlightControls, Flaps) => 1, //A single u4 could do it, but we're going to send a whole word
            (Flaps, FlightControls) => 1, // Planes can have leading and trailing edge flaps.  I don't know if they are controlled separately
            (FlightControls, Engine) => 8, //We'll estimate a float32 for each of the engines (up to four engines) and 2 words per float32
            (Engine, FlightControls) => 8, //Temperature, speed,
            (FlightControls, LandingGear) => 1, //Binary message, but we'll send a whole word
            (LandingGear, FlightControls) => 1,
            (FlightControls, Weapons) => 4, //Targeting information along with the weapon selected and whether or not to open the compartment
            (Weapons, FlightControls) => 20, //confirmation data of currently configured system
            // 578 rounds of M61A1 Vulcan
            // 9 rockets
            // Bomb

            // Without Feedback
            (FlightControls, Rudder) => 2,    //float32 for degree
            (FlightControls, Ailerons) => 4,  //float32 for degree on each wing
            (FlightControls, Elevators) => 4, //float32 for degree on each wing
            (FlightControls, Slats) => 4,     //float32 for degree on each wing
            (FlightControls, Spoilers) => 4,  //float32 for degree on each wing
            (FlightControls, Brakes) => 4,    //float32 for degree on each side
            //Brakes should have torque sensor
            //Load on wheel sensor
            //Sensors
            (Fuel, FlightControls) => 4,
            (Heading, FlightControls) => 2,
            (Altimeter, FlightControls) => 1,
            (Positioning, FlightControls) => 3,
            (Pitch, FlightControls) => 6,
            _ => 2, //2 words for anything unlisted
        }
    }

    pub fn on_sr(&self) -> (Address, u8) {
        use Address::*;
        match self {
            Weapons => (FlightControls, 20),
            _ => (FlightControls, 2),
            // I also need to know how many words to send.
            // An i16 let's me use -1 as a sentinel value to indicate that the device will specify.  We could also just use any value.
        }
    }
}

impl From<u8> for Address {
    fn from(value: u8) -> Self {
        use Address::*;
        match value {
            value if value == BusControl as u8 => BusControl,
            value if value == FlightControls as u8 => FlightControls,
            value if value == Trim as u8 => Trim,
            value if value == Engine as u8 => Engine,
            value if value == Flaps as u8 => Flaps,
            value if value == LandingGear as u8 => LandingGear,
            value if value == Weapons as u8 => Weapons,
            value if value == Radar as u8 => Radar,
            value if value == Rover as u8 => Rover,
            value if value == Radio as u8 => Radio,
            value if value == Rudder as u8 => Rudder,
            value if value == Ailerons as u8 => Ailerons,
            value if value == Elevators as u8 => Elevators,
            value if value == Slats as u8 => Slats,
            value if value == Spoilers as u8 => Spoilers,
            value if value == Brakes as u8 => Brakes,
            value if value == Fuel as u8 => Fuel,
            value if value == Heading as u8 => Heading,
            value if value == Altimeter as u8 => Altimeter,
            value if value == Positioning as u8 => Positioning, //GPS
            value if value == Pitch as u8 => Pitch,
            value if value == ClimateControl as u8 => ClimateControl,
            value if value == Tailhook as u8 => Tailhook,
            value if value == Gyro as u8 => Gyro,
            value if value == Climate as u8 => Climate,
            _ => Broadcast,
        }
    }
}

#[allow(unused)]
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
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
        // The amount of delay to reach a desired message frequency.
        // delays will be harmonic frequencies that double at each drop in priority
        // 50Hz -- 1/50 = 0.02s -- 0.02 * 1000 * 1000 * 1000 = 20_000_000ns
        use MsgPri::*;
        match self {
            Immediate => 0,         // send this immediately
            VeryHigh => 20_000_000, // 50Hz
            High => 40_000_000,     // 25Hz
            Medium => 80_000_000,   // 12.5Hz
            Low => 160_000_000,     // 6.25Hz
            VeryLow => 320_000_000, // 3.125Hz
            Lowest => 640_000_000,  // 1.5625Hz
            _ => 0,                 // /infty Hz
        }
    }
}

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

#[derive(Clone)]
pub enum SystemState {
    Inactive,
    Active,
}

#[derive(Clone)]
pub struct FighterBCScheduler {
    // to be used for BC as its event handler
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

impl FighterBCScheduler {
    pub fn new() -> Self {
        use Address::*;
        use MsgPri::*;
        let fighter_schedule: Vec<Event> = vec![
            //F-18 schedule
            // With feedback
            // Event {source: FlightControls, destination: Trim,  priority: Low,  repeating: true,
            //     word_count: 2, //one float32 should carry sufficient data
            // },
            // Event {source: Trim,   destination: FlightControls,    priority: Low,  repeating: true,
            //     word_count: 2,//one float32 should carry sufficient data
            // },
            Event {
                source: FlightControls,
                destination: Flaps,
                priority: Low,
                repeating: true,
                word_count: 2, //1,
            },
            // Event {source: Flaps,  destination: FlightControls,    priority: Low,  repeating: true,
            //     word_count: 1,
            // },
            Event {
                source: FlightControls,
                destination: Engine,
                priority: VeryHigh,
                repeating: true,
                word_count: 2, //16, //We'll estimate a float32 for each of the engines (up to four engines) and 2 words per float32
            },
            // Event {source: Engine, destination: FlightControls,    priority: High, repeating: true,
            //     word_count: 16, //Temperature, speed,
            // },
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
            Event {
                source: FlightControls,
                destination: Rudder,
                priority: VeryHigh,
                repeating: true,
                word_count: 4, //2,//float32 for degree
            },
            Event {
                source: Rudder,
                destination: FlightControls,
                priority: VeryHigh,
                repeating: true,
                word_count: 2, //2,//float32 for degree
            },
            // Without feedback
            Event {
                source: FlightControls,
                destination: Ailerons,
                priority: VeryHigh,
                repeating: true,
                word_count: 8, //4,//float32 for degree on each wing
            },
            // Event {source: FlightControls, destination: Elevators, priority: VeryHigh, repeating: true,
            //     word_count: 8, //4,//float32 for degree on each wing
            // },
            // Event {source: FlightControls, destination: Slats, priority: VeryHigh, repeating: true,
            //     word_count: 4,//float32 for degree on each wing
            // },
            // Event {source: FlightControls, destination: Spoilers,  priority: VeryHigh, repeating: true,
            //     word_count: 2, //4,//float32 for degree on each wing
            // },
            // Sensors
            Event {
                source: Fuel,
                destination: FlightControls,
                priority: Lowest,
                repeating: true,
                word_count: 4, //one float32 for quantity, one float32 for flow
            },
            // Event {source: Gyro, destination: FlightControls,    priority: Medium,   repeating: true,
            //     word_count: 10, //one float32 for heading
            // },
            // Event {source: Altimeter,  destination: FlightControls,    priority: Medium,   repeating: true,
            //     word_count: 1,
            // },
            Event {
                source: Positioning,
                destination: FlightControls,
                priority: Lowest,
                repeating: true,
                word_count: 6, // Lat, Long, Alt
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
        let mut scheduler = FighterBCScheduler {
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
        for event in fighter_schedule {
            // should we randomize the events?  This would make the time series analysis a little different
            scheduler.priority_list.push(event, 0);
        }
        scheduler.landing_gear_events.push(Event {
            source: FlightControls,
            destination: Brakes,
            priority: High,
            repeating: true,
            word_count: 4, //float32 for degree on each side
        });
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

    fn update_priority(&mut self, event: Event, time: u128) {
        let delay = event.priority.delay() as u128;
        let next_time = time + delay;
        self.priority_list.push(event, next_time);
    }

    fn bus_available(&mut self) -> u128 {
        self.timeout
    }
}

impl EventHandler for FighterBCScheduler {
    fn on_bc_ready(&mut self, d: &mut Device) /*-> Option<String>*/
    {
        // We pop the next message and wait until we should send it. This cannot be preempted, but that shouldn't be a problem.
        // SR bits should only come during a message requested by the bus controller.
        let spin_sleeper = spin_sleep::SpinSleeper::new(100_000);
        let message = self.priority_list.pop_min();
        match message {
            Some((
                Event {
                    source: src,
                    destination: dst,
                    priority: pri,
                    repeating: repeat,
                    word_count: wc,
                },
                mut time,
            )) => {
                self.current_event = Some(Event {
                    source: src,
                    destination: dst,
                    priority: MsgPri::Immediate,
                    repeating: false,
                    word_count: wc,
                });
                time = if time > self.timeout {
                    time
                } else {
                    self.timeout
                };
                let mut current = d.clock.elapsed().as_nanos();
                if time >= current {
                    let wait = time - current;
                    // spin_sleeper.sleep_ns(wait.try_into().unwrap());
                    current = d.clock.elapsed().as_nanos();
                }
                match (src, dst) {
                    (source, _) if source as u8 == d.address => {
                        // BC to RT
                        if repeat {
                            self.update_priority(
                                Event {
                                    source: src,
                                    destination: dst,
                                    priority: pri,
                                    repeating: repeat,
                                    word_count: wc,
                                },
                                time,
                            );
                        }
                        // d.act_bc2rt(dst as u8, wc); // Can't be wordcount, must be data.  We don't know what data we want to send, that's the Device itself.
                        let bus_available = current + (2 + wc as u128) * 400_000;
                        self.timeout = bus_available;
                        //Some(format!("[{:0>6?}] from {src:?} to {dst:?} with {src:?} as BC\n[{:0>6?}] - message finished\n", current/1000, bus_available/1000))
                    }
                    (_, destination) if destination as u8 == d.address => {
                        // RT to BC
                        if repeat {
                            self.update_priority(
                                Event {
                                    source: src,
                                    destination: dst,
                                    priority: pri,
                                    repeating: repeat,
                                    word_count: wc,
                                },
                                time,
                            );
                        }
                        d.act_rt2bc(src as u8, wc);
                        let bus_available = current + (2 + wc as u128) * 400_000;
                        self.timeout = bus_available;
                        //Some(format!("[{:0>6?}] from {src:?} to {dst:?} with {dst:?} as BC\n[{:0>6?}] - message finished\n", current/1000, bus_available/1000))
                    }
                    _ => {
                        // RT to RT
                        if repeat {
                            self.update_priority(
                                Event {
                                    source: src,
                                    destination: dst,
                                    priority: pri,
                                    repeating: repeat,
                                    word_count: wc,
                                },
                                time,
                            );
                        }
                        d.act_rt2rt(src as u8, dst as u8, wc);
                        let bus_available = current + (4 + wc as u128) * 4_000_000;
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

    fn on_sts(&mut self, d: &mut Device, w: &mut Word) {
        let rt = w.address();
        if w.message_errorbit() != 0 {
            match self.current_event {
                Some(event) => {
                    self.priority_list.push(event, 0);
                }
                _ => {}
            }
        } else if w.service_request_bit() != 0 {
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
        self.default_on_sts(d, w);
    }
}

pub struct FighterDeviceEventHandler {
    // to be used for RT.
    pub data: LinkedList<(u32, Vec<u16>)>,
    address: Address,
    time_offset: u128,
    current_data: Option<Vec<u16>>,
    latest_timestamp: u128,
    destination: Option<Address>,
    pub total_recv_count: u32,
}

impl FighterDeviceEventHandler {
    fn new(database: &str, component: Address, fields: &str) -> Self {
        let conn = Connection::open(database).unwrap();
        let num_fields = fields.matches(",").count();
        let command = format!(
            "SELECT elapsed_ms, delta_ms, absolute_time{:} FROM sensor_data",
            fields
        );
        // println!("Executing command: {:?}", command);
        let mut results = conn.prepare(&command[..]).unwrap();
        let data_iter = results.query_map([], |row| {
            let time: u32 = row.get(0)?;
            let mut data: Vec<u16> = Vec::new();
            for i in 0..num_fields {
                let field_content: f32 = row.get(3 + i)?;
                for slice in SplitInt::new(field_content.to_bits()).extract() {
                    // extract 16bits ints from 32bit floats
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
                // data_vec.push_back((content.0, (content.1.into_iter().map(Word::new_data).collect()))), // Words are ready to be sent out.  This change seems to have slowed things down.  I will investigate further.
                _ => {}
            }
        }
        let handler = FighterDeviceEventHandler {
            address: component,
            data: data_vec,
            time_offset: 0,
            current_data: None,
            latest_timestamp: 0,
            destination: None,
            total_recv_count: 0,
        };
        handler
    }
}

impl EventHandler for FighterDeviceEventHandler {
    fn on_cmd(&mut self, d: &mut Device, w: &mut Word) {
        self.default_on_cmd(d, w);
        if self.address == Address::FlightControls {
            if w.tr() == TR::Receive {
                self.destination = Some(Address::from(w.address()));
            }
        }
    }
    fn on_memory_ready(&mut self, d: &mut Device) {
        // data ready in memory
        self.total_recv_count += 1;
        let current_herz =
            ((self.total_recv_count * 1_000_000) as f64) / (d.clock.elapsed().as_micros() as f64);
        d.log(
            WRD_EMPTY,
            ErrMsg::MsgFlight(format!(
                "{:.4} Hz {:?} Recieved {:?}",
                current_herz, self.address, d.memory
            )),
        );
    }
    fn on_data_write(&mut self, d: &mut Device, dword_count: u8) {
        // todo: check dword_count matches self.current_data
        let current_time = d.clock.elapsed().as_millis() - self.time_offset;
        while (self.latest_timestamp < current_time && !self.data.is_empty())
            || self.current_data == None
        {
            let new_data = self.data.pop_front().unwrap();
            self.current_data = Some(new_data.1);
        }
        let write_buffer;
        if self.address == Address::FlightControls {
            use Address::*;
            write_buffer = match self.destination {
                Some(Rudder) => &self.current_data.as_ref().unwrap()[8..12],
                Some(Brakes) => &self.current_data.as_ref().unwrap()[12..16],
                Some(Engine) => &self.current_data.as_ref().unwrap()[16..18],
                Some(Spoilers) => &self.current_data.as_ref().unwrap()[18..20],
                Some(Flaps) => &self.current_data.as_ref().unwrap()[20..22],
                _ => &self.current_data.as_ref().unwrap()[..8],
            };
        } else {
            write_buffer = self.current_data.as_ref().unwrap();
        }
        for data in write_buffer {
            d.write(Word::new_data(*data as u32));
        }
        d.log(
            WRD_EMPTY,
            ErrMsg::MsgFlight(format!("{:?} Sending {:?}", self.address, write_buffer)),
        );
    }
}

pub fn eval_fighter_sim(database: &str, w_delays: u128, run_time: u64, attack: AttackType) {
    // let database = "sample_data.sqlite";
    let devices = vec![
        Address::BusControl,
        Address::FlightControls,
        Address::Flaps,
        Address::Engine,
        Address::Rudder,
        Address::Ailerons,
        // Address::// Elevators,
        // Address::// Spoilers,
        Address::Fuel,
        Address::Positioning,
        // Address::// Gyro,
        Address::Brakes,
        Address::BusMonitor,
        Address::AttackController,
    ];
    let total_devices = devices.len() as u32;
    let mut sys = System::new(total_devices, w_delays);

    let mut attack_controller = AttackController {
        current_attack: AttackType::Benign,
        emitter: Arc::new(Mutex::new(EventHandlerEmitter {
            handler: Box::new(DefaultEventHandler {}),
        })),
    };

    for d in devices {
        let emitter = match d {
            Address::BusControl => Arc::new(Mutex::new(EventHandlerEmitter {
                handler: Box::new(FighterBCScheduler::new()),
            })),
            Address::BusMonitor => Arc::new(Mutex::new(EventHandlerEmitter {
                handler: Box::new(DefaultEventHandler {}),
            })),
            Address::AttackController => Arc::clone(&attack_controller.emitter),
            _ => {
                let fields = match d {
                    Address::BusControl => "",
                    Address::FlightControls => ", yoke_x_position, yoke_y_position, yoke_x_indicator, yoke_y_indicator, rudder_pedal_position, rudder_pedal_indicator, brakes_right_position, brakes_left_position, throttle_level_position1, spoiler_handle_position, flaps_handle_percent",
                    Address::Flaps => "",
                    Address::Engine => "",
                    Address::Rudder => ", rudder_position",
                    Address::Ailerons => "",
                    Address::Elevators => "",
                    Address::Spoilers => "",
                    Address::Fuel => ", fuel_total_quantity, estimated_fuel_flow",
                    Address::Positioning => ", gps_latitude, gps_longitude, gps_altitude",
                    Address::Gyro => ", plane_pitch, plane_bank, incidence_alpha, incidence_beta, plane_heading_gyro",
                    Address::Brakes => "",
                    _ => "",
                };
                Arc::new(Mutex::new(EventHandlerEmitter {
                    handler: Box::new(FighterDeviceEventHandler::new(database, d, fields)),
                }))
            }
        };
        let mode = match d {
            Address::BusControl => Mode::BC,
            Address::BusMonitor => Mode::BM,
            _ => Mode::RT,
        };
        sys.run_d(d as u8, mode, emitter, d == Address::AttackController);
    }

    let attack_time = 30;
    let keep_time = run_time - attack_time;
    sys.go();
    sys.sleep_ms(attack_time);
    // we can add as many as attacks but some may not appear (due to the previous attacks).
    attack_controller.sabotage(
        attack,
        Address::FlightControls as u8,
        Address::FlightControls as u8,
    );
    sys.sleep_ms(keep_time);
    sys.stop();
    sys.join();
}

#[cfg(tests)]
mod tests {
    use super::*;

    fn test_address_functions() {
        for (src, dst) in [
            (Address::FlightControls, Address::Trim),
            (Address::Trim, Address::FlightControls),
            (Address::FlightControls, Address::Flaps),
            (Address::Flaps, Address::FlightControls),
            (Address::FlightControls, Address::Engine),
            (Address::Engine, Address::FlightControls),
            (Address::FlightControls, Address::LandingGear),
            (Address::LandingGear, Address::FlightControls),
            (Address::FlightControls, Address::Weapons),
            (Address::Weapons, Address::FlightControls),
            // Without Feedback
            (Address::FlightControls, Address::Rudder),
            (Address::FlightControls, Address::Ailerons),
            (Address::FlightControls, Address::Elevators),
            (Address::FlightControls, Address::Slats),
            (Address::FlightControls, Address::Spoilers),
            (Address::FlightControls, Address::Brakes),
            //Sensors
            (Address::Fuel, Address::FlightControls),
            (Address::Heading, Address::FlightControls),
            (Address::Altimeter, Address::FlightControls),
            (Address::Positioning, Address::FlightControls),
            (Address::Pitch, Address::FlightControls),
        ] {
            println!("src: {:?}, dst: {:?}", src, dst);
            println!("priority: {:?}", src.priority(&dst));
            println!("repeating: {:?}", src.repeat_function(&dst));
            if src.repeat_function(&dst) {
                println!(
                    "Repeat freq: {:?}Hz",
                    1.0 / (src.priority(&dst).delay() as f32 / 1_000_000_000.0)
                );
            }
            println!("word_count: {:?}", src.word_count(&dst));
            println!("------------------")
        }
    }
}
