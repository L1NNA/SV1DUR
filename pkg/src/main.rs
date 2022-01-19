use bitfield::bitfield;
use chrono::{Datelike, Timelike, Utc};
use crossbeam_channel::{bounded, unbounded, Receiver, Sender, TryRecvError};
use spin_sleep;
use std::fmt;
use std::fs::{create_dir, read_dir, File, OpenOptions};
use std::io::prelude::*;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};
const WRD_EMPTY: Word = Word { 0: 0 };
const CONFIG_PRINT_LOGS: bool = false;

#[derive(Copy, Clone, Debug, PartialEq)]
enum ErrMsg {
    MsgEmpt,
    MsgStaChg,
    MsgEntWrdRec,
    MsgEntErrPty,
    MsgEntCmd,
    MsgEntCmdRcv,
    MsgEntCmdTrx,
    MsgEntCmdMcx,
    MsgEntDat,
    MsgEntSte,
}

impl ErrMsg {
    fn value(&self) -> &'static str {
        match *self {
            ErrMsg::MsgEmpt => "",
            ErrMsg::MsgStaChg => "Status Changed",
            ErrMsg::MsgEntWrdRec => "Word Received",
            ErrMsg::MsgEntErrPty => "Parity Error",
            ErrMsg::MsgEntCmd => "CMD Received",
            ErrMsg::MsgEntCmdRcv => "CMD RCV Received",
            ErrMsg::MsgEntCmdTrx => "CMD TRX Received",
            ErrMsg::MsgEntCmdMcx => "CMD MCX Received",
            ErrMsg::MsgEntDat => "Data Received",
            ErrMsg::MsgEntSte => "Status Received",
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
    pub all,_ : 0, 20;
    u16;
    pub data, set_data: 19, 3;
}

impl fmt::Display for Word {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "w:{:#021b}", self.0)
    }
}

impl Word {
    fn new_status(src_addr: u8) -> Word {
        let mut w = Word { 0: 0 };
        w.set_sync(1);
        w.set_address(src_addr);
        return w;
    }

    fn new_data(val: u16) -> Word {
        let mut w = Word { 0: 0 };
        w.set_data(val);
        return w;
    }

    fn new_cmd(addr: u8, dword_count: u8, tr: u8) -> Word {
        let mut w = Word { 0: 0 };
        w.set_sync(1);
        w.set_tr(tr);
        w.set_address(addr);
        w.set_dword_count(dword_count);
        w.set_mode(2);
        w.set_instrumentation_bit(1);
        return w;
    }
}

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

trait EventHandler: Clone + Send {
    fn on_wrd_rec(&mut self, d: &mut Device, w: &mut Word);
    fn on_err_parity(&mut self, d: &mut Device, w: &mut Word);
    fn on_cmd(&mut self, d: &mut Device, w: &mut Word);
    fn on_cmd_rcv(&mut self, d: &mut Device, w: &mut Word);
    fn on_cmd_trx(&mut self, d: &mut Device, w: &mut Word);
    fn on_cmd_mcx(&mut self, d: &mut Device, w: &mut Word);
    fn on_dat(&mut self, d: &mut Device, w: &mut Word);
    fn on_sts(&mut self, d: &mut Device, w: &mut Word);
}

trait Scheduler: Clone + Send {
    fn on_bc_ready(&mut self, d: &mut Device);
}

struct Router<K: Scheduler, V: EventHandler> {
    scheduler: K,
    handler: V,
}

#[derive(Clone, Debug)]
struct Device {
    pub fake: bool,
    pub ccmd: u8,
    pub mode: Mode,
    pub state: State,
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
    pub receiver: Receiver<Word>,
    pub delta_t_avg: u128,
    pub delta_t_start: u128,
    pub delta_t_count: u128,
}

impl Device {
    fn write(&self, val: Word) {
        // println!("writing {} {}", val, val.sync());
        for (i, s) in self.transmitters.iter().enumerate() {
            if (i as u32) != self.id {
                s.try_send(val);
                // s.send(val);
            }
        }
    }

    fn read(&self) -> Result<Word, TryRecvError> {
        // return self.receiver.recv().unwrap();
        return self.receiver.try_recv();
    }

    fn reset_all_stateful(&mut self) {
        self.set_state(State::Idle);
        self.number_of_current_cmd = 0;
        self.delta_t_start = 0;
        self.memory.clear();
        self.dword_count = 0;
        self.dword_count_expected = 0;
        self.in_brdcst = false;
    }

    fn log(&mut self, word: Word, e: ErrMsg) {
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
            println!(
                "{} {}{:02}-{:02} {:^15} {} {} d_t:{}",
                l.0,
                l.1,
                l.2,
                l.3,
                l.4.to_string(),
                l.5,
                l.6.value(),
                l.7,
            );
        }
        self.logs.push(l);
    }

    fn set_state(&mut self, state: State) {
        self.state = state;
        self.log(WRD_EMPTY, ErrMsg::MsgStaChg);
    }

    fn act_bc2rt(&mut self, dest: u8, data: &Vec<u16>) {
        self.set_state(State::BusyTrx);
        self.write(Word::new_cmd(dest, data.len() as u8, 0));
        for d in data {
            self.write(Word::new_data(*d));
        }
        self.set_state(State::AwtStsRcvB2R);
        self.delta_t_start = self.clock.elapsed().as_nanos();
    }
    fn act_rt2bc(&mut self, src: u8, dword_count: u8) {
        self.set_state(State::BusyTrx);
        self.write(Word::new_cmd(src, dword_count, 1));
        // expecting to recieve dword_count number of words
        self.dword_count_expected = dword_count;
        self.set_state(State::AwtStsTrxR2B);
        self.delta_t_start = self.clock.elapsed().as_nanos();
    }
    fn act_rt2rt(&mut self, src: u8, dst: u8, dword_count: u8) {
        self.set_state(State::BusyTrx);
        let mut cmd = Word::new_cmd(src, dword_count, 1);
        cmd.set_sub_address(dst);
        self.write(cmd);
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

#[derive(Clone, Debug)]
struct DefaultEventHandler {}

impl EventHandler for DefaultEventHandler {
    fn on_wrd_rec(&mut self, d: &mut Device, w: &mut Word) {
        // for bm to monitor every word
        // d.log(*w, ErrMsg::MsgEntWrdRec);
    }
    fn on_err_parity(&mut self, d: &mut Device, w: &mut Word) {
        // log error tba
        // d.log(*w, ErrMsg::MsgEntErrPty);
    }
    fn on_cmd(&mut self, d: &mut Device, w: &mut Word) {
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
    fn on_cmd_trx(&mut self, d: &mut Device, w: &mut Word) {
        // may be triggered after cmd
        d.log(*w, ErrMsg::MsgEntCmdTrx);
        if !d.fake {
            d.set_state(State::BusyTrx);
            d.write(Word::new_status(d.address));
            for i in 0..w.dword_count() {
                d.write(Word::new_data((i + 1) as u16));
            }
        }
        d.reset_all_stateful();
    }
    fn on_cmd_rcv(&mut self, d: &mut Device, w: &mut Word) {
        d.log(*w, ErrMsg::MsgEntCmdRcv);
        // may be triggered after cmd
        d.set_state(State::AwtData);
        d.dword_count = 0;
        d.dword_count_expected = w.dword_count();
        if w.address() == 31 {
            d.in_brdcst = true;
        }
    }
    fn on_cmd_mcx(&mut self, d: &mut Device, w: &mut Word) {
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
    fn on_dat(&mut self, d: &mut Device, w: &mut Word) {
        if d.state == State::AwtData {
            d.log(*w, ErrMsg::MsgEntDat);
            if !d.fake {
                if d.ccmd == 1 {
                    // TBA:  synchronize clock to data
                    // (clock is u128 but data is not u16..)
                    // maybe set the microscecond component of the clock
                    d.ccmd = 0;
                } else {
                    if d.dword_count < d.dword_count_expected {
                        d.memory.push(w.data() as u16);
                    }
                    d.dword_count += 1;
                    if d.dword_count == d.dword_count_expected {
                        d.set_state(State::BusyTrx);
                        if d.mode != Mode::BC {
                            // only RT will responding status message
                            d.write(Word::new_status(d.address));
                        }
                        d.reset_all_stateful();
                    }
                }
            }
        }
    }
    fn on_sts(&mut self, d: &mut Device, w: &mut Word) {
        if d.mode == Mode::BC {
            d.log(*w, ErrMsg::MsgEntSte);
            // check delta_t
            if d.delta_t_start != 0 {
                let delta_t = d.clock.elapsed().as_nanos() - d.delta_t_start;
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
struct System {
    n_devices: u32,
    max_devices: u32,
    transmitters: Vec<Sender<Word>>,
    receivers: Vec<Receiver<Word>>,
    clock: Instant,
    go: Arc<AtomicBool>,
    exit: Arc<AtomicBool>,
    handlers: Vec<thread::JoinHandle<u32>>,
    home_dir: String,
}

impl System {
    fn new(max_devices: u32) -> Self {
        let clock = Instant::now();
        let home_dir = Utc::now().format("%F-%H-%M-%S").to_string();

        // i don't understand... why I have to clone..
        create_dir(PathBuf::from(home_dir.clone()));

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
        };
        for _ in 0..sys.max_devices {
            let (s1, r1) = bounded(512);
            // let (s1, r1) = unbounded();
            sys.transmitters.push(s1);
            sys.receivers.push(r1);
        }
        return sys;
    }

    fn go(&mut self) {
        self.go.store(true, Ordering::Relaxed);
    }

    fn pause(&mut self) {
        self.go.store(false, Ordering::Relaxed);
    }
    fn stop(&mut self) {
        self.exit.store(true, Ordering::Relaxed);
    }
    fn join(self) {
        for h in self.handlers {
            h.join().unwrap();
        }
        let mut lines = Vec::new();
        println!("Merging logs...");
        for path in read_dir(self.home_dir.clone()).unwrap() {
            let f = File::open(path.unwrap().path()).expect("Unable to open file");
            let br = BufReader::new(f);
            for line in br.lines() {
                let ln: String = line.unwrap();
                let split: Vec<&str> = ln.split(' ').collect();
                lines.push((split[0].parse::<u128>().unwrap(), ln));
            }
        }
        lines.sort_by_key(|k| k.0);
        let log_file = PathBuf::from(self.home_dir.clone()).join("sys.log");
        let mut file = OpenOptions::new()
            .write(true)
            .append(true)
            .create(true)
            .open(log_file)
            .unwrap();
        for l in lines {
            writeln!(file, "{}", l.1);
        }
    }
    fn sleep_ms(&mut self, ms: u64) {
        thread::sleep(Duration::from_millis(ms));
    }
    fn run_d<K: Scheduler + 'static, V: EventHandler + 'static>(
        &mut self,
        addr: u8,
        mode: Mode,
        router: Router<K, V>,
    ) {
        let transmitters = self.transmitters.clone();
        let receiver = self.receivers[self.n_devices as usize].clone();
        let mut device = Device {
            fake: false,
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
            receiver: receiver,
            delta_t_avg: 0,
            delta_t_count: 0,
            delta_t_start: 0,
        };
        let go = Arc::clone(&self.go);
        let exit = Arc::clone(&self.exit);
        let log_file = PathBuf::from(self.home_dir.clone()).join(format!("{}.log", device));
        self.n_devices += 1;

        let h = thread::spawn(move || {
            let spin_sleeper = spin_sleep::SpinSleeper::new(1000);
            let mut handler = router.handler;
            let mut scheduler = router.scheduler;

            loop {
                if !go.load(Ordering::Relaxed) || device.state == State::Off {
                    spin_sleeper.sleep_ns(1000_000);
                }
                {
                    if device.mode == Mode::BC && device.state == State::Idle {
                        scheduler.on_bc_ready(&mut device);
                    }
                    // if device.mode == Mode::BC{
                    //     println!("here")
                    // }
                    let res = device.read();
                    if !res.is_err() {
                        let mut w = res.unwrap();
                        handler.on_wrd_rec(&mut device, &mut w);
                        // synchronizatoin bit distinguishes data/(command/status) word
                        if w.sync() == 1 {
                            // device.log(w, ErrMsg::MsgEntCmd);
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
                    }
                }
                if exit.load(Ordering::Relaxed) {
                    //exiting
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
                    for l in device.logs {
                        writeln!(
                            file,
                            "{} {}{:02}-{:02} {:^15} {} {} d_t:{}",
                            l.0,
                            l.1,
                            l.2,
                            l.3,
                            l.4.to_string(),
                            l.5,
                            l.6.value(),
                            l.7
                        )
                        .unwrap();
                    }
                    println!("{} Done flushing logs", device_des);
                    break;
                }
            }
            return 0;
        });
        self.handlers.push(h);
    }
}

#[derive(Clone, Debug)]
struct DefaultScheduler {
    // val: u8,
    // path: String,
    // data: Vec<u32>
    total_device: u8,
    target: u8,
    data: Vec<u16>,
}

impl Scheduler for DefaultScheduler {
    fn on_bc_ready(&mut self, d: &mut Device) {
        self.target = self.target % (self.total_device - 1) + 1;
        let another_target = self.target % (self.total_device - 1) + 1;
        // d.act_bc2rt(self.target, &self.data);
        // d.act_rt2bc(self.target, self.data.len() as u8);
        d.act_rt2rt(self.target, another_target, self.data.len() as u8)
    }
}

fn test1() {
    // let mut delays_single = Vec::new();
    let n_devices = 8;
    let mut sys = System::new(n_devices as u32);
    for m in 0..n_devices {
        // let (s1, r1) = bounded(64);
        // s_vec.lock().unwrap().push(s1);
        let router = Router {
            // control all communications
            scheduler: DefaultScheduler {
                total_device: n_devices,
                target: 0,
                data: vec![1, 2, 3],
            },
            // control device-level response
            handler: DefaultEventHandler {},
        };
        if m == 0 {
            sys.run_d(m as u8, Mode::BC, router);
        } else {
            sys.run_d(m as u8, Mode::RT, router);
        }
    }
    sys.go();
    sys.sleep_ms(10);
    sys.stop();
    sys.join();
    // let mut delays = Vec::new();
    // loop {
    //     let (w3, index): (Result<Word, RecvError>, usize) = recv_multiple2(&r_vec);
    //     // println!("{} boardcast rcv", clock.elapsed().as_micros());
    //     // for s in &s_vec {
    //     let mut c = clock.elapsed().as_nanos();
    //     for (i, s) in s_vec.iter().enumerate() {
    //         if i != index {
    //             s.try_send(w3.unwrap());
    //         }
    //     }
    //     // println!("{} boardcast snt", clock.elapsed().as_micros());
    //     c = clock.elapsed().as_nanos() - c;

    //     // delays.push(c as u64);
    //     // if delays.len() % 100000 == 0 {
    //     //     println!("avg sent-delays per 10000 boardcast {}", average(&delays),);
    //     //     delays.clear();
    //     // }
    // }

    // Send a message and then receive one.
    // loop {
    //     // c = clock.elapsed().as_nanos();
    //     // println!("{} ", clock.elapsed().as_nanos());
    //     // let mut w = Word { 0: 0 };
    //     // w.set_data(k + 1);
    //     // s.send(w).unwrap();
    //     // w = r.recv().unwrap();
    //     // k = w.data();
    //     // c = clock.elapsed().as_nanos() - c;
    //     // delays.push(c as u64);
    //     // if k % 10000 == 0 {
    //     //     println!(
    //     //         "done. avg round-delays per 10000 rounds {}",
    //     //         average(&delays)
    //     //     );
    //     //     delays.clear();
    //     // }
    //     let w: Word = recv_multiple(&r_vec).unwrap();
    //     for s in &s_vec {
    //         s.send(w).unwrap();
    //     }
    // }
}

fn main() {
    test1();
}
