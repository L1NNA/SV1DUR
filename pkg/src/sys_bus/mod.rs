use bitfield::bitfield;
use chrono::Utc;
use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, Sender, TryRecvError};
use spin_sleep;
use std::collections::VecDeque;
use std::fmt;
use std::fs::{create_dir, read_dir, File, OpenOptions};
use std::io::prelude::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
pub const WRD_EMPTY: Word = Word { 0: 0 };
pub const ATK_DEFAULT_DELAYS: u128 = 4_000;
pub const CONFIG_PRINT_LOGS: bool = false;
pub const CONFIG_SAVE_DEVICE_LOGS: bool = false;
pub const CONFIG_SAVE_SYS_LOGS: bool = true;
pub const BROADCAST_ADDRESS: u8 = 31;
pub const RT_WORD_LOAD_TIME: u128 = 20_000;
pub const BC_WARMUP_STEPS: u128 = 20;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};
use num_format::{Locale, ToFormattedString};

#[allow(unused)]
#[derive(Clone, Debug, PartialEq)]
pub enum ErrMsg {
    MsgEmpt,
    // show write queue size
    MsgWrt(usize),
    MsgBCReady,
    // show write queue size
    MsgStaChg(usize),
    MsgEntWrdRec,
    MsgEntErrPty(i128, i128),
    MsgEntCmd,
    MsgEntCmdRcv,
    MsgEntCmdTrx,
    MsgEntCmdMcx,
    MsgEntDat,
    MsgEntSte,
    // dropped status word
    MsgEntSteDrop,
    MsgAttk(String),
    MsgMCXClr(usize),
    // MB log
    MsgBMLog,
    // Flight system level log
    MsgFlight(String),
    MsgBCTimeout(u128),
}

impl ErrMsg {
    fn value(&self) -> String {
        use ErrMsg::*;
        match self {
            MsgEmpt => "".to_owned(),
            // show write queue size
            MsgWrt(wq) => format!("Wrt({})", wq).to_string(),
            MsgBCReady => "BC is ready".to_owned(),
            MsgStaChg(wq) => format!("Status Changed({})", wq).to_string(),
            MsgEntWrdRec => "Word Received".to_owned(),
            MsgEntErrPty(recv, lag) => format!(
                "Parity Error({} {})",
                recv.to_formatted_string(&Locale::en),
                lag
            )
            .to_string(),
            MsgEntCmd => "CMD Received".to_owned(),
            MsgEntCmdRcv => "CMD RCV Received".to_owned(),
            MsgEntCmdTrx => "CMD TRX Received".to_owned(),
            MsgEntCmdMcx => "CMD MCX Received".to_owned(),
            MsgEntDat => "Data Received".to_owned(),
            MsgEntSte => "Status Received".to_owned(),
            MsgEntSteDrop => "Status Dropped".to_owned(),
            MsgAttk(msg) => msg.to_owned(),
            // mode change
            MsgMCXClr(mem_len) => format!("MCX[{}] Clr", mem_len),
            MsgBMLog => "BM".to_owned(),
            MsgBCTimeout(timeout) => format!("BC Timeout {}", timeout).to_string(),
            MsgFlight(msg) => msg.to_owned(),
        }
    }
}

pub fn format_log(l: &(u128, Mode, u32, u8, State, Word, ErrMsg, u128)) -> String {
    return format!(
        "{:>12} {}{:02}-{:02} {:^22} {} {:^22} avg_d_t:{}",
        l.0.to_formatted_string(&Locale::en),
        l.1,
        l.2,
        l.3,
        l.4.to_string(),
        l.5,
        l.6.value(),
        l.7
    );
}

pub fn format_log_bm(l: &(u128, Mode, u32, u8, State, Word, ErrMsg, u128)) -> String {
    // return format!("{} {:?}", l.0, l.5,);
    return format!("{},{},{}, {}", l.0, l.5.all(), l.5.parity_bit(), l.5.attk());
}

#[derive(Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum TR {
    Receive = 0,
    Transmit = 1,
}

impl From<u8> for TR {
    fn from(value: u8) -> Self {
        use TR::*;
        match value {
            0 => Receive,
            _ => Transmit,
        }
    }
}

bitfield! {
    #[derive(Copy, Clone)]
    pub struct Word(u32);
    impl Debug;
    u8;
    // for status
    pub sync, set_sync: 2, 0;
    pub address, set_address: 7, 3;
    pub message_errorbit, set_message_errorbit: 8, 8;
    pub instrumentation_bit, set_instrumentation_bit: 9, 9;
    pub service_request_bit, set_service_request_bit: 10, 10;
    pub reserved_bits, set_reserved_bits: 13, 11;
    pub brdcst_received_bit, set_brdcst_received_bit: 14, 14;
    pub busy_bit, set_busy_bit: 15, 15;
    pub subsystem_flag_bit, set_subsystem_flag_bit: 16, 16;
    pub dynamic_bus_control_accpt_bit, set_dynamic_bus_control_accpt_bit: 17, 17;
    pub terminal_flag_bit, set_terminal_flag_bit: 18, 18;
    pub parity_bit, set_parity_bit: 19, 19;
    // for command:
    pub into TR, tr, set_tr: 8, 8;
    // it was 13, 9 but since we use instrumentation bit
    // we have kept reduce the sub-address space to 15.
    pub sub_address, set_sub_address: 13, 10;
    pub mode, set_mode: 13, 11;
    // pub mode, set_mode: 13, 9;
    pub dword_count, set_dword_count: 18, 14;
    pub mode_code, set_mode_code: 18, 14;
    // for data word
    u32;
    pub all,_ : 20, 0;
    pub data, set_data: 18, 3;
    // additional (attack type):
    pub attk, set_attk: 24,21;
}

impl fmt::Display for Word {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "w:{:#027b}[{:02}]", self.0, self.attk()) // We need an extra 2 bits for '0b' on top of the number of bits we're printing
    }
}

impl Word {
    pub fn new_status(src_addr: u8) -> Word {
        let mut w = Word { 0: 0 };
        w.set_sync(1);
        w.set_address(src_addr);
        w.calculate_parity_bit();
        return w;
    }

    pub fn new_data(val: u32) -> Word {
        let mut w = Word { 0: 0 };
        w.set_data(val as u32);
        w.calculate_parity_bit();
        return w;
    }

    pub fn new_cmd(addr: u8, dword_count: u8, tr: TR) -> Word {
        let mut w = Word { 0: 0 };
        w.set_sync(1);
        w.set_tr(tr as u8); // 1: transmit, 0: receive
        w.set_address(addr); // the RT address which is five bits long
                             // address 11111 (31) is reserved for broadcast protocol

        w.set_dword_count(dword_count); // the quantity of data that will follow after the command
        w.set_mode(2);
        w.set_instrumentation_bit(1);
        w.calculate_parity_bit();
        return w;
    }
    #[allow(unused)]
    pub fn calculate_parity_bit(&mut self) {
        /*
        This code will calculate and apply the parity bit.  This will not affect other bits in the bitfield.
        */
        // let mask = u32::MAX - 1; //MAX-1 leaves the paritybit empty (I think this assumption may be wrong.  I think this is actually the sync bits)
        let mask = u32::MAX - 2u32.pow(19); // This will likely be the code we need.  It keeps all of the bits outside of the "19" bit.
        let int = self.all() & mask;
        let parity_odd = true;
        if int.count_ones() % 2 == 0 {
            self.set_parity_bit(!parity_odd as u8);
        } else {
            self.set_parity_bit(parity_odd as u8);
        }
    }
}

#[allow(unused)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Mode {
    RT,
    BC,
    BM,
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[allow(unused)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum State {
    Idle,
    Off,
    Pause,
    // waiting for data
    AwtData,
    // transmitting (including artificial delays)
    BusyTrx,
    // bc2rt - bc waiting for reciever status code
    AwtStsRcvB2R(u8),
    // rt2bc - bc waiting for the transmitter status code
    AwtStsTrxR2B(u8),
    // rt2rt - bc waiting for reciever status code
    AwtStsRcvR2R(u8, u8),
    // rt2rt - bc waiting for the transmitter status code
    AwtStsTrxR2R(u8, u8),
}
impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub trait EventHandler: Send {
    fn on_wrd_rec(&mut self, d: &mut Device, w: &mut Word) {
        self.default_on_wrd_rec(d, w);
    }
    fn on_err_parity(&mut self, d: &mut Device, w: &mut Word, recv_time: i128, lag: i128) {
        self.default_on_err_parity(d, w, recv_time, lag);
    }
    fn on_cmd(&mut self, d: &mut Device, w: &mut Word) {
        self.default_on_cmd(d, w);
    }
    fn on_cmd_rcv(&mut self, d: &mut Device, w: &mut Word) {
        self.default_on_cmd_rcv(d, w);
    }
    fn on_cmd_trx(&mut self, d: &mut Device, w: &mut Word) {
        self.default_on_cmd_trx(d, w);
    }
    fn on_cmd_mcx(&mut self, d: &mut Device, w: &mut Word) {
        self.default_on_cmd_mcx(d, w);
    }
    fn on_dat(&mut self, d: &mut Device, w: &mut Word) {
        self.default_on_dat(d, w);
    }
    fn on_sts(&mut self, d: &mut Device, w: &mut Word) {
        self.default_on_sts(d, w);
    }
    fn on_bc_ready(&mut self, _: &mut Device) {}
    fn on_bc_timeout(&mut self, d: &mut Device) {
        self.default_on_bc_timeout(d);
    }
    fn on_memory_ready(&mut self, _: &mut Device) {}
    fn on_data_write(&mut self, d: &mut Device, dword_count: u8) {
        self.default_on_data_write(d, dword_count);
    }
    fn default_on_bc_timeout(&mut self, d: &mut Device) {
        d.log(WRD_EMPTY, ErrMsg::MsgBCTimeout(d.timeout));
        let mut reset_cmd = Word::new_cmd(BROADCAST_ADDRESS, 0, TR::Receive);
        reset_cmd.set_mode(1);
        reset_cmd.set_mode_code(30);
        d.write(reset_cmd);
    }
    fn default_on_data_write(&mut self, d: &mut Device, dword_count: u8) {
        for i in 0..dword_count {
            d.write(Word::new_data((i + 1) as u32));
        }
    }

    #[allow(unused)]
    fn default_on_wrd_rec(&mut self, d: &mut Device, w: &mut Word) {
        // for bm to monitor every word
        // d.log(*w, ErrMsg::MsgEntWrdRec);
    }
    #[allow(unused)]
    fn default_on_err_parity(&mut self, d: &mut Device, w: &mut Word, recv_time: i128, lag: i128) {
        // log error tba
        d.log(*w, ErrMsg::MsgEntErrPty(recv_time, lag));
    }
    fn default_on_cmd(&mut self, d: &mut Device, w: &mut Word) {
        // cmds are only for RT, matching self's address
        if d.mode == Mode::RT {
            let destination = w.address();
            // 31 is the boardcast address
            if destination == d.address || destination == BROADCAST_ADDRESS {
                // d.log(*w, ErrMsg::MsgEntCmd);
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
            }
            // rt2rt sub destination
            // if w.tr() == TR::Transmit && w.sub_address() == d.address {
            //     self.on_cmd_rcv(d, w);
            // }
        }
    }
    fn default_on_cmd_trx(&mut self, d: &mut Device, w: &mut Word) {
        // may be triggered after cmd
        d.log(*w, ErrMsg::MsgEntCmdTrx);
        if !d.fake {
            d.set_state(State::BusyTrx);
            d.write(Word::new_status(d.address));
            self.on_data_write(d, w.dword_count());
            // for i in 0..w.dword_count() {
            //     d.write(Word::new_data((i + 1) as u32));
            // }
        }
        let current_cmds = d.reset_all_stateful();
        d.number_of_current_cmd = current_cmds;
    }
    fn default_on_cmd_rcv(&mut self, d: &mut Device, w: &mut Word) {
        d.log(*w, ErrMsg::MsgEntCmdRcv);
        // may be triggered after cmd
        d.set_state(State::AwtData);
        d.dword_count = 0;
        d.dword_count_expected = w.dword_count();
        if w.address() == BROADCAST_ADDRESS {
            d.in_brdcst = true;
        }
    }
    fn default_on_cmd_mcx(&mut self, d: &mut Device, w: &mut Word) {
        if d.address == w.address() || w.address() == BROADCAST_ADDRESS {
            d.log(*w, ErrMsg::MsgEntCmdMcx);
            // may be triggered after cmd
            if !d.fake {
                // actual operation not triggerred for attackers
                // mode code match for command:
                match w.mode_code() {
                    4 => {
                        // Mode code for TX shutdown
                        d.reset_all_stateful();
                        d.set_state(State::Off);
                    }
                    17 => {
                        // synchronization
                        // ccmd indicating that the next data word
                        // is related to the current command
                        // (in this case, the clock to be synced)
                        d.ccmd = 1;
                        d.set_state(State::AwtData);
                    }
                    30 => {
                        // clear cache (only when it is recieving data)
                        d.log(WRD_EMPTY, ErrMsg::MsgMCXClr(d.memory.len()));
                        d.reset_all_stateful();
                        d.set_state(State::Idle);
                        // clear write queue (cancel the status words to be sent)
                        d.write_queue.clear();
                    }
                    31 => {
                        // cancel operation
                        d.set_state(State::Idle);
                    }
                    _ => {}
                }
            }
        }
    }
    fn default_on_dat(&mut self, d: &mut Device, w: &mut Word) {
        if d.state == State::AwtData {
            d.log(*w, ErrMsg::MsgEntDat);
            if d.ccmd == 1 {
                // TBA:  synchronize clock to data
                // (clock is u128 but data is not u16..)
                // maybe set the microscecond component of the clock
                d.ccmd = 0;
            } else {
                if d.dword_count < d.dword_count_expected {
                    d.memory.push(w.data());
                }
                d.dword_count += 1;
                if d.dword_count == d.dword_count_expected {
                    d.set_state(State::BusyTrx);
                    if d.mode != Mode::BC {
                        // only real RT will responding status message
                        if !d.fake {
                            d.write(Word::new_status(d.address));
                        }
                    }
                    self.on_memory_ready(d);
                    d.reset_all_stateful();
                }
            }
        }
    }
    fn default_on_sts(&mut self, d: &mut Device, w: &mut Word) {
        if d.mode == Mode::BC {
            d.log(*w, ErrMsg::MsgEntSte);
            // check delta_t
            let mut check_delta_t = false;
            match d.state {
                State::AwtStsTrxR2B(src) => {
                    //(transmitter confirmation)
                    // rt2bc
                    if src == w.address() {
                        d.set_state(State::AwtData)
                    }
                    check_delta_t = true;
                }
                State::AwtStsRcvB2R(dest) => {
                    // rt2rt (reciver confirmation)
                    // bc2rt
                    if dest == w.address() {
                        d.reset_all_stateful();
                    }
                }
                State::AwtStsTrxR2R(src, dest) => {
                    //(transmitter confirmation)
                    // rt2rt
                    if src == w.address() {
                        d.set_state(State::AwtStsRcvR2R(src, dest));
                        d.delta_t_start = d.clock.elapsed().as_nanos();
                    }
                    check_delta_t = true;
                }
                State::AwtStsRcvR2R(src, dest) => {
                    // rt2rt (reciver confirmation)
                    // rt2rt
                    if dest == w.address() {
                        d.reset_all_stateful();
                    }
                }
                _ => {
                    // dropped status word
                    d.log(*w, ErrMsg::MsgEntSteDrop);
                }
            }
            if check_delta_t && d.delta_t_start != 0 {
                let delta_t = d.clock.elapsed().as_nanos() - d.delta_t_start;
                // delta_t has to be in between 4 and 12
                d.delta_t_avg += delta_t;
                d.delta_t_count += 1;
            }
        }
    }
    fn verify(&mut self, _: &System) -> bool {
        false
    }
    fn get_attk_type(&self) -> AttackType {
        AttackType::Benign
    }
}

#[derive(Clone, Debug)]
pub struct Device {
    pub fake: bool,
    pub atk_type: AttackType,
    pub ccmd: u8,
    pub mode: Mode,
    pub state: State,
    pub memory: Vec<u32>,
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
    pub write_queue: VecDeque<(u128, Word)>,
    pub write_delays: u128,
    pub receiver: Receiver<Word>,
    pub delta_t_avg: u128,
    pub delta_t_start: u128,
    pub delta_t_count: u128,
    pub timeout: u128,
    pub timeout_times: u128,
    pub time_write_ready: u128,
}

impl Device {
    pub fn write(&mut self, mut val: Word) {
        if self.fake {
            val.set_attk(self.atk_type as u32);
        }
        self.write_queue.push_back((0, val));
    }

    pub fn read(&self) -> Result<Word, RecvTimeoutError> {
        return self.receiver.recv_timeout(Duration::from_micros(5));
        // return self.receiver.try_recv();
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
        self.timeout = 0;
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
            self.clock.elapsed().as_nanos(),
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
        if state != self.state {
            self.state = state;
            self.log(WRD_EMPTY, ErrMsg::MsgStaChg(self.write_queue.len()));
        }
    }

    pub fn act_bc2rt(&mut self, dest: u8, data: &Vec<u32>) {
        self.set_state(State::BusyTrx);
        self.write(Word::new_cmd(dest, data.len() as u8, TR::Receive));
        for d in data {
            self.write(Word::new_data(*d));
        }
        self.set_state(State::AwtStsRcvB2R(dest));
        self.delta_t_start = self.clock.elapsed().as_nanos();
        // 12_000 is the max allowed RT write delays.
        // put 20_000 to include the queue transmission time.
        self.timeout = self.clock.elapsed().as_nanos()
            + (RT_WORD_LOAD_TIME + self.write_delays + 50_000) * (data.len() as u128 + 2);
    }
    pub fn act_rt2bc(&mut self, src: u8, dword_count: u8) {
        self.set_state(State::BusyTrx);
        self.write(Word::new_cmd(src, dword_count, TR::Transmit));
        // expecting to recieve dword_count number of words
        self.dword_count_expected = dword_count;
        self.set_state(State::AwtStsTrxR2B(src));
        self.delta_t_start = self.clock.elapsed().as_nanos();
        self.timeout = self.clock.elapsed().as_nanos()
            + (RT_WORD_LOAD_TIME + self.write_delays + 50_000) * (dword_count as u128 + 2);
    }
    pub fn act_rt2rt(&mut self, src: u8, dst: u8, dword_count: u8) {
        self.set_state(State::BusyTrx);
        self.write(Word::new_cmd(dst, dword_count, TR::Receive));
        self.write(Word::new_cmd(src, dword_count, TR::Transmit));
        // expecting to recieve dword_count number of words
        self.set_state(State::AwtStsTrxR2R(src, dst));
        self.delta_t_start = self.clock.elapsed().as_nanos();
        self.timeout = self.clock.elapsed().as_nanos()
            + (RT_WORD_LOAD_TIME + self.write_delays + 50_000) * (dword_count as u128 + 4);
    }
}

impl fmt::Display for Device {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}-{}", self.mode, self.address, self.state)
    }
}

pub struct System {
    pub n_devices: u32,
    pub max_devices: u32,
    pub transmitters: Vec<Sender<Word>>,
    pub receivers: Vec<Receiver<Word>>,
    pub clock: Instant,
    pub go: Arc<AtomicBool>,
    pub exit: Arc<AtomicBool>,
    pub handlers: Option<Vec<thread::JoinHandle<u32>>>,
    pub devices: Vec<Arc<Mutex<Device>>>,
    pub logs: Vec<(u128, Mode, u32, u8, State, Word, ErrMsg, u128)>,
    pub home_dir: String,
    pub write_delays: u128,
}

impl System {
    pub fn new(max_devices: u32, write_delays: u128) -> Self {
        let home_dir = Utc::now().format("%F-%H-%M-%S-%f").to_string();
        return System::new_with_name(max_devices, write_delays, home_dir);
    }
    pub fn new_with_name(max_devices: u32, write_delays: u128, home_dir: String) -> Self {
        let clock = Instant::now();

        if CONFIG_SAVE_DEVICE_LOGS || CONFIG_SAVE_SYS_LOGS {
            let _ = create_dir(PathBuf::from(&home_dir));
        }

        let mut sys_bus = System {
            n_devices: 0,
            max_devices: max_devices,
            transmitters: Vec::new(),
            receivers: Vec::new(),
            clock: clock,
            go: Arc::new(AtomicBool::new(false)),
            exit: Arc::new(AtomicBool::new(false)),
            handlers: Some(Vec::new()),
            home_dir: home_dir,
            write_delays: write_delays,
            devices: Vec::new(),
            logs: Vec::new(),
        };
        for _ in 0..sys_bus.max_devices {
            let (s1, r1) = bounded(0);
            // let (s1, r1) = unbounded();
            sys_bus.transmitters.push(s1);
            sys_bus.receivers.push(r1);
        }
        return sys_bus;
    }

    pub fn go(&mut self) {
        self.go.store(true, Ordering::Relaxed);
    }

    #[allow(unused)]
    pub fn pause(&mut self) {
        self.go.store(false, Ordering::Relaxed);
    }
    pub fn stop(&mut self) {
        self.exit.store(true, Ordering::Relaxed);
    }
    pub fn join(&mut self) {
        if let Some(handles) = self.handlers.take() {
            for h in handles {
                let _ = h.join();
            }
        } else {
            panic!("tried to join but no threads exist");
        }

        // println!("Merging logs...");
        for device_mx in &self.devices {
            let device = device_mx.lock().unwrap();
            if device.mode != Mode::BM {
                device.log_merge(&mut self.logs);
            }
        }

        self.logs.sort_by_key(|k| k.0);
        if CONFIG_SAVE_SYS_LOGS {
            let log_file = PathBuf::from(self.home_dir.clone()).join("sys_bus.log");
            let mut file = OpenOptions::new()
                .write(true)
                .append(true)
                .create(true)
                .open(log_file)
                .unwrap();
            for l in &self.logs {
                let _ = writeln!(file, "{}", format_log(&l));
            }
            let log_file = PathBuf::from(self.home_dir.clone()).join("sys_bus.flight.log");
            let mut file = OpenOptions::new()
                .write(true)
                .append(true)
                .create(true)
                .open(log_file)
                .unwrap();
            for l in &self.logs {
                match &l.6 {
                    ErrMsg::MsgFlight(msg) => {
                        let _ = writeln!(file, "{} {}", l.0, msg);
                    }
                    _ => {}
                }
            }
        }
    }
    pub fn sleep_ms(&mut self, ms: u64) {
        thread::sleep(Duration::from_millis(ms));
    }
    pub fn sleep_ms_progress(&mut self, mut ms: u64) {
        let mut next: u64 = 5;
        let sty = ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")
            .progress_chars("##-");

        let pbs = ProgressBar::new(ms);
        pbs.set_style(sty.clone());
        pbs.set_draw_target(ProgressDrawTarget::stdout());

        while ms != 0 {
            if next > ms {
                next = ms
            }
            thread::sleep(Duration::from_millis(next));
            ms = ms - next;
            // pbs.set_message(format!("{}", ms));
            pbs.inc(next);
        }
    }
    pub fn run_d(
        &mut self,
        addr: u8,
        mode: Mode,
        handler_emitter: Arc<Mutex<EventHandlerEmitter>>,
        fake: bool,
    ) {
        let transmitters = self.transmitters.clone();
        let receiver = self.receivers[self.n_devices as usize].clone();
        let mut w_delay = self.write_delays;
        if fake {
            w_delay = ATK_DEFAULT_DELAYS;
        }
        let device_obj = Device {
            fake: fake,
            atk_type: AttackType::Benign,
            ccmd: 0,
            state: State::Idle,
            mode: mode,
            memory: Vec::new(),
            logs: Vec::new(),
            number_of_current_cmd: 0,
            in_brdcst: false,
            address: addr,
            id: self.n_devices,
            dword_count: 0,
            dword_count_expected: 0,
            clock: self.clock,
            transmitters: transmitters,
            write_queue: VecDeque::new(),
            read_queue: Vec::new(),
            receiver: receiver,
            delta_t_avg: 0,
            delta_t_count: 0,
            delta_t_start: 0,
            write_delays: w_delay,
            timeout: 0,
            timeout_times: 0,
            time_write_ready: 0,
        };
        let device_name = format!("{}", device_obj);
        let go = Arc::clone(&self.go);
        let exit = Arc::clone(&self.exit);
        let log_file = PathBuf::from(self.home_dir.clone()).join(format!("{}.log", device_obj));
        let log_file_bm = PathBuf::from(self.home_dir.clone()).join(format!("{}.dat", device_obj));
        self.n_devices += 1;
        let device_mtx = Arc::new(Mutex::new(device_obj));
        let device_mtx_thread_local = device_mtx.clone();
        let device_handler_emitter = Arc::clone(&handler_emitter);
        self.devices.push(device_mtx.clone());
        let h = thread::Builder::new()
            .name(format!("{}", device_name).to_string())
            .spawn(move || {
                let spin_sleeper = spin_sleep::SpinSleeper::new(1000);
                // read_time, valid message flag, word
                let mut prev_word = (0, false, WRD_EMPTY);
                // lock the device object - release only after thread shutdown:
                let mut device = device_mtx_thread_local.lock().unwrap();
                // warmup offset
                let mut bc_step = 0;

                loop {
                    if !go.load(Ordering::Relaxed) || device.state == State::Off {
                        spin_sleeper.sleep_ns(1_000_000);
                    }
                    if device.state != State::Off {
                        let mut current = device.clock.elapsed().as_nanos();
                        if device.mode == Mode::BC {
                            let mut timeout = device.timeout;
                            // 10 timeout for warming up
                            if bc_step <= BC_WARMUP_STEPS {
                                // if it is for warming up, we add additional margin for timeout
                                // but we can't just skip since certain operation fails forever
                                // such as BC2RT where RT is a BM
                                timeout += 20_000_000;
                            }
                            if device.state == State::Idle {
                                device.log(WRD_EMPTY, ErrMsg::MsgBCReady);
                                let mut local_emitter = device_handler_emitter.lock().unwrap();
                                device.timeout = 0;
                                local_emitter.handler.on_bc_ready(&mut device);
                                bc_step += 1;
                            } else if timeout > 0 && current > timeout {
                                device.timeout_times += 1;
                                let mut local_emitter = device_handler_emitter.lock().unwrap();
                                local_emitter.handler.on_bc_timeout(&mut device);
                                device.reset_all_stateful();
                                device.timeout = 0;
                                spin_sleeper.sleep_ns(100_0000);
                            }
                        }

                        // write is `asynchrnoized` and sequential
                        if current > device.time_write_ready {
                            if let Some(entry) = device.write_queue.pop_front() {
                                let wq = device.write_queue.len();
                                spin_sleeper.sleep_ns(device.write_delays as u64);
                                device.log(entry.1, ErrMsg::MsgWrt(wq));
                                for (i, s) in device.transmitters.iter().enumerate() {
                                    if (i as u32) != device.id {
                                        // let _e = s.try_send(entry.1);
                                        // let _e = s.send(entry.1);
                                        let _e =
                                            s.send_timeout(entry.1, Duration::from_millis(100));
                                        if _e.is_err() {
                                            break;
                                        }
                                    }
                                }
                                current = device.clock.elapsed().as_nanos();
                                device.time_write_ready = current + RT_WORD_LOAD_TIME;
                            }
                        }
                        // update current after potential blocking operation
                        let res = device.read();
                        current = device.clock.elapsed().as_nanos();
                        let diff =
                            (current as i128) - (prev_word.0 as i128) - (RT_WORD_LOAD_TIME as i128);
                        if prev_word.1 && diff > 0 {
                            // message in the cache is valid & after word_time . processe the word.
                            let mut w = prev_word.2;
                            let mut local_emitter = device_handler_emitter.lock().unwrap();
                            let new_atk_type = local_emitter.handler.get_attk_type();
                            if new_atk_type != device.atk_type {
                                // new handler
                                device.reset_all_stateful();
                                device.atk_type = new_atk_type;
                            }

                            if device.mode == Mode::BM {
                                device.log(w, ErrMsg::MsgBMLog);
                            } else {
                                if w.sync() == 1 {
                                    if w.instrumentation_bit() == 1 {
                                        local_emitter.handler.on_cmd(&mut device, &mut w)
                                    } else {
                                        // status word
                                        local_emitter.handler.on_sts(&mut device, &mut w);
                                    }
                                } else {
                                    // data word
                                    local_emitter.handler.on_dat(&mut device, &mut w);
                                }
                            }

                            // clear cache
                            prev_word = (0, false, WRD_EMPTY);
                        }
                        if !res.is_err() {
                            // update current after blocking
                            if prev_word.0 == 0 {
                                // empty cache, do replacement
                                prev_word = (current, true, res.unwrap());
                            } else {
                                // collision
                                if diff < 0 {
                                    let mut w = res.unwrap();
                                    // if w.address() == device.address {
                                    let mut local_emitter = device_handler_emitter.lock().unwrap();
                                    let new_atk_type = local_emitter.handler.get_attk_type();
                                    if new_atk_type != device.atk_type {
                                        // new handler
                                        device.reset_all_stateful();
                                        device.atk_type = new_atk_type;
                                    }
                                    if prev_word.1 {
                                        // if previous word is a valid message then file parity error
                                        // if not, the error was already filed.
                                        // log previous word recieve time
                                        // if device.state != State::Idle {
                                        local_emitter.handler.on_err_parity(
                                            &mut device,
                                            &mut prev_word.2,
                                            prev_word.0 as i128,
                                            diff,
                                        );
                                        // log current word recieve time
                                        local_emitter.handler.on_err_parity(
                                            &mut device,
                                            &mut w,
                                            current as i128,
                                            diff,
                                        );
                                        // log the previous word (corrupted)
                                        if device.mode == Mode::BM {
                                            w.set_parity_bit(1);
                                            device.log(w, ErrMsg::MsgBMLog);
                                        }
                                        // }
                                    }
                                    // }
                                    device.reset_all_stateful();
                                    prev_word = (0, false, WRD_EMPTY);
                                }
                            }
                        }
                    }
                    if exit.load(Ordering::Relaxed) {
                        //exiting
                        if CONFIG_SAVE_DEVICE_LOGS {
                            println!(
                                "{} writing {} logs to {} ",
                                device,
                                device.logs.len(),
                                log_file.to_str().unwrap()
                            );
                            let mut file = OpenOptions::new()
                                .write(true)
                                .append(true)
                                .create(true)
                                .open(log_file)
                                .unwrap();
                            let device_des = device.to_string();
                            for l in &device.logs {
                                writeln!(file, "{}", format_log(&l)).unwrap();
                            }
                            println!("{} Done flushing logs", device_des);
                        }
                        // for bus monitor
                        if CONFIG_SAVE_SYS_LOGS && device.mode == Mode::BM {
                            println!(
                                "{} writing {} logs to {} ",
                                device,
                                device.logs.len(),
                                log_file_bm.to_str().unwrap()
                            );
                            let mut file = OpenOptions::new()
                                .write(true)
                                .append(true)
                                .create(true)
                                .open(log_file_bm)
                                .unwrap();
                            let device_des = device.to_string();
                            for l in &device.logs {
                                writeln!(file, "{}", format_log_bm(&l)).unwrap();
                            }
                            println!("{} Done flushing logs", device_des);
                        }
                        break;
                    }
                }
                return 0;
            })
            .expect("failed to spawn thread");
        if let Some(handlers) = &mut self.handlers {
            handlers.push(h);
        } else {
            panic!("tried to push but no threads exist");
        }
    }
}

#[allow(unused)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Proto {
    RT2RT = 0,
    BC2RT = 1,
    RT2BC = 2,
}

#[derive(Clone, Debug)]
pub struct DefaultBCEventHandler {
    // val: u8,
    // path: String,
    // data: Vec<u32>
    pub total_device: u8,
    pub target: u8,
    pub data: Vec<u32>,
    pub proto: Proto,
    pub proto_rotate: bool,
}

impl EventHandler for DefaultBCEventHandler {
    fn on_bc_ready(&mut self, d: &mut Device) {
        self.target = self.target % (self.total_device - 1) + 1;
        let another_target = self.target % (self.total_device - 1) + 1;
        //
        // d.act_rt2bc(self.target, self.data.len() as u8);
        // a simple rotating scheduler
        // println!("{:?}", self.proto);
        match self.proto {
            Proto::RT2RT => {
                d.act_rt2rt(self.target, another_target, self.data.len() as u8);
                if self.proto_rotate {
                    self.proto = Proto::BC2RT;
                }
            }
            Proto::BC2RT => {
                d.act_bc2rt(self.target, &self.data);
                if self.proto_rotate {
                    self.proto = Proto::RT2BC;
                }
            }
            Proto::RT2BC => {
                d.act_rt2bc(self.target, self.data.len() as u8);
                if self.proto_rotate {
                    self.proto = Proto::RT2RT;
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct DefaultEventHandler {}

impl EventHandler for DefaultEventHandler {}

pub struct EventHandlerEmitter {
    pub handler: Box<dyn EventHandler>,
}

impl EventHandlerEmitter {}

#[allow(unused)]
#[derive(Clone, Debug, Copy, PartialEq)]
pub enum AttackType {
    Benign = 0,
    AtkCollisionAttackAgainstTheBus = 1,
    AtkCollisionAttackAgainstAnRT = 2,
    AtkDataThrashingAgainstRT = 3,
    AtkMITMAttackOnRTs = 4,
    AtkShutdownAttackRT = 5,
    AtkFakeStatusReccmd = 6,
    AtkFakeStatusTrcmd = 7,
    AtkDesynchronizationAttackOnRT = 8,
    AtkDataCorruptionAttack = 9,
    AtkCommandInvalidationAttack = 10,
}

impl From<i32> for AttackType {
    fn from(value: i32) -> Self {
        use AttackType::*;
        match value {
            0 => Benign,
            1 => AtkCollisionAttackAgainstTheBus,
            2 => AtkCollisionAttackAgainstAnRT,
            3 => AtkDataThrashingAgainstRT,
            4 => AtkMITMAttackOnRTs,
            5 => AtkShutdownAttackRT,
            6 => AtkFakeStatusReccmd,
            7 => AtkFakeStatusTrcmd,
            8 => AtkDesynchronizationAttackOnRT,
            9 => AtkDataCorruptionAttack,
            10 => AtkCommandInvalidationAttack,
            _ => Benign,
        }
    }
}

pub fn eval_sys(w_delays: u128, n_devices: u8, proto: Proto, proto_rotate: bool) -> System {
    // let n_devices = 3;
    // let w_delays = w_delays;
    let mut sys_bus = System::new(n_devices as u32, w_delays);
    for m in 0..n_devices {
        // let (s1, r1) = bounded(64);
        // s_vec.lock().unwrap().push(s1);
        if m == 0 {
            sys_bus.run_d(
                m as u8,
                Mode::BC,
                Arc::new(Mutex::new(EventHandlerEmitter {
                    handler: Box::new(DefaultBCEventHandler {
                        total_device: n_devices,
                        target: 0,
                        data: vec![1, 2, 3],
                        proto: proto,
                        proto_rotate: proto_rotate,
                    }),
                })),
                false,
            );
        } else {
            sys_bus.run_d(
                m as u8,
                Mode::RT,
                Arc::new(Mutex::new(EventHandlerEmitter {
                    handler: Box::new(DefaultEventHandler {}),
                })),
                false,
            );
        }
    }
    sys_bus.go();
    sys_bus.sleep_ms(200);
    sys_bus.stop();
    sys_bus.join();
    return sys_bus;
}

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn test_delta_t() {
        let system = eval_sys(40000, 3, Proto::RT2RT, true);
        let bc_mx = system.devices[0].clone();
        let bc = bc_mx.lock().unwrap();
        // smoke test
        println!("{}", bc.delta_t_avg / bc.delta_t_count);
        assert!(bc.delta_t_count > 0);
        assert!(bc.delta_t_avg / bc.delta_t_count > 0);
        assert!(bc.logs.len() > 1000);
    }
    #[test]
    fn test_timeout() {
        // a system consists of only a BC and a BM.
        // RT2BC for this BM address should be not-responsing so there will be timeouts
        // on BC.
        let n_devices = 2;
        let w_delays = 0;
        let mut sys_bus = System::new(n_devices as u32, w_delays);
        for m in 0..n_devices {
            // let (s1, r1) = bounded(64);
            // s_vec.lock().unwrap().push(s1);
            if m == 0 {
                sys_bus.run_d(
                    m as u8,
                    Mode::BC,
                    Arc::new(Mutex::new(EventHandlerEmitter {
                        handler: Box::new(DefaultBCEventHandler {
                            total_device: n_devices,
                            target: 0,
                            data: vec![1, 2, 3],
                            proto: Proto::RT2BC,
                            proto_rotate: false,
                        }),
                    })),
                    false,
                );
            } else {
                sys_bus.run_d(
                    m as u8,
                    Mode::BM,
                    Arc::new(Mutex::new(EventHandlerEmitter {
                        handler: Box::new(DefaultEventHandler {}),
                    })),
                    false,
                );
            }
        }
        sys_bus.go();
        sys_bus.sleep_ms(3000);
        sys_bus.stop();
        sys_bus.join();
        assert!(sys_bus.devices[0].lock().unwrap().timeout_times > 2);
    }
}
