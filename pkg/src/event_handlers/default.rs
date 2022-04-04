use crate::primitive_types::{Word, ErrMsg, State, Mode, TR, WRD_EMPTY, BROADCAST_ADDRESS, ModeCode, AttackType};
use crate::schedulers::Proto;
use crate::devices::Device;
use crate::sys::System;

pub trait EventHandler: Send {
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
        self.default_on_sts(d, w)
    }
    fn on_bc_ready(&mut self, _: &mut Device) {}
    fn on_memory_ready(&mut self, _: &mut Device) {}
    fn on_data_write(&mut self, d: &mut Device, dword_count: u8) {
        self.default_on_data_write(d, dword_count);
    }

    fn default_on_data_write(&mut self, d: &mut Device, dword_count: u8) {
        for i in 0..dword_count {
            d.write(Word::new_data((i + 1) as u16));
        }
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
        if d.state == State::AwtData {
            d.error_bit = true;
        }
    }
    fn default_on_cmd(&mut self, d: &mut Device, w: &mut Word) {
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
            }
            else if w.tr() == TR::Receive {
                d.ensure_idle();
            }
            // // rt2rt sub destination
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
            d.write(Word::new_status(d.address, d.service_request, d.error_bit));
            self.on_data_write(d, w.dword_count());
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
        if d.address == w.address() {
            d.log(*w, ErrMsg::MsgEntCmdMcx);
            // may be triggered after cmd
            if !d.fake {
                // actual operation not triggerred for attackers
                // mode code match for command:
                match ModeCode::from(w.mode_code()) {
                    ModeCode::TXshutdown => {
                        // Mode code for TX shutdown
                        d.reset_all_stateful();
                        d.set_state(State::Off);
                    }
                    ModeCode::Synchronization => {
                        // synchronization
                        // ccmd indicating that the next data word
                        // is related to the current command
                        // (in this case, the clock to be synced)
                        d.ccmd = 1;
                        d.set_state(State::AwtData);
                    }
                    ModeCode::ClearCache => {
                        // clear cache (only when it is recieving data)
                        d.log(WRD_EMPTY, ErrMsg::MsgMCXClr(d.memory.len()));
                        d.reset_all_stateful();
                        d.set_state(State::Idle);
                        // clear write queue (cancel the status words to be sent)
                        d.write_queue.clear();
                    }
                    ModeCode::CancelCommand => {
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
            d.dword_count += 1;
            d.log(*w, ErrMsg::MsgEntDat(d.dword_count as usize, d.dword_count_expected as usize));
            if d.ccmd == 1 {
                // TBA:  synchronize clock to data
                // (clock is u128 but data is not u16..)
                // maybe set the microscecond component of the clock
                d.ccmd = 0;
            } else {
                if d.dword_count <= d.dword_count_expected {
                    d.memory.push(w.data());
                }
                if d.dword_count == d.dword_count_expected {
                    d.set_state(State::BusyTrx);
                    if d.mode != Mode::BC {
                        // only real RT will responding status message
                        if !d.fake {
                            d.write(Word::new_status(d.address, d.service_request, d.error_bit));
                        }
                    }
                    self.on_memory_ready(d);
                    d.reset_all_stateful();
                }
            }
        }
    }
    fn default_on_sts(&mut self, d: &mut Device, w: &mut Word){
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
                #[allow(unused)]
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
            if d.delta_t_start != 0 {
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
pub struct DefaultEventHandler {}

impl EventHandler for DefaultEventHandler {}


#[derive(Clone, Debug)]
pub struct DefaultBCEventHandler {
    // val: u8,
    // path: String,
    // data: Vec<u32>
    pub total_device: u8,
    pub target: u8,
    pub data: Vec<u16>,
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

pub struct EventHandlerEmitter {
    pub handler : Box<dyn EventHandler>
}

impl EventHandlerEmitter {}