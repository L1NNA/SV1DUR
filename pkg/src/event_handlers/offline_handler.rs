use crate::primitive_types::{Word, ErrMsg, State, Mode, TR, WRD_EMPTY, Address};
use crate::devices::Device;
use crate::terminals::ComponentInfo;
use crate::event_handlers::EventHandler;
use std::collections::LinkedList;

pub const BROADCAST_ADDRESS: u8 = 31;

#[derive(Clone)]
pub struct OfflineHandler {
    pub data: LinkedList<(u32, Vec<u16>)>,
    time_offset: u128,
    current_data: Option<Vec<u16>>,
    latest_timestamp: u128,
}

impl OfflineHandler {
    pub fn new(data: LinkedList<(u32, Vec<u16>)>) -> OfflineHandler {
        let handler = OfflineHandler{data: data, time_offset: 0, current_data: None, latest_timestamp: 0,};
        handler
    }
}

impl EventHandler for OfflineHandler {
    fn on_cmd_trx(&mut self, d: &mut Device, w: &mut Word) {
        // may be triggered after cmd
        d.log(*w, ErrMsg::MsgEntCmdTrx);
        if self.time_offset == 0 {
            self.time_offset = d.clock.elapsed().as_millis();
        }
        if !d.fake {
            d.set_state(State::BusyTrx);
            let current_time = d.clock.elapsed().as_millis() - self.time_offset;
            while self.current_data == None || (self.latest_timestamp < current_time && !self.data.is_empty()) {
                let new_data = self.data.pop_front().unwrap();
                self.current_data = Some(new_data.1);
            }
            d.write(Word::new_status(d.address, d.service_request, d.error_bit));
            for data in self.current_data.as_ref().unwrap() {
                d.write(Word::new_data(*data));
            }
        }
        let current_cmds = d.reset_all_stateful();
        d.number_of_current_cmd = current_cmds;
    }
}

#[derive(Clone)]
pub struct OfflineFlightControlsHandler {
    pub data: LinkedList<(u32, Vec<u16>)>,
    time_offset: u128,
    current_data: Option<Vec<u16>>,
    latest_timestamp: u128,
    destination: Option<Address>,
}

impl OfflineFlightControlsHandler {
    pub fn new(data: LinkedList<(u32, Vec<u16>)>) -> OfflineFlightControlsHandler {
        let handler = OfflineFlightControlsHandler{data: data, time_offset: 0, current_data: None, latest_timestamp: 0, destination: None};
        handler
    }
}

impl EventHandler for OfflineFlightControlsHandler {
    fn on_cmd(&mut self, d: &mut Device, w: &mut Word) {
        // cmds are only for RT, matching self's address
        if d.mode == Mode::RT {
            let destination = w.address();
            // 31 is the boardcast address
            if destination == d.address || destination == BROADCAST_ADDRESS {
                // d.log(*w, ErrMsg::MsgEntCmd);
                // println!("{} {} {}", w, w.tr(), w.mode());
                d.number_of_current_cmd += 1;
                // if there was previously a command word recieved
                // cancel previous command (clear state)
                if d.number_of_current_cmd >= 2 {
                    // cancel whatever going to write
                    d.write_queue.clear();
                    d.reset_all_stateful();
                }
                if w.tr() == TR::Receive && (w.mode() == 1 || w.mode() == 0) {
                    // shutdown etc mode change command
                    self.on_cmd_mcx(d, w);
                } else {
                    if w.tr() == TR::Receive {
                        // receive command
                        self.on_cmd_rcv(d, w);
                    } else {
                        // transmission command
                        // faked device only mimic events but not responding
                        self.on_cmd_trx(d, w);
                    }
                }
            } else {
                if w.tr() == TR::Receive {
                    self.destination = Some(Address::from(w.address()));
                }
            }
            // rt2rt sub destination
            if w.tr() == TR::Transmit && w.sub_address() == d.address {
                self.on_cmd_rcv(d, w);
            }
        }
    }

    fn on_cmd_trx(&mut self, d: &mut Device, w: &mut Word) {
        // may be triggered after cmd
        d.log(*w, ErrMsg::MsgEntCmdTrx);
        if self.time_offset == 0 {
            self.time_offset = d.clock.elapsed().as_nanos();
        }
        if !d.fake {
            d.set_state(State::BusyTrx);
            let current_time = d.clock.elapsed().as_nanos() - self.time_offset;
            while self.latest_timestamp < current_time && !self.data.is_empty() {
                let new_data = self.data.pop_front().unwrap();
                self.current_data = Some(new_data.1);
            }
            d.write(Word::new_status(d.address, d.service_request, d.error_bit));
            let mut data_words: Vec<Word> = Vec::new();
            use Address::*;
            let slice = match self.destination {
                Some(Rudder) => &self.current_data.as_ref().unwrap()[8..12],
                Some(Brakes) => &self.current_data.as_ref().unwrap()[12..16],
                Some(Engine) => &self.current_data.as_ref().unwrap()[16..18],
                Some(Spoilers) => &self.current_data.as_ref().unwrap()[18..20],
                Some(Flaps) => &self.current_data.as_ref().unwrap()[20..22],
                _ => &self.current_data.as_ref().unwrap()[..8]
            };
            for data in slice {
                d.write(Word::new_data(*data));
            }
        }
        let current_cmds = d.reset_all_stateful();
        d.number_of_current_cmd = current_cmds;
    }
}