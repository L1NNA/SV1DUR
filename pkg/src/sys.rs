use bitfield::bitfield;
use chrono::Utc;
use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError};
use spin_sleep;
use std::fmt;
use std::fs::{create_dir, read_dir, File, OpenOptions};
use std::io::prelude::*;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
pub const WRD_EMPTY: Word = Word { 0: 0 };
pub const CONFIG_PRINT_LOGS: bool = false;
pub const CONFIG_SAVE_DEVICE_LOGS: bool = true;
pub const CONFIG_SAVE_SYS_LOGS: bool = true;

#[allow(unused)]
#[derive(Clone, Debug, PartialEq)]
pub enum ErrMsg {
    MsgEmpt,
    MsgWrt,
    MsgBCReady,
    MsgStaChg,
    MsgEntWrdRec,
    MsgEntErrPty,
    MsgEntCmd,
    MsgEntCmdRcv,
    MsgEntCmdTrx,
    MsgEntCmdMcx,
    MsgEntDat,
    MsgEntSte,
    MsgAttk(String),
}

impl ErrMsg {
    fn value(&self) -> String {
        match self {
            ErrMsg::MsgEmpt => "".to_owned(),
            ErrMsg::MsgWrt => "Wrt".to_owned(),
            ErrMsg::MsgBCReady => "BC is ready".to_owned(),
            ErrMsg::MsgStaChg => "Status Changed".to_owned(),
            ErrMsg::MsgEntWrdRec => "Word Received".to_owned(),
            ErrMsg::MsgEntErrPty => "Parity Error".to_owned(),
            ErrMsg::MsgEntCmd => "CMD Received".to_owned(),
            ErrMsg::MsgEntCmdRcv => "CMD RCV Received".to_owned(),
            ErrMsg::MsgEntCmdTrx => "CMD TRX Received".to_owned(),
            ErrMsg::MsgEntCmdMcx => "CMD MCX Received".to_owned(),
            ErrMsg::MsgEntDat => "Data Received".to_owned(),
            ErrMsg::MsgEntSte => "Status Received".to_owned(),
            ErrMsg::MsgAttk(msg) => {
                return msg.to_owned();
            }
        }
    }
}

fn format_log(l: &(u128, Mode, u32, u8, State, Word, ErrMsg, u128)) -> String {
    return format!(
        "{} {}{:02}-{:02} {:^15} {} {:^16} avg_d_t:{}",
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
    pub tr, set_tr: 8, 8;
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
        write!(f, "w:{:#027b}[{}]", self.0, self.attk()) // We need an extra 2 bits for '0b' on top of the number of bits we're printing
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

    pub fn new_cmd(addr: u8, dword_count: u8, tr: u8) -> Word {
        let mut w = Word { 0: 0 };
        w.set_sync(1);
        w.set_tr(tr); // 1: transmit, 0: receive
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
    AwtStsRcvB2R,
    // rt2bc - bc waiting for the transmitter status code
    AwtStsTrxR2B,
    // rt2rt - bc waiting for reciever status code
    AwtStsRcvR2R,
    // rt2rt - bc waiting for the transmitter status code
    AwtStsTrxR2R,
}
impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub trait EventHandler: Clone + Send {
    fn on_wrd_rec(&mut self, d: &mut Device, w: &mut Word) {
        self.default_on_wrd_rec(d, w);
    }
    fn on_err_parity(&mut self, d: &mut Device, w: &mut Word) {
        self.default_on_err_parity(d, w);
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

    #[allow(unused)]
    fn default_on_wrd_rec(&mut self, d: &mut Device, w: &mut Word) {
        // for bm to monitor every word
        // d.log(*w, ErrMsg::MsgEntWrdRec);
    }

    #[allow(unused)]
    fn default_on_err_parity(&mut self, d: &mut Device, w: &mut Word) {
        // log error tba
        d.log(*w, ErrMsg::MsgEntErrPty);
    }
    fn default_on_cmd(&mut self, d: &mut Device, w: &mut Word) {
        // cmds are only for RT, matching self's address
        if d.mode == Mode::RT {
            let destination = w.address();
            // 31 is the boardcast address
            if destination == d.address || destination == 31 {
                // d.log(*w, ErrMsg::MsgEntCmd);
                // println!("{} {} {}", w, w.tr(), w.mode());
                d.number_of_current_cmd += 1;
                // if there was previously a command word recieved
                // cancel previous command (clear state)
                if d.number_of_current_cmd >= 2 {
                    d.reset_all_stateful();
                }
                if w.tr() == 0 && (w.mode() == 1 || w.mode() == 0) {
                    // shutdown etc mode change command
                    self.on_cmd_mcx(d, w);
                } else {
                    if w.tr() == 0 {
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
            if w.tr() == 1 && w.sub_address() == d.address {
                self.on_cmd_rcv(d, w);
            }
        }
    }
    fn default_on_cmd_trx(&mut self, d: &mut Device, w: &mut Word) {
        // may be triggered after cmd
        d.log(*w, ErrMsg::MsgEntCmdTrx);
        if !d.fake {
            d.set_state(State::BusyTrx);
            d.write(Word::new_status(d.address));
            for i in 0..w.dword_count() {
                d.write(Word::new_data((i + 1) as u32));
            }
        }
        d.reset_all_stateful();
    }
    fn default_on_cmd_rcv(&mut self, d: &mut Device, w: &mut Word) {
        d.log(*w, ErrMsg::MsgEntCmdRcv);
        // may be triggered after cmd
        d.set_state(State::AwtData);
        d.dword_count = 0;
        d.dword_count_expected = w.dword_count();
        if w.address() == 31 {
            d.in_brdcst = true;
        }
    }
    fn default_on_cmd_mcx(&mut self, d: &mut Device, w: &mut Word) {
        if d.address == w.address() {
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
                        // clear cache
                        d.reset_all_stateful();
                        d.set_state(State::Off);
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
                    d.reset_all_stateful();
                }
            }
        }
    }
    fn default_on_sts(&mut self, d: &mut Device, w: &mut Word) {
        if d.mode == Mode::BC {
            d.log(*w, ErrMsg::MsgEntSte);
            // check delta_t
            if d.delta_t_start != 0 {
                let delta_t = d.clock.elapsed().as_nanos() - d.delta_t_start;
                // delta_t has to be in between 4 and 12
                d.delta_t_avg += delta_t;
                d.delta_t_count += 1;
            }
            match d.state {
                State::AwtStsTrxR2B => {
                    //(transmitter confirmation)
                    // rt2bc

                    d.set_state(State::AwtData)
                }
                State::AwtStsRcvB2R => {
                    // rt2rt (reciver confirmation)
                    // bc2rt
                    d.reset_all_stateful();
                }
                State::AwtStsTrxR2R => {
                    //(transmitter confirmation)
                    // rt2rt
                    d.set_state(State::AwtStsRcvR2R);
                    d.delta_t_start = d.clock.elapsed().as_nanos();
                }
                State::AwtStsRcvR2R => {
                    // rt2rt (reciver confirmation)
                    // rt2rt
                    d.reset_all_stateful();
                }
                _ => {}
            }
        }
    }
}

pub trait Scheduler: Clone + Send {
    fn on_bc_ready(&mut self, d: &mut Device) {}
}

#[derive(Clone, Debug)]
pub struct Router<K: Scheduler, V: EventHandler> {
    pub scheduler: K,
    pub handler: V,
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
    pub write_queue: Vec<(u128, Word)>,
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
            val.set_attk(self.atk_type as u32);
        }
        if self.write_queue.len() < 1 {
            self.write_queue
                .push((self.clock.elapsed().as_nanos() + self.write_delays, val));
        } else {
            // println!("here {} {} {:?}, {}", self, self.write_queue.len(), self.write_queue.last().unwrap().0, self.write_delays);
            self.write_queue
                .push((self.write_queue.last().unwrap().0 + self.write_delays, val));
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

    pub fn reset_all_stateful(&mut self) {
        self.set_state(State::Idle);
        self.number_of_current_cmd = 0;
        self.delta_t_start = 0;
        self.memory.clear();
        self.dword_count = 0;
        self.dword_count_expected = 0;
        self.in_brdcst = false;
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
        self.state = state;
        self.log(WRD_EMPTY, ErrMsg::MsgStaChg);
    }

    pub fn act_bc2rt(&mut self, dest: u8, data: &Vec<u32>) {
        self.set_state(State::BusyTrx);
        self.write(Word::new_cmd(dest, data.len() as u8, 0));
        for d in data {
            self.write(Word::new_data(*d));
        }
        self.set_state(State::AwtStsRcvB2R);
        self.delta_t_start = self.clock.elapsed().as_nanos();
    }
    pub fn act_rt2bc(&mut self, src: u8, dword_count: u8) {
        self.set_state(State::BusyTrx);
        self.write(Word::new_cmd(src, dword_count, 1));
        // expecting to recieve dword_count number of words
        self.dword_count_expected = dword_count;
        self.set_state(State::AwtStsTrxR2B);
        self.delta_t_start = self.clock.elapsed().as_nanos();
    }
    pub fn act_rt2rt(&mut self, src: u8, dst: u8, dword_count: u8) {
        self.set_state(State::BusyTrx);
        self.write(Word::new_cmd(dst, dword_count, 0));
        self.write(Word::new_cmd(src, dword_count, 1));
        // expecting to recieve dword_count number of words
        self.set_state(State::AwtStsTrxR2R);
        self.delta_t_start = self.clock.elapsed().as_nanos();
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
    pub handlers: Vec<thread::JoinHandle<u32>>,
    pub devices: Vec<Arc<Mutex<Device>>>,
    pub logs: Vec<(u128, Mode, u32, u8, State, Word, ErrMsg, u128)>,
    pub home_dir: String,
    pub write_delays: u128,
}

impl System {
    pub fn new(max_devices: u32, write_delays: u128) -> Self {
        let clock = Instant::now();
        let home_dir = Utc::now().format("%F-%H-%M-%S").to_string();

        // i don't understand... why I have to clone..
        let _ = create_dir(PathBuf::from(home_dir.clone()));

        let mut sys = System {
            n_devices: 0,
            max_devices: max_devices,
            transmitters: Vec::new(),
            receivers: Vec::new(),
            clock: clock,
            go: Arc::new(AtomicBool::new(false)),
            exit: Arc::new(AtomicBool::new(false)),
            handlers: Vec::new(),
            home_dir: home_dir,
            write_delays: write_delays,
            devices: Vec::new(),
            logs: Vec::new(),
        };
        for _ in 0..sys.max_devices {
            let (s1, r1) = bounded(512);
            // let (s1, r1) = unbounded();
            sys.transmitters.push(s1);
            sys.receivers.push(r1);
        }
        return sys;
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
    pub fn join(mut self) -> Vec<Arc<Mutex<Device>>> {
        for h in self.handlers {
            let _ = h.join();
        }
        println!("Merging logs...");
        for device_mx in &self.devices {
            let device = device_mx.lock().unwrap();
            device.log_merge(&mut self.logs);
        }

        self.logs.sort_by_key(|k| k.0);
        if CONFIG_SAVE_SYS_LOGS {
            let log_file = PathBuf::from(self.home_dir.clone()).join("sys.log");
            let mut file = OpenOptions::new()
                .write(true)
                .append(true)
                .create(true)
                .open(log_file)
                .unwrap();
            for l in self.logs {
                let _ = writeln!(file, "{}", format_log(&l));
            }
        }
        return self.devices.clone();
    }
    pub fn sleep_ms(&mut self, ms: u64) {
        thread::sleep(Duration::from_millis(ms));
    }
    pub fn run_d<K: Scheduler + 'static, V: EventHandler + 'static>(
        &mut self,
        addr: u8,
        mode: Mode,
        router: Router<K, V>,
        atk_type: AttackType,
    ) {
        let transmitters = self.transmitters.clone();
        let receiver = self.receivers[self.n_devices as usize].clone();
        let mut w_delay = self.write_delays;
        let mut fake = false;
        if atk_type != AttackType::Benign {
            fake = true;
        }
        if fake {
            w_delay = 0;
        }
        let mut device_obj = Device {
            fake: fake,
            atk_type: atk_type,
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
            write_queue: Vec::new(),
            read_queue: Vec::new(),
            receiver: receiver,
            delta_t_avg: 0,
            delta_t_count: 0,
            delta_t_start: 0,
            write_delays: w_delay,
        };
        let device_name = format!("{}", device_obj);
        let go = Arc::clone(&self.go);
        let exit = Arc::clone(&self.exit);
        let log_file = PathBuf::from(self.home_dir.clone()).join(format!("{}.log", device_obj));
        self.n_devices += 1;
        let device_mtx = Arc::new(Mutex::new(device_obj));
        let device_mtx_thread_local = device_mtx.clone();
        self.devices.push(device_mtx.clone());
        let h = thread::Builder::new()
            .name(format!("{}", device_name).to_string())
            .spawn(move || {
                let spin_sleeper = spin_sleep::SpinSleeper::new(1000);
                let mut handler = router.handler;
                let mut scheduler = router.scheduler;
                // read_time, valid message flag, word
                let mut prev_word = (0, false, WRD_EMPTY);
                // lock the device object - release only after thread shutdown:
                let mut device = device_mtx_thread_local.lock().unwrap();

                loop {
                    if !go.load(Ordering::Relaxed) || device.state == State::Off {
                        spin_sleeper.sleep_ns(1000_000);
                    }
                    {
                        if device.mode == Mode::BC && device.state == State::Idle {
                            device.log(WRD_EMPTY, ErrMsg::MsgBCReady);
                            scheduler.on_bc_ready(&mut device);
                        }
                        // if device.mode == Mode::BC{
                        //     println!("here")
                        // }

                        // write is `asynchrnoized`
                        let wq = device.write_queue.len();
                        let current = device.clock.elapsed().as_nanos();
                        if wq > 0 {
                            let mut w_logs = Vec::new();
                            for entry in device.write_queue.iter() {
                                // if now it is the time to actually write
                                if entry.0 <= current {
                                    for (i, s) in device.transmitters.iter().enumerate() {
                                        if (i as u32) != device.id {
                                            let _e = s.try_send(entry.1);
                                            // s.send(val);
                                        }
                                    }
                                    w_logs.push((entry.1, ErrMsg::MsgWrt));
                                }
                            }
                            for wl in w_logs {
                                device.log(wl.0, wl.1);
                            }
                            // clearing all the data (otherwise delta_t keeps increasing)
                            device.write_queue.retain(|x| (*x).0 > current);
                            // device.write_queue.clear();
                        }

                        let word_load_time = 0; //20_000; // the number of microseconds to transmit 1 word on the bus.  This will help us find collisions
                        let res = device.read();
                        if !res.is_err() {
                            if prev_word.0 == 0 {
                                // empty cache, do replacement
                                prev_word = (current, true, res.unwrap());
                            } else if current - prev_word.0 < word_load_time {
                                // collision
                                let mut w = res.unwrap();
                                if w.address() == device.address {
                                    if prev_word.1 {
                                        // if previous word is a valid message then file parity error
                                        // if not, the error was already filed.
                                        handler.on_err_parity(&mut device, &mut prev_word.2);
                                    }
                                    handler.on_err_parity(&mut device, &mut w);
                                }
                                // replaced with new timestamp, and invalid message flag (collided)
                                prev_word = (current, false, w);
                            }
                        }
                        if prev_word.1 && current >= prev_word.0 + word_load_time {
                            // message in the cache is valid & after word_time . processe the word.
                            let mut w = prev_word.2;
                            if w.sync() == 1 {
                                if w.instrumentation_bit() == 1 {
                                    handler.on_cmd(&mut device, &mut w)
                                } else {
                                    // status word
                                    handler.on_sts(&mut device, &mut w);
                                }
                            } else {
                                // data word
                                handler.on_dat(&mut device, &mut w);
                            }
                            // clear cache
                            prev_word = (0, false, WRD_EMPTY);
                        }
                        //     if !res.is_err() {
                        //         let current = device.clock.elapsed().as_nanos();
                        //         let mut w = res.unwrap();
                        //         let collision: bool;
                        //         let elements = device.read_queue.len();
                        //         if elements > 0
                        //             && device.read_queue[elements - 1].0 > current - word_time
                        //         {
                        //             // if the message was received before the previous message finished
                        //             collision = true; // we need to acknowledge that there was a collision.  Parity error should only happen 50% of the time, but we'll say it always happens.
                        //             device.read_queue[elements - 1].2 = true; // ensure the previous message is marked as a collision
                        //         } else {
                        //             collision = false;
                        //         }
                        //         device.read_queue.push((current, w, collision));
                        //         handler.on_wrd_rec(&mut device, &mut w);
                        //     }
                        //     let rq = device.read_queue.clone(); // clone to avoid mutable vs immutable borrows
                        //     if rq.len() > 0 {
                        //         let current = device.clock.elapsed().as_nanos();
                        //         for entry in rq.iter() {
                        //             if entry.0 <= current - word_time {
                        //                 // if the start time was 20us ago, we just finished receiving it
                        //                 let mut w = entry.1;
                        //                 if entry.2 {
                        //                     // if the next message already started transmitting, before the first one finished, we get a parity error and both messages fail to deliver
                        //                     handler.on_err_parity(&mut device, &mut w);
                        //                 } else {
                        //                     // Previous message was valid
                        //                     // synchronization bit distinguishes data/(command/status) word
                        //                     if w.sync() == 1 {
                        //                         // device.log(w, ErrMsg::MsgEntCmd);
                        //                         if w.instrumentation_bit() == 1 {
                        //                             handler.on_cmd(&mut device, &mut w)
                        //                         } else {
                        //                             // status word
                        //                             handler.on_sts(&mut device, &mut w);
                        //                         }
                        //                     } else {
                        //                         // data word
                        //                         handler.on_dat(&mut device, &mut w);
                        //                     }
                        //                 }
                        //                 device.read_queue.retain(|x| (*x).0 > current);
                        //             }
                        //         }
                        //     }
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
                        break;
                    }
                }
                return 0;
            })
            .expect("failed to spawn thread");
        self.handlers.push(h);
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
pub struct DefaultScheduler {
    // val: u8,
    // path: String,
    // data: Vec<u32>
    pub total_device: u8,
    pub target: u8,
    pub data: Vec<u32>,
    pub proto: Proto,
    pub proto_rotate: bool,
}

impl Scheduler for DefaultScheduler {
    fn on_bc_ready(&mut self, d: &mut Device) {
        self.target = self.target % (self.total_device - 1) + 1;
        let another_target = self.target % (self.total_device - 1) + 1;
        //
        // d.act_rt2bc(self.target, self.data.len() as u8);
        // a simple rotating scheduler
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
pub struct EmptyScheduler {}
impl Scheduler for EmptyScheduler {}

#[derive(Clone, Debug)]
pub struct DefaultEventHandler {}

impl EventHandler for DefaultEventHandler {}

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

#[cfg(test)]
mod tests {
    // Note this useful idiom: importing names from outer (for mod tests) scope.
    use super::*;

    #[test]
    fn test_delta_t() {
        // let mut delays_single = Vec::new();
        let n_devices = 8;
        let w_delays = 0;
        let mut sys = System::new(n_devices as u32, w_delays);
        for m in 0..n_devices {
            // let (s1, r1) = bounded(64);
            // s_vec.lock().unwrap().push(s1);
            let router = Router {
                // control all communications
                scheduler: DefaultScheduler {
                    total_device: n_devices,
                    target: 0,
                    data: vec![1, 2, 3],
                    proto: Proto::BC2RT,
                    proto_rotate: true,
                },
                // control device-level response
                handler: DefaultEventHandler {},
            };
            if m == 0 {
                sys.run_d(m as u8, Mode::BC, router, AttackType::Benign);
            } else {
                sys.run_d(m as u8, Mode::RT, router, AttackType::Benign);
            }
        }
        sys.go();
        sys.sleep_ms(100);
        sys.stop();
        let devices = sys.join();
        let bc_mx = devices[0].clone();
        let bc = bc_mx.lock().unwrap();
        println!("{}", bc.delta_t_avg / bc.delta_t_count);
        assert!(bc.delta_t_count > 0);
        assert!(bc.delta_t_avg / bc.delta_t_count > 0);
    }
}
