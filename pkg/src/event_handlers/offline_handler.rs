use crate::primitive_types::{Word, ErrMsg, State, Mode, TR, WRD_EMPTY};
use crate::devices::Device;

pub const BROADCAST_ADDRESS: u8 = 31;

pub struct OfflineHandler {

}

impl EventHandler for OfflineHandler {
    fn on_cmd_trx(&mut self, d: &mut Device, w: &mut Word) {
        // may be triggered after cmd
        d.log(*w, ErrMsg::MsgEntCmdTrx);
        if !d.fake {
            d.set_state(State::BusyTrx);
            d.write(Word::new_status(d.address, d.service_request, d.error_bit));
            for i in 0..w.dword_count() {
                d.write(Word::new_data((i + 1) as u32));
            }
        }
        let current_cmds = d.reset_all_stateful();
        d.number_of_current_cmd = current_cmds;
    }
}
