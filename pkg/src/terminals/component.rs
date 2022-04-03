use bitfield::bitfield;
use crate::primitive_types::{Address, Word};
use std::any::type_name;

// trait Component {
//     fn data_for_remote(&self, &remote: Address) -> Vec<Word>{}
//     fn data_from_remote(&self, &remote: Address) {}
// }

// impl Component for FlightControls {
//     fn data_for_bus(&self, &remote: Address) {
//         match remote {
            
//         }
//     }
// }

bitfield! {
    pub struct SplitInt(u32);
    impl Debug;
    u16;
    pub data1, set_data1: 31, 0;
    pub word1, _: 15, 0;
    pub word2, _: 31, 16;
}

impl SplitInt {
    pub fn new(var: u32) -> SplitInt {
        let int = SplitInt {0: var};
        int
    }

    pub fn extract(&mut self) -> Vec<u16> {
        vec![self.word1(), self.word2()]
    }
}

// bitfield! {
//     pub struct SplitFloat(f32);
//     impl Debug;
//     u16;
//     pub data1, set_data1: 31, 0;
//     pub word1, _: 15, 0;
//     pub word2, _: 31, 16;
// }

// impl SplitFloat {
//     pub fn new(var: f32) -> SplitFloat {
//         let float = SplitFloat {0: var};
//         float
//     }

//     pub fn extract(&mut self) -> Vec<u16> {
//         vec![self.word1(), self.word2()]
//     }
// }

bitfield!{
    pub struct ComponentInfo(u32);
    impl Debug;
    u16;
    pub data1, set_data1: 31, 0;
    pub data2, set_data2: 63, 32;
    pub data3, set_data3: 95, 64;
    pub data4, set_data4: 127, 96;
    pub data5, set_data5: 159, 128;
    pub data6, set_data6: 191, 160;
    pub data7, set_data7: 223, 192;
    pub word1, _: 15, 0;
    pub word2, _: 31, 16;
    pub word3, _: 47, 32;
    pub word4, _: 63, 48;
    pub word5, _: 79, 64;
    pub word6, _: 95, 80;
    pub word7, _: 111, 96;
    pub word8, _: 127, 112;
    pub word9, _: 143, 128;
    pub word10, _: 159, 144;
    pub word11, _: 175, 160;
    pub word12, _: 191, 176;
    pub word13, _: 207, 192;
    pub word14, _: 223, 208;
}

impl ComponentInfo {
    pub fn new() -> ComponentInfo {
        let mut info = ComponentInfo {0:0};
        return info;
    }

    pub fn update(&mut self, new_data: Vec<u16>) {
        let mut data_qty = new_data.len();
        if data_qty > 0 {
            self.set_data1(new_data[0]);
        }
        if data_qty > 1 {
            self.set_data2(new_data[1]);
        }
        if data_qty > 2 {
            self.set_data3(new_data[2]);
        }
        if data_qty > 3 {
            self.set_data4(new_data[3]);
        }
        if data_qty > 4 {
            self.set_data5(new_data[4]);
        }
        if data_qty > 5 {
            self.set_data6(new_data[5]);
        }
        if data_qty > 6 {
            self.set_data7(new_data[6]);
        }
    }

    pub fn extract(&mut self, data_qty: u8) -> Option<Vec<u16>> {
        if data_qty == 0 {
            ()
        }
        let mut data : Vec<u16> = Vec::new();
        if data_qty > 0 {
            data.push(self.word1());
            data.push(self.word2());
        }
        if data_qty > 1 {
            data.push(self.word3());
            data.push(self.word4());
        }
        if data_qty > 2 {
            data.push(self.word5());
            data.push(self.word6());
        }
        if data_qty > 3 {
            data.push(self.word7());
            data.push(self.word8());
        }
        if data_qty > 4 {
            data.push(self.word9());
            data.push(self.word10());
        }
        if data_qty > 5 {
            data.push(self.word11());
            data.push(self.word12());
        }
        if data_qty > 6 {
            data.push(self.word13());
            data.push(self.word14());
        }
        Some(data)
    }
}

// bitfield!{
//     pub struct Brakes {
//         impl Debug;
//         f32;
//         pub torque1, set_torque1: 31, 0;
//         pub torque2, set_torque2: 63, 32;
//         pub torque3, set_torque3: 95, 64;
//         pub load1, set_load1: 127, 96;
//         pub load2, set_load2: 159, 128;
//         pub load3, set_load3: 191, 160;
//         pub word1, _: 15, 0;
//         pub word2, _: 31, 16;
//         pub word3, _: 47, 32;
//         pub word4, _: 63, 48;
//         pub word5, _: 79, 64;
//         pub word6, _: 95, 80;
//         pub word7, _: 111, 96;
//         pub word8, _: 127, 112;
//         pub word9, _: 143, 128;
//         pub word10, _: 159, 144;
//         pub word11, _: 175, 160;
//         pub word12, _: 191, 176;
//     }
// }

// // impl update_state(&self, new_state: Vec<f32>) for Brakes {
// //     self.set_torque1(new_state.pop());
// //     self.set_torque2(new_state.pop());
// //     self.set_torque3(new_state.pop());
// //     self.set_load1(new_state.pop());
// //     self.set_load2(new_state.pop());
// //     self.set_load3(new_state.pop());
// // }

// impl Component for Brakes {
//     fn data_for_remote(&self, &remote: Address) -> Vec<Word>{
//         let ret = Vec::new();
//         ret.push(Word::new_data(self.word1));
//         ret.push(Word::new_data(self.word2));
//         ret.push(Word::new_data(self.word3));
//         ret.push(Word::new_data(self.word4));
//         ret.push(Word::new_data(self.word5));
//         ret.push(Word::new_data(self.word6));
//         ret.push(Word::new_data(self.word7));
//         ret.push(Word::new_data(self.word8));
//         ret.push(Word::new_data(self.word9));
//         ret.push(Word::new_data(self.word10));
//         ret.push(Word::new_data(self.word11));
//         ret.push(Word::new_data(self.word12));
//         return ret;
//     }

//     fn data_from_remote(&self, &remote: Address, data: Vec<f32>) {
//         // apply brakes
//     }
// }

// impl EventHandler for Component {
//     fn default_on_cmd_trx(&mut self, d: &mut Device, w: &mut Word) {
//         // may be triggered after cmd
//         d.log(*w, ErrMsg::MsgEntCmdTrx);
//         if !d.fake {
//             d.set_state(State::BusyTrx);
//             d.write(Word::new_status(d.address));
//             for i in 0..w.dword_count() {
//                 d.write(Word::new_data((i + 1) as u32));
//             }
//         }
//         d.reset_all_stateful();
//     }
// }

// bitfield! {
//     pub struct Heading {
//         impl Debug;
//         pub heading, set_heading: 31, 0;
//         pub word1, _: 15, 0;
//         pub word2, _: 31, 16;
//     }
// }

// // impl update_state(&self, new_state: f32) for Heading {
// //     self.set_heading(new_state);
// // }

// impl Component for Heading {
//     fn data_for_remote(&self, &remote: Address) -> Vec<Word> {
//         let ret = Vec::new();
//         ret.push(Word::new_data(self.word1));
//         ret.push(Word::new_data(self.word2));
//         return ret;
//     }

//     fn data_from_remote(&self, &remote: Address) {
//         // nothing to implement
//     }
// }

// bitfield! {
//     pub struct Fuel {
//         impl Debug;
//         pub flow, set_flow: 31, 0;
//         pub quantity, set_quantity: 63, 32;
//         pub word1, _: 15, 0;
//         pub word2, _: 31, 16;
//         pub word3, _: 47, 32;
//         pub word4, _: 63, 48;
//     }
// }

// // impl update_state(&self, new_state: Vec<f32>) for Fuel {
// //     self.set_flow(new_state.pop());
// //     self.set_quantity(new_state.pop());
// // }

// impl Component for Fuel {
//     fn data_for_remote(&self, &remote: Address) -> Vec<Word> {
//         let ret = Vec::new();
//         ret.push(Word::new_data(self.word1));
//         ret.push(Word::new_data(self.word2));
//         ret.push(Word::new_data(self.word3));
//         ret.push(Word::new_data(self.word4));
//         return ret;
//     }

//     fn data_from_remote(&self, &remote: Address) {
//         // nothing to implement
//     }
// }