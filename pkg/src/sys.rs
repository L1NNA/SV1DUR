use crate::primitive_types::{ErrMsg, Word, Mode, State, AttackType, WRD_EMPTY,
CONFIG_SAVE_DEVICE_LOGS, CONFIG_SAVE_SYS_LOGS, ATK_DEFAULT_DELAYS};
use crate::event_handlers::{EventHandler, DefaultEventHandler};
use crate::devices::{Device, format_log};
use crate::schedulers::{DefaultScheduler, Scheduler, Proto};
use chrono::Utc;
use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError, select};
use spin_sleep;
#[allow(unused)]
use std::fs::{create_dir, read_dir, File, OpenOptions};
use std::io::prelude::*;
#[allow(unused)]
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use std::collections::LinkedList;

#[derive(Clone, Debug)]
pub struct Router<K: Scheduler, V: EventHandler> {
    pub scheduler: K,
    pub handler: V,
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
        let clock = Instant::now();
        let home_dir = Utc::now().format("%F-%H-%M-%S-%f").to_string();

        if CONFIG_SAVE_DEVICE_LOGS || CONFIG_SAVE_SYS_LOGS {
            let _ = create_dir(PathBuf::from(&home_dir));
        }

        let mut sys = System {
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
            for l in &self.logs {
                let _ = writeln!(file, "{}", format_log(&l));
            }
        }
    }
    pub fn sleep_ms(&mut self, ms: u64) {
        thread::sleep(Duration::from_millis(ms));
    }

    pub fn run_d<K: Scheduler + 'static, V: EventHandler + 'static>(
        &mut self,
        addr: u8,
        mode: Mode,
        router: Arc<Mutex<Router<K, V>>>,
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
            w_delay = ATK_DEFAULT_DELAYS;
        }
        let device_obj = Device {
            fake: fake,
            atk_type: atk_type,
            ccmd: 0,
            state: State::Idle,
            error_bit: false,
            service_request: false,
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
            write_queue: LinkedList::new(),
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
        let device_router = Arc::clone(&router);
        self.devices.push(device_mtx.clone());
        let h = thread::Builder::new()
            .name(format!("{}", device_name).to_string())
            .spawn(move || {
                let spin_sleeper = spin_sleep::SpinSleeper::new(1_000);
                let mut local_router = device_router.lock().unwrap();
                // read_time, valid message flag, word
                let mut prev_word = (0, false, WRD_EMPTY);
                // lock the device object - release only after thread shutdown:
                let mut device = device_mtx_thread_local.lock().unwrap();
                let mut time_bus_available = 0;
                loop {
                    if !go.load(Ordering::Relaxed) || device.state == State::Off {
                        spin_sleeper.sleep_ns(1000_000);
                    }
                    if device.state != State::Off {
                        if device.mode == Mode::BC && device.state == State::Idle {
                            device.log(WRD_EMPTY, ErrMsg::MsgBCReady);
                            local_router.scheduler.on_bc_ready(&mut device);
                        }
                        // if device.mode == Mode::BC{
                        //     println!("here")
                        // }

                        // write is `asynchrnoized`
                        let wq = device.write_queue.len();
                        let mut current: u128 = device.clock.elapsed().as_nanos();
                        if wq > 0 {
                            // let mut w_logs = Vec::new();
                            while device.write_queue.len() > 0 && device.write_queue.front().unwrap().0 <= current && time_bus_available <= current {
                                let entry = device.write_queue.pop_front().unwrap();
                                // log can be slower than write ...
                                device.log(entry.1, ErrMsg::MsgWrt(wq));
                                for (i, s) in device.transmitters.iter().enumerate() {
                                    if (i as u32) != device.id {
                                        let _e = s.try_send(entry.1);
                                        time_bus_available = current + w_delay;
                                        // s.send(val);
                                    }
                                }
                                // w_logs.push((entry.1, ErrMsg::MsgWrt));

                            }
                        }

                        let word_load_time = 20_000; // the number of microseconds to transmit 1 word on the bus.  This will help us find collisions
                        let mut res: Result<Word, TryRecvError>;
                        // if prev_word.0 == 0 {
                        //     res = device.maybe_block_read(); // Adding this line was marginally faster.  It may slow things down on a more capable computer.
                        // } else {                             // This did slow things down on a faster computer.  By a significant margin.  Likely from context switching.
                            res = device.read();
                        // }
                        if !res.is_err() {
                            if prev_word.0 == 0 {
                                // empty cache, do replacement
                                prev_word = (current, true, res.unwrap());
                            } else if current - prev_word.0 < word_load_time {
                                // collision
                                let mut w = res.unwrap();
                                if prev_word.1 {
                                    // if previous word is a valid message then file parity error
                                    // if not, the error was already filed.
                                    local_router
                                        .handler
                                        .on_err_parity(&mut device, &mut prev_word.2);
                                }
                                local_router.handler.on_err_parity(&mut device, &mut w);
                                // replaced with new timestamp, and invalid message flag (collided)
                                prev_word = (current, false, w);
                            }
                        }
                        if prev_word.1 && current >= prev_word.0 + word_load_time {
                            // message in the cache is valid & after word_time . processe the word.
                            let mut w = prev_word.2;
                            if w.sync() == 1 {
                                if w.instrumentation_bit() == 1 {
                                    local_router.handler.on_cmd(&mut device, &mut w)
                                } else {
                                    // status word
                                    local_router.handler.on_sts(&mut device, &mut w);
                                    if w.service_request_bit() != 0 {
                                        local_router.scheduler.request_sr(w.address());
                                    }
                                    if w.message_errorbit() != 0{
                                        local_router.scheduler.error_bit();
                                    }
                                }
                            } else {
                                // data word
                                local_router.handler.on_dat(&mut device, &mut w);
                            }
                            // clear cache
                            prev_word = (0, false, WRD_EMPTY);
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
pub fn eval_sys(w_delays: u128, n_devices: u8, proto: Proto, proto_rotate: bool) -> System {
    // let n_devices = 3;
    // let w_delays = w_delays;
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
                proto: proto,
                proto_rotate: proto_rotate,
            },
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
    sys.sleep_ms(100);
    sys.stop();
    sys.join();
    return sys;
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
        assert!(bc.logs.len() > 0);
    }
}
