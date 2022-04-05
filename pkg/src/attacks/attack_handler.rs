use crate::event_handlers::{EventHandler};
use crate::primitive_types::{Address, State, Word, WRD_EMPTY, ErrMsg, TR};
use crate::devices::Device;

#[derive(Clone, Debug, PartialEq)]
pub enum AttackSelection {
    NoAttack,
    Attack1(u8),
    Attack2(Address),
    Attack3(Address),
    Attack4(Address, Address),
    Attack5(Address),
    Attack6(Address),
    Attack7(Address),
    Attack8(Address),
    Attack9(Address),
    Attack10(Address)
}

pub struct AttackHandler {
    attack: AttackSelection,
    state: State,
    words_expected: u8,
    rapid_fire: bool,
    temp_source: Address,
    temp_target: Address
}

impl AttackHandler {
    pub fn new() -> Self {
        AttackHandler{attack: AttackSelection::NoAttack, 
            state: State::Idle, 
            words_expected: 0, 
            rapid_fire: false, 
            temp_source: Address::FlightControls, 
            temp_target: Address::Ailerons}
    }

    pub fn inject(&mut self, d: &mut Device, dword_count: u8) {
        for i in 0..dword_count {
            let w = Word::new_data(i as u16);
            d.log(
                WRD_EMPTY,
                ErrMsg::MsgAttk(format!("Sent Fake Data {} ", w).to_string()),
            );
            d.write(w);
        }
        self.end_attack();
    }

    pub fn inject_command_word(&mut self, d: &mut Device, target: Address, mode_code: u8) {
        let mut w = Word::new_cmd(target as u8, mode_code, TR::Receive);
        w.set_mode(1);
        d.write(w);
        self.end_attack();
    }

    pub fn inject_status_word(&mut self, d: &mut Device, target: Address) {
        d.write(Word::new_malicious_status(target as u8));
        d.log(
            WRD_EMPTY,
            ErrMsg::MsgAttk(format!("Fake status injected!").to_string()),
        );
        self.end_attack();
    }

    pub fn start_mitm(&mut self, d: &mut Device) {
        d.log(
            WRD_EMPTY,
            ErrMsg::MsgAttk(format!("Starting MITM attack...").to_string()),
        );
        d.set_state(State::Off);
        let word_count = self.words_expected;
        let mut w = Word::new_cmd(self.temp_source as u8, word_count, TR::Receive);
        d.write(w);
        w.set_address(self.temp_target as u8);
        w.set_tr(TR::Transmit as u8);
        d.write(w);
        d.set_state(State::Idle);
        self.end_attack();
    }

    pub fn desynchronize_rt(&mut self, d: &mut Device, target: Address) {
        d.log(
            WRD_EMPTY,
            ErrMsg::MsgAttk(
                format!("Attacker>> Desynchronizing RT{} ...", target as u8).to_string(),
            ),
        );
        let word_count = 17;
        d.write(Word::new_cmd(target as u8, word_count, TR::Receive));
        d.write(Word::new_data(0x000F));
        self.end_attack();
    }

    pub fn invalidate_command(&mut self, d: &mut Device, target: Address) {
        let word_count = 31;
        let w = Word::new_cmd(target as u8, word_count, TR::Receive);
        d.write(w);
        d.log(
            WRD_EMPTY,
            ErrMsg::MsgAttk(format!("Attacker>> Injecting fake command on RT{}", w).to_string()),
        );
        self.end_attack();
    }

    pub fn end_attack(&mut self) {
        self.state == State::Idle;
        if !self.rapid_fire {
            self.attack == AttackSelection::NoAttack;
        }
    }
}

impl EventHandler for AttackHandler {
    fn set_attk_type(&mut self, attack: AttackSelection) {
        println!("Setting attack type");
        self.attack = attack;
    }

    fn on_cmd(&mut self, d: &mut Device, w: &mut Word) {
        d.log(*w, ErrMsg::MsgAttk("on_cmd".to_string()));
        match self.attack {
            AttackSelection::Attack1(dword_count) => {
                d.log(
                    *w,
                    ErrMsg::MsgAttk("Jamming launched (after cmd)".to_string()),
                );
                self.inject(d, dword_count);
            },
            AttackSelection::Attack2(target) => {
                if w.address() == target as u8 { // Should we look at handling transmit and receive differently?
                    d.log(
                        *w,
                        ErrMsg::MsgAttk("Jamming launched (after cmd)".to_string()),
                    );
                    self.inject(d, w.dword_count());
                }
            },
            AttackSelection::Attack3(target) => {
                if w.address() == target as u8 && w.tr() == TR::Receive {
                    d.log(
                        *w,
                        ErrMsg::MsgAttk(format!(">>> Thrashing triggered (after cmd_word)").to_string()),
                    );
                    self.state = State::AwtData
                }
            },
            AttackSelection::Attack4(src_target, dst_target) => {
                // State::AwtData means both targets are identified. 
                // State::AwtStsTrxR2R means that we have found the receiver but not the transmitter.
                if self.state != State::AwtData {
                    if w.tr() == TR::Receive && w.address() == dst_target as u8 {
                        self.words_expected == w.dword_count();
                        self.state == State::AwtStsTrxR2R(0, 0);
                        d.log(
                            WRD_EMPTY,
                            ErrMsg::MsgAttk(
                                format!("Atttacker>> Target dst identified (RT{})", dst_target as u8)
                                    .to_string(),
                            ),
                        );
                    } else if w.tr() == TR::Transmit && w.address() == src_target as u8 {
                        self.state == State::AwtData;
                        d.log(
                            WRD_EMPTY,
                            ErrMsg::MsgAttk(
                                format!("Atttacker>> Target src identified (RT{})", src_target as u8)
                                    .to_string(),
                            ),
                        );
                    }
                }
            },
            AttackSelection::Attack5(target) => {
                if w.address() == target as u8 {
                    d.log(
                        WRD_EMPTY,
                        ErrMsg::MsgAttk(format!("Attacker>> Killing RT{}", target as u8).to_string()),
                    );
                    let mode_code = 4;
                    self.inject_command_word(d, target, mode_code);
                }
            },
            AttackSelection::Attack6(target) => {
                if self.state != State::AwtData {
                    if w.address() == target as u8 && w.tr() == TR::Receive {
                        self.words_expected = w.dword_count();
                        d.log(
                            WRD_EMPTY,
                            ErrMsg::MsgAttk(
                                format!("Attacker>> Target detected (RT{:02})", target as u8).to_string(),
                            ),
                        );
                        self.state = State::AwtData;
                        self.words_expected = w.dword_count();
                        self.temp_source = Address::from(w.address());
                        d.log(
                            WRD_EMPTY,
                            ErrMsg::MsgAttk(format!("Fake status triggered (after cmd_word)").to_string()),
                        );
                    }
                }
            },
            AttackSelection::Attack7(target) => {
                if w.address() == target as u8 && w.tr() == TR::Transmit {
                    d.log(
                        WRD_EMPTY,
                        ErrMsg::MsgAttk(
                            format!("Attacker>> Target detected (RT{})", target as u8).to_string(),
                        ),
                    );
                    d.log(
                        WRD_EMPTY,
                        ErrMsg::MsgAttk(
                            format!("Sending fake status word (after tr_cmd_word)").to_string(),
                        ),
                    );
                    self.inject_status_word(d, target);
                }
            },
            AttackSelection::Attack8(target) => {
                if w.address() == target as u8 && self.state != State::AwtData {
                    if w.tr() == TR::Receive {
                        self.words_expected = w.dword_count();
                    }
                    self.state == State::AwtData;
                    d.log(
                        *w,
                        ErrMsg::MsgAttk(
                            format!("Attacker>> Target detected(RT{})", target as u8).to_string(),
                        ),
                    );
                }
            },
            AttackSelection::Attack9(target) => {
                if w.address() == target as u8 && w.tr() == TR::Transmit {
                    self.words_expected = w.dword_count();

                    // do we need the sub address?
                    d.log(
                        *w,
                        ErrMsg::MsgAttk(
                            format!("Attacker>> Target detected(RT{})", target as u8).to_string(),
                        ),
                    );
                    self.inject_status_word(d, target);
                    self.inject(d, self.words_expected);
                }
            },
            AttackSelection::Attack10(target) => {
                if w.address() == target as u8 && w.tr() == TR::Transmit {
                    d.log(
                        *w,
                        ErrMsg::MsgAttk(
                            format!("Attacker>> Target detected(RT{})", target as u8).to_string(),
                        ),
                    );
                    self.inject(d, w.dword_count());
                }
            },
            _ => {d.log(*w, ErrMsg::MsgAttk("".to_string()));}
        }
        // self.default_on_cmd(d, w);  // This code will cause us to "ensure_idle" on the receive command.  This may cause certain attacks to not work.
    }

    fn on_dat(&mut self, d: &mut Device, w: &mut Word) {
        match self.attack {
            AttackSelection::Attack1(dword_count) => {
                d.log(
                    *w,
                    ErrMsg::MsgAttk("Jamming launched (after data)".to_string()),
                );
                self.inject(d, dword_count as u8);
            },
            AttackSelection::Attack3(target) => {
                if self.state == State::AwtData && self.words_expected > 0 {
                    self.words_expected -= 1;
                    if self.words_expected == 0 {
                        d.log(
                            WRD_EMPTY,
                            ErrMsg::MsgAttk(format!(">>> Fake command injected!").to_string()),
                        );
                        let mode_code = 30;
                        self.inject_command_word(d, target, mode_code);
                    }
                }
            },
            AttackSelection::Attack6(target) => {
                if self.state == State::AwtData {
                    self.words_expected -= 1;
                    if self.words_expected == 0 {
                        self.inject_status_word(d, target);
                        self.state = State::Idle
                    }
                }
            },
            AttackSelection::Attack8(target) => {
                if self.state == State::AwtData {
                    self.words_expected -= 1;
                    if self.words_expected == 0 {
                        self.desynchronize_rt(d, target);
                    }
                }
            },
            _ => {}

        }
        // self.default_on_dat(d, w);
    }


    fn on_sts(&mut self, d: &mut Device, w: &mut Word) {
        match self.attack {
            AttackSelection::Attack1(dword_count) => {
                d.log(
                    *w,
                    ErrMsg::MsgAttk("Jamming launched (after status)".to_string()),
                );
                self.inject(d, dword_count as u8);
            },
            AttackSelection::Attack4(src_target, dst_target) => {
                if src_target as u8 == w.address() && self.state == State::AwtData {
                    self.start_mitm(d);
                } else if dst_target as u8 == w.address() && self.state == State::Idle {
                    d.log(
                        WRD_EMPTY,
                        ErrMsg::MsgAttk(format!(
                            "Attacker>> Man in the Middle Successfully Completed!"
                        )),
                    );
                }
            },
            AttackSelection::Attack5(target) => {
                if w.address() == target as u8 {
                    d.log(
                        WRD_EMPTY,
                        ErrMsg::MsgAttk(format!("Attacker>> Killing RT{}", target as u8).to_string()),
                    );
                    let mode_code = 4;
                    self.inject_command_word(d, target, mode_code);
                }
            },
            AttackSelection::Attack8(target) => {
                match self.state {
                    State::AwtStsTrxR2R(_,_) => self.desynchronize_rt(d, target),
                    _ => {},
                };
            },
            _ => {}
        }
        // self.default_on_sts(d, w);
    }
}
