use crate::primitive_types::{ErrMsg, Word, Mode, State, AttackType, WRD_EMPTY,
    CONFIG_SAVE_DEVICE_LOGS, CONFIG_SAVE_SYS_LOGS, ATK_DEFAULT_DELAYS,
    WORD_LOAD_TIME, COLLISION_TIME};
    use crate::event_handlers::{EventHandler, DefaultEventHandler, EventHandlerEmitter, DefaultBCEventHandler};
    use crate::devices::{Device, format_log, format_log_bm};
    use crate::schedulers::{DefaultScheduler, Scheduler, Proto};
    use chrono::Utc;
    use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError, select, Select, TryRecvError::Empty};
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
        pub transmitters: Vec<Sender<(u128, Word)>>,
        pub receivers: Vec<Receiver<(u128, Word)>>,
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
                let device = device_mx.lock();
                match device {
                    Ok(device) => if device.mode != Mode::BC {device.log_merge(&mut self.logs)},
                    Err(_) => println!("Error flushing log on device"),
                }
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
                let log_file = PathBuf::from(self.home_dir.clone()).join("sys.flight.log");
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
            let log_file_bm = PathBuf::from(self.home_dir.clone()).join(format!("{}.dat", device_obj));
            self.n_devices += 1;
            let device_mtx = Arc::new(Mutex::new(device_obj));
            let device_mtx_thread_local = device_mtx.clone();
            let device_handler_emitter = Arc::clone(&handler_emitter);
            self.devices.push(device_mtx.clone());
            let temp_mtx = device_mtx.clone();
            let mut temp_dev = temp_mtx.lock().unwrap();
            temp_dev.log(WRD_EMPTY, ErrMsg::MsgAttk("Thread starting".to_string()));
            let h = thread::Builder::new()
                .name(format!("{}", device_name).to_string())
                .spawn(move || {
                    let spin_sleeper = spin_sleep::SpinSleeper::new(1_000);
                    // let mut local_router = device_router.lock().unwrap();
                    // read_time, valid message flag, word
                    let mut prev_word = (0, false, WRD_EMPTY);
                    // lock the device object - release only after thread shutdown:
                    let mut device = device_mtx_thread_local.lock().unwrap();
                    let mut time_bus_available = 0;
                    let mut wq = device.write_queue.len();
                    loop {
                        if !go.load(Ordering::Relaxed) || device.state == State::Off {
                            spin_sleeper.sleep_ns(1000_000);
                        }
                        if device.state != State::Off {
                            if device.mode == Mode::BC {
                                if device.state == State::Idle {
                                    device.log(WRD_EMPTY, ErrMsg::MsgBCReady);
                                    let mut local_emitter = device_handler_emitter.lock().unwrap();
                                    local_emitter.handler.on_bc_ready(&mut device);
                                } 
                                // TODO: I need to find a new way to do timeouts.
                                // else if local_router.scheduler.bus_available() < device.clock.elapsed().as_nanos() {
                                //     device.log(WRD_EMPTY, ErrMsg::MsgAttk("timeout reached".to_string()));
                                //     device.set_state(State::Idle);
                                // }
                            }
    
                            // // write is `asynchrnoized`
                            // let mut current: u128 = device.clock.elapsed().as_nanos();
                            // wq = device.write_queue.len();
                            // while device.write_queue.len() > 0 && device.write_queue.front().unwrap().0 <= current {
                            //     let entry = device.write_queue.pop_front().unwrap();
                            //     // log can be slower than write ...
                            //     let wq = device.write_queue.len();
                            //     device.log_at(entry.0/1000, entry.1, ErrMsg::MsgWrt(wq));
                            //     for (i, s) in device.transmitters.iter().enumerate() {
                            //         if (i as u32) != device.id {
                            //             let _e = s.try_send(entry);
                            //             time_bus_available = entry.0 + w_delay;
                            //             // s.send(val);
                            //         }
                            //     }
                            //     // w_logs.push((entry.1, ErrMsg::MsgWrt));
                            // }
    
                            // write is `asynchrnoized`
                            let wq = device.write_queue.len();
                            let current = device.clock.elapsed().as_nanos();
                            if wq > 0 {
                                // let mut w_logs = Vec::new();
                                for entry in device.write_queue.clone().iter() {
                                    // if now it is the time to actually write
                                    if entry.0 <= current {
                                        // log can be slower than write ...
                                        device.log_at(entry.0/1000, entry.1, ErrMsg::MsgWrt(wq));
                                        for (i, s) in device.transmitters.iter().enumerate() {
                                            if (i as u32) != device.id {
                                                let _e = s.try_send(*entry);
                                                // s.send(val);
                                            }
                                        }
                                        // w_logs.push((entry.1, ErrMsg::MsgWrt));
                                    }
                                }
                                // for wl in w_logs {
                                //     device.log(wl.0, wl.1);
                                // }
                                // clearing all the data (otherwise delta_t keeps increasing)
                                device.write_queue.retain(|x| (*x).0 > current);
                                if device.write_queue.len() == 0 {
                                    device.number_of_current_cmd = 0;
                                }
                                // device.write_queue.clear();
                            }
    
                            let res = device.read();
                            if !res.is_err() {
                                let (time, mut word) = res.unwrap();
                                if prev_word.0 == 0 {
                                    // empty cache, do replacement
                                    prev_word = (time, true, word);
                                } else if (time as i128 - prev_word.0 as i128) < WORD_LOAD_TIME as i128 {
                                    // collision
                                    let mut w = res.unwrap();
                                    if word.address() == device.address {
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
                                            local_emitter
                                                .handler
                                                .on_err_parity(&mut device, &mut prev_word.2);
                                        }
                                        local_emitter.handler.on_err_parity(&mut device, &mut word);
                                    }
                                    // replaced with new timestamp, and invalid message flag (collided)
                                    prev_word = (current, false, word);
                                }
                            }
                            if prev_word.1 && current >= prev_word.0 + WORD_LOAD_TIME {
                                // message in the cache is valid & after word_time . processe the word.
                                let mut w = prev_word.2;
                                let mut local_emitter = device_handler_emitter.lock().unwrap();
                                device.atk_type = local_emitter.handler.get_attk_type();
    
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
    
                            // let mut res: Result<(u128, Word), TryRecvError>;
                            // // if prev_word.0 == 0 {
                            // //     res = device.maybe_block_read(); // Adding this line was marginally faster.  It may slow things down on a more capable computer.
                            // // } else {                             // This did slow things down on a faster computer.  By a significant margin.  Likely from context switching.
                            //     res = device.read();
                            // // }
                            // match res {
                            //     Ok(mut msg) => {
                            //         let (time, mut word) = msg;
                            //         if device.read_queue.is_empty() {
                            //             // empty cache, do replacement
                            //             if (time as i128 - time_bus_available as i128) < 0 { // We were transmitting when they started
                            //                 device.read_queue.push_back((time, word, false));
                            //             } else {
                            //                 device.read_queue.push_back((time, word, true));
                            //             }
                            //         } else if time - device.read_queue.back().unwrap().0 < COLLISION_TIME {
                            //             // collision
                            //             if device.read_queue.back().unwrap().2 {
                            //                 // if previous word is a valid message then file parity error
                            //                 // if not, the error was already filed.
                            //                 device.read_queue.back_mut().unwrap().2 = false;
                            //             }
                            //             // local_router.handler.on_err_parity(&mut device, &mut w);
                            //             // replaced with new timestamp, and invalid message flag (collided)
                            //             device.read_queue.push_back((time, word, false));
                            //         } else {
                            //             device.read_queue.push_back((time, word, true));
                            //         }
                            //     },
                            //     Err(msg) => {
                            //         if msg != TryRecvError::Empty {
                            //             device.log(WRD_EMPTY, ErrMsg::MsgAttk("ReadErr".to_string()));
                            //         }
                            //     }
                            // }
                            // while !device.read_queue.is_empty() && device.read_queue.front().unwrap().0 <= current - WORD_LOAD_TIME {
                            //     let (time, mut word, valid) = device.read_queue.pop_front().unwrap();
                            //     let mut local_emitter = device_handler_emitter.lock().unwrap();
                            //     if !valid {
                            //         let new_atk_type = local_emitter.handler.get_attk_type();
                            //         if new_atk_type != device.atk_type {
                            //             device.reset_all_stateful();
                            //             device.atk_type = new_atk_type;
                            //         }
                            //         local_emitter.handler.on_err_parity(&mut device, &mut word);
                            //     } else if word.sync() == 1 {
                            //         // device.ensure_idle();
                            //         if word.instrumentation_bit() == 1 {
                            //             local_emitter.handler.on_cmd(&mut device, &mut word)
                            //         } else {
                            //             // status word
                            //             local_emitter.handler.on_sts(&mut device, &mut word);
                            //             if word.service_request_bit() != 0 {
                            //                 // local_emitter.scheduler.request_sr(word.address());
                            //             }
                            //             if word.message_errorbit() != 0{
                            //                 // local_emitter.scheduler.error_bit();
                            //             }
                            //         }
                            //     } else {
                            //         // data word
                            //         local_emitter.handler.on_dat(&mut device, &mut word);
                            //     }
                            // }
                        }
                        if exit.load(Ordering::Relaxed) {
                            device.log(WRD_EMPTY, ErrMsg::MsgAttk("Ending thread".to_string()));
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
                    Arc::new(Mutex::new(EventHandlerEmitter {
                        handler: Box::new(DefaultBCEventHandler {
                            total_device: n_devices,
                            target: 0,
                            data: vec![1, 2, 3],
                            proto: proto,
                            proto_rotate: proto_rotate,
                        })
                    })),
                    AttackType::Benign.into(),
                );
            } else {
                sys.run_d(
                    m as u8,
                    Mode::RT,
                    Arc::new(Mutex::new(EventHandlerEmitter {
                        handler: Box::new(DefaultEventHandler {})
                    })),
                    AttackType::Benign.into(),
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
    