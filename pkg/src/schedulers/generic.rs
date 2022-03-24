use crate::devices::Device;


#[allow(unused)]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Proto {
    RT2RT = 0,
    BC2RT = 1,
    RT2BC = 2,
}

#[derive(Clone, Debug)]
pub struct EmptyScheduler {}
impl Scheduler for EmptyScheduler {}

pub trait Scheduler: Clone + Send {
    #[allow(unused)]
    fn on_bc_ready(&mut self, d: &mut Device) {}
    #[allow(unused)]
    fn queue(&mut self, terminal: u8) {}
    #[allow(unused)]
    fn request_sr(&mut self, terminal: u8) {}
    #[allow(unused)]
    fn error_bit(&mut self) {}
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

    fn queue(&mut self, _terminal: u8) {

    }

    fn error_bit(&mut self) {
        
    }
}
