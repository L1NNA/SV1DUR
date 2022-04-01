pub mod attack1;
pub mod attack10;
pub mod attack2;
pub mod attack3;
pub mod attack4;
pub mod attack5;
pub mod attack6;
pub mod attack7;
pub mod attack8;
pub mod attack9;

use crate::sys::{
    format_log, AttackType, DefaultBCEventHandler, DefaultEventHandler, Device, ErrMsg,
    EventHandler, EventHandlerEmitter, Mode, Proto, State, System, Word, TR, WRD_EMPTY,
};
use std::sync::{Arc, Mutex};

pub struct AttackController {
    pub current_attack: AttackType,
}
