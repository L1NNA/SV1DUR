trait Component {
    fn data_for_remote(&self, &remote: Address) -> Vec<Word>{}
    fn data_from_remote(&self, &remote: Address) {}
}

impl Component for FlightControls {
    fn data_for_bus(&self, &remote: Address) {
        match remote {
            
        }
    }
}

bitfield!{
    pub struct Brakes {
        impl Debug;
        f32;
        pub torque1, set_torque1: 31, 0;
        pub torque2, set_torque2: 63, 32;
        pub torque3, set_torque3: 95, 64;
        pub load1, set_load1: 127, 96;
        pub load2, set_load2: 159, 128;
        pub load3, set_load3: 191, 160;
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
    }
}

impl update_state(&self, new_state: Vec<f32>) for Brakes {
    self.set_torque1(new_state.pop());
    self.set_torque2(new_state.pop());
    self.set_torque3(new_state.pop());
    self.set_load1(new_state.pop());
    self.set_load2(new_state.pop());
    self.set_load3(new_state.pop());
}

impl Component for Brakes {
    fn data_for_remote(&self, &remote: Address) -> Vec<Word>{
        let ret = Vec::new();
        ret.push(Word::new_data(self.word1));
        ret.push(Word::new_data(self.word2));
        ret.push(Word::new_data(self.word3));
        ret.push(Word::new_data(self.word4));
        ret.push(Word::new_data(self.word5));
        ret.push(Word::new_data(self.word6));
        ret.push(Word::new_data(self.word7));
        ret.push(Word::new_data(self.word8));
        ret.push(Word::new_data(self.word9));
        ret.push(Word::new_data(self.word10));
        ret.push(Word::new_data(self.word11));
        ret.push(Word::new_data(self.word12));
        return ret;
    }

    fn data_from_remote(&self, &remote: Address, data: Vec<f32>) {
        // apply brakes
    }
}

impl EventHandler for Component {
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
}

bitfield! {
    pub struct Heading {
        impl Debug;
        pub heading, set_heading: 31, 0;
        pub word1, _: 15, 0;
        pub word2, _: 31, 16;
    }
}

impl update_state(&self, new_state: f32) for Heading {
    self.set_heading(new_state);
}

impl Component for Heading {
    fn data_for_remote(&self, &remote: Address) -> Vec<Word> {
        let ret = Vec::new();
        ret.push(Word::new_data(self.word1));
        ret.push(Word::new_data(self.word2));
        return ret;
    }

    fn data_from_remote(&self, &remote: Address) {
        // nothing to implement
    }
}

bitfield! {
    pub struct Fuel {
        impl Debug;
        pub flow, set_flow: 31, 0;
        pub quantity, set_quantity: 63, 32;
        pub word1, _: 15, 0;
        pub word2, _: 31, 16;
        pub word3, _: 47, 32;
        pub word4, _: 63, 48;
    }
}

impl update_state(&self, new_state: Vec<f32>) for Fuel {
    self.set_flow(new_state.pop());
    self.set_quantity(new_state.pop());
}

impl Component for Fuel {
    fn data_for_remote(&self, &remote: Address) -> Vec<Word> {
        let ret = Vec::new();
        ret.push(Word::new_data(self.word1));
        ret.push(Word::new_data(self.word2));
        ret.push(Word::new_data(self.word3));
        ret.push(Word::new_data(self.word4));
        return ret;
    }

    fn data_from_remote(&self, &remote: Address) {
        // nothing to implement
    }
}