use std::fmt;
use crate::primitive_types::{Mode, State, Word, ErrMsg, AttackType, WRD_EMPTY, TR};
use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError, RecvError, select, after};
use std::time::{Duration, Instant};
use std::collections::LinkedList;

pub const CONFIG_PRINT_LOGS: bool = false;

pub fn format_log(l: &(u128, Mode, u32, u8, State, Word, ErrMsg, u128)) -> String {
    return format!(
        "{} {}{:02}-{:02} {:^22} {} {:^22} avg_d_t:{}",
        l.0,
        l.1,
        l.2,
        l.3,
        l.4.to_string(),
        l.5,
        l.6.value(),
        l.7
    );
}

#[derive(Clone, Debug)]
pub struct Device {
    pub fake: bool,
    pub atk_type: AttackType,
    pub ccmd: u8,
    pub mode: Mode,
    pub state: State,
    pub error_bit: bool,
    pub service_request: bool,
    pub memory: Vec<u16>,
    pub number_of_current_cmd: u8,
    pub in_brdcst: bool,
    pub address: u8,
    pub id: u32,
    pub dword_count: u8,
    pub dword_count_expected: u8,
    pub clock: Instant,
    pub logs: Vec<(u128, Mode, u32, u8, State, Word, ErrMsg, u128)>,
    pub transmitters: Vec<Sender<Word>>,
    pub read_queue: Vec<(u128, Word, bool)>,
    pub write_queue: LinkedList<(u128, Word)>,
    pub write_delays: u128,
    pub receiver: Receiver<Word>,
    pub delta_t_avg: u128,
    pub delta_t_start: u128,
    pub delta_t_count: u128,
}

impl Device {
    pub fn write(&mut self, mut val: Word) {
        // println!("writing {} {}", val, val.sync());
        // for (i, s) in self.transmitters.iter().enumerate() {
        //     if (i as u32) != self.id {
        //         s.try_send(val);
        //         // s.send(val);
        //     }
        // }
        if self.fake {
            val.set_attk(self.atk_type as u8);
        }
        if self.write_queue.len() < 1 {
            self.write_queue
                .push_back((self.clock.elapsed().as_nanos(), val));
        } else {
            // println!("here {} {} {:?}, {}", self, self.write_queue.len(), self.write_queue.last().unwrap().0, self.write_delays);
            self.write_queue
                .push_back((self.write_queue.back().unwrap().0 + self.write_delays, val));
        }
        // let transmitters = self.transmitters.clone();
        // let id = self.id.clone();
        // thread::spawn(move || {
        //     for (i, s) in transmitters.iter().enumerate() {
        //         if (i as u32) != id {
        //             s.try_send(val);
        //             // s.send(val);
        //         }
        //     }
        // });
    }

    pub fn read(&self) -> Result<Word, TryRecvError> {
        // return self.receiver.recv().unwrap();
        return self.receiver.try_recv();
    }

    pub fn maybe_block_read(&self) -> Result<Word, TryRecvError> {
        if self.mode == Mode::BC {
            self.receiver.try_recv()
            // if self.state == State::Idle {
            //     self.receiver.try_recv()
            // } else {
            //     select! {  // This made this so much worse
            //         recv(self.receiver) -> message => match message{Ok(word) => Ok(word), Err(_) => Err(TryRecvError::Empty)},
            //         recv(after(Duration::from_nanos(14))) -> _ => Err(TryRecvError::Empty),
            //     }
            // }
        } else if [State::Idle, State::AwtData].contains(&self.state) {
            self.receiver.try_recv()
        } else {
            let message = self.receiver.recv();
            match message {
                Ok(word) => Ok(word),
                Err(_) => Err(TryRecvError::Empty),
            }
        }
    }

    pub fn reset_all_stateful(&mut self) -> u8 {
        let current_cmd = self.number_of_current_cmd;
        self.set_state(State::Idle);
        self.number_of_current_cmd = 0;
        self.delta_t_start = 0;
        self.memory.clear();
        self.dword_count = 0;
        self.dword_count_expected = 0;
        self.in_brdcst = false;
        // return the previous number of cmd
        // in case it shouldn't be reseted.
        return current_cmd;
    }

    pub fn log(&mut self, word: Word, e: ErrMsg) {
        let mut avg_delta_t = 0;
        if self.delta_t_count > 0 {
            avg_delta_t = self.delta_t_avg / self.delta_t_count;
        }
        let l = (
            self.clock.elapsed().as_micros(), // .as_nanos(),
            self.mode,
            self.id,
            self.address,
            self.state,
            word,
            e,
            avg_delta_t,
        );
        if CONFIG_PRINT_LOGS {
            println!("{}", format_log(&l));
        }
        self.logs.push(l);
    }

    pub fn log_merge(&self, log_list: &mut Vec<(u128, Mode, u32, u8, State, Word, ErrMsg, u128)>) {
        for l in &self.logs {
            log_list.push(l.clone());
        }
    }

    pub fn set_state(&mut self, state: State) {
        self.state = state;
        self.log(WRD_EMPTY, ErrMsg::MsgStaChg(self.write_queue.len()));
    }

    pub fn act_bc2rt(&mut self, dest: u8, data: &Vec<u16>) {
        self.set_state(State::BusyTrx);
        self.write(Word::new_cmd(dest, data.len() as u8, TR::Receive));
        for d in data {
            self.write(Word::new_data(*d));
        }
        self.set_state(State::AwtStsRcvB2R(dest));
        self.delta_t_start = self.clock.elapsed().as_nanos();
    }

    #[allow(unused)]
    pub fn act_bc2rt_wc(&mut self, dest: u8, dword_count: u8) {
        // This function is so that the "scheduler" does not need to know the data that will be passed from the BC.
        self.set_state(State::BusyTrx);
        self.write(Word::new_cmd(dest, dword_count, TR::Receive));
        // for d in data {
        //     self.write(Word::new_data(*d));
        // }
        self.set_state(State::AwtStsRcvB2R(dest));
        self.delta_t_start = self.clock.elapsed().as_nanos();
    }

    pub fn act_rt2bc(&mut self, src: u8, dword_count: u8) {
        self.set_state(State::BusyTrx);
        self.write(Word::new_cmd(src, dword_count, TR::Transmit));
        // expecting to recieve dword_count number of words
        self.dword_count_expected = dword_count;
        self.set_state(State::AwtStsTrxR2B(src));
        self.delta_t_start = self.clock.elapsed().as_nanos();
    }
    
    pub fn act_rt2rt(&mut self, src: u8, dst: u8, dword_count: u8) {
        self.set_state(State::BusyTrx);
        self.write(Word::new_cmd(dst, dword_count, TR::Receive));
        self.write(Word::new_cmd(src, dword_count, TR::Transmit));
        // expecting to recieve dword_count number of words
        self.set_state(State::AwtStsTrxR2R(src, dst));
        self.delta_t_start = self.clock.elapsed().as_nanos();
    }
}

impl fmt::Display for Device {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}-{}", self.mode, self.address, self.state)
    }
}