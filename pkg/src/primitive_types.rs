use bitfield::bitfield;
use std::fmt;

pub const WRD_EMPTY: Word = Word { 0: 0 };
pub const ATK_DEFAULT_DELAYS: u128 = 4000;
pub const CONFIG_PRINT_LOGS: bool = false;
pub const CONFIG_SAVE_DEVICE_LOGS: bool = true;
pub const CONFIG_SAVE_SYS_LOGS: bool = true;
pub const BROADCAST_ADDRESS: u8 = 31;

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
    pub into TR, tr, set_tr: 8, 8;
    // it was 13, 9 but since we use instrumentation bit
    // we have kept reduce the sub-address space to 15.
    pub sub_address, set_sub_address: 13, 10;
    pub mode, set_mode: 13, 11;
    // pub mode, set_mode: 13, 9;
    pub dword_count, set_dword_count: 18, 14;
    pub mode_code, set_mode_code: 18, 14;
    // for data word
    u32;
    pub all,_ : 20, 0;
    pub data, set_data: 18, 3;
    // additional (attack type):
    pub attk, set_attk: 24,21;
}

impl fmt::Display for Word {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "w:{:#027b}[{:02}]", self.0, self.attk()) // We need an extra 2 bits for '0b' on top of the number of bits we're printing
    }
}

impl Word {
    pub fn new_status(addr: u8, service_request: bool, error_bit: bool) -> Word {
        let mut w = Word { 0: 0 };
        w.set_sync(1);
        w.set_address(addr);
        w.set_service_request_bit(service_request as u8);
        w.set_message_errorbit(error_bit as u8);
        w.calculate_parity_bit();
        return w;
    }

    pub fn new_malicious_status(addr: u8) -> Word {
        let mut w = Word {0: 0};
        w.set_sync(1);
        w.set_address(addr);
        w.calculate_parity_bit();
        return w;
    }

    pub fn new_data(val: u32) -> Word {
        let mut w = Word { 0: 0 };
        w.set_data(val as u32);
        w.calculate_parity_bit();
        return w;
    }

    pub fn new_cmd(addr: u8, dword_count: u8, tr: TR) -> Word {
        let mut w = Word { 0: 0 };
        w.set_sync(1);
        w.set_tr(tr as u8); // 1: transmit, 0: receive
        w.set_address(addr); // the RT address which is five bits long
                             // address 11111 (31) is reserved for broadcast protocol

        w.set_dword_count(dword_count); // the quantity of data that will follow after the command
        w.set_mode(2);
        w.set_instrumentation_bit(1);
        w.calculate_parity_bit();
        return w;
    }
    #[allow(unused)]
    pub fn calculate_parity_bit(&mut self) {
        /*
        This code will calculate and apply the parity bit.  This will not affect other bits in the bitfield.
        */
        // let mask = u32::MAX - 1; //MAX-1 leaves the paritybit empty (I think this assumption may be wrong.  I think this is actually the sync bits)
        let mask = u32::MAX - 2u32.pow(19); // This will likely be the code we need.  It keeps all of the bits outside of the "19" bit.
        let int = self.all() & mask;
        let parity_odd = true;
        if int.count_ones() % 2 == 0 {
            self.set_parity_bit(!parity_odd as u8);
        } else {
            self.set_parity_bit(parity_odd as u8);
        }
    }
}

#[allow(unused)]
#[derive(Clone, Debug, PartialEq)]
pub enum ErrMsg {
    MsgEmpt,
    // show write queue size
    MsgWrt(usize),
    MsgBCReady,
    // show write queue size
    MsgStaChg(usize),
    MsgEntWrdRec,
    MsgEntErrPty,
    MsgEntCmd,
    MsgEntCmdRcv,
    MsgEntCmdTrx,
    MsgEntCmdMcx,
    MsgEntDat,
    MsgEntSte,
    // dropped status word
    MsgEntSteDrop,
    MsgAttk(String),
    MsgMCXClr(usize),
}

impl ErrMsg {
    pub fn value(&self) -> String {
        use ErrMsg::*;
        match self {
            MsgEmpt => "".to_owned(),
            // show write queue size
            MsgWrt(wq) => format!("Wrt({})", wq).to_string(),
            MsgBCReady => "BC is ready".to_owned(),
            MsgStaChg(wq) => format!("Status Changed({})", wq).to_string(),
            MsgEntWrdRec => "Word Received".to_owned(),
            MsgEntErrPty => "Parity Error".to_owned(),
            MsgEntCmd => "CMD Received".to_owned(),
            MsgEntCmdRcv => "CMD RCV Received".to_owned(),
            MsgEntCmdTrx => "CMD TRX Received".to_owned(),
            MsgEntCmdMcx => "CMD MCX Received".to_owned(),
            MsgEntDat => "Data Received".to_owned(),
            MsgEntSte => "Status Received".to_owned(),
            MsgEntSteDrop => "Status Dropped".to_owned(),
            MsgAttk(msg) => msg.to_owned(),
            // mode change
            MsgMCXClr(mem_len) => format!("MCX[{}] Clr", mem_len),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum TR {
    Receive = 0,
    Transmit = 1,
}

impl From<u8> for TR {
    fn from(value: u8) -> Self {
        use TR::*;
        match value {
            0 => Receive,
            _ => Transmit,
        }
    }
}

#[allow(unused)]
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

#[allow(unused)]
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
    AwtStsRcvB2R(u8),
    // rt2bc - bc waiting for the transmitter status code
    AwtStsTrxR2B(u8),
    // rt2rt - bc waiting for reciever status code
    AwtStsRcvR2R(u8, u8),
    // rt2rt - bc waiting for the transmitter status code
    AwtStsTrxR2R(u8, u8),
}
impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}


#[allow(unused)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Address {
    BusControl,
    FlightControls,
    Trim,
    Engine,
    Flaps,
    LandingGear,
    Weapons,
    Radar,
    Rover,
    Radio,
    Rudder,
    Ailerons,
    Elevators,
    Slats,
    Spoilers,
    Brakes,
    Fuel,
    Heading,
    Altimeter,
    Positioning, //GPS
    Pitch,
    ClimateControl,
    Tailhook,
    Gyro,
    Climate,
    Broadcast = 31,
}

#[allow(unused)]
impl Address {
    pub fn priority(&self, destination: &Address) -> MsgPri {
        // Defines the "priority" for each pairing of devices.  
        // This priority is used to determine how quickly the next message should be sent.
        use Address::*;
        use MsgPri::*;
        match (self, destination) {
            // With Feedback
            (FlightControls,   Trim)              => Low,
            (Trim,             FlightControls)    => Lowest,
            (FlightControls,   Flaps)             => Low,
            (Flaps,            FlightControls)    => Lowest,
            (FlightControls,   Engine)            => VeryHigh,
            (Engine,           FlightControls)    => High,
            (FlightControls,   LandingGear)       => Low,
            (LandingGear,      FlightControls)    => Lowest,
            (FlightControls,   Weapons)           => VeryHigh,
            (Weapons,          FlightControls)    => Medium,
            // Without Feedback
            (FlightControls, Rudder)      => VeryHigh,
            (FlightControls, Ailerons)    => VeryHigh,
            (FlightControls, Elevators)   => VeryHigh,
            (FlightControls, Slats)       => VeryHigh,
            (FlightControls, Spoilers)    => VeryHigh,
            (FlightControls, Brakes)      => High,
            //Sensors
            (Fuel,         FlightControls) => Lowest,
            (Heading,      FlightControls) => Medium,
            (Altimeter,    FlightControls) => Medium,
            (Positioning,  FlightControls) => Lowest,
            (Pitch,        FlightControls) => Medium,
            /*
            Add in steering for the front wheel?
            Climate control?
            Radar
            ROVER - 
            Tailhook
            */
            _ => VeryHigh,
        }
    }

    pub fn repeat_function(&self, destination: &Address) -> bool {
        // This dictates whether or not a message will be repeated on a regular frequency.
        use Address::*;
        use MsgPri::*;
        match (self, destination) {
            // With Feedback
            (FlightControls,   Trim)              => true,
            (Trim,             FlightControls)    => true,
            (FlightControls,   Flaps)             => true,
            (Flaps,            FlightControls)    => true,
            (FlightControls,   Engine)            => true,
            (Engine,           FlightControls)    => true,
            (FlightControls,   LandingGear)       => true,
            (LandingGear,      FlightControls)    => true,
            (FlightControls,   Weapons)           => true,
            (Weapons,          FlightControls)    => true,
            // Without Feedback
            (FlightControls, Rudder)      => true,
            (FlightControls, Ailerons)    => true,
            (FlightControls, Elevators)   => true,
            (FlightControls, Slats)       => true,
            (FlightControls, Spoilers)    => true,
            (FlightControls, Brakes)      => true,
            //Sensors
            (Fuel,         FlightControls) => true,
            (Heading,      FlightControls) => true,
            (Altimeter,    FlightControls) => true,
            (Positioning,  FlightControls) => true,
            (Pitch,        FlightControls) => true,
            _ => false,
        }
    }

    pub fn word_count(&self, destination: &Address) -> u8 {
        // This dictates the number of words that need to be passed between the devices to transfer all of the data.
        use Address::*;
        use MsgPri::*;
        match (self, destination) {
            // With Feedback
            (FlightControls,   Trim)              => 2, //one float32 should carry sufficient data
            (Trim,             FlightControls)    => 2,
            (FlightControls,   Flaps)             => 1, //A single u4 could do it, but we're going to send a whole word
            (Flaps,            FlightControls)    => 1, // Planes can have leading and trailing edge flaps.  I don't know if they are controlled separately
            (FlightControls,   Engine)            => 8, //We'll estimate a float32 for each of the engines (up to four engines) and 2 words per float32
            (Engine,           FlightControls)    => 8, //Temperature, speed, 
            (FlightControls,   LandingGear)       => 1, //Binary message, but we'll send a whole word
            (LandingGear,      FlightControls)    => 1,
            (FlightControls,   Weapons)           => 4, //Targeting information along with the weapon selected and whether or not to open the compartment
            (Weapons,          FlightControls)    => 20, //confirmation data of currently configured system
                                                        // 578 rounds of M61A1 Vulcan
                                                        // 9 rockets
                                                        // Bomb

            // Without Feedback
            (FlightControls, Rudder)      => 2, //float32 for degree
            (FlightControls, Ailerons)    => 4, //float32 for degree on each wing
            (FlightControls, Elevators)   => 4, //float32 for degree on each wing
            (FlightControls, Slats)       => 4, //float32 for degree on each wing
            (FlightControls, Spoilers)    => 4, //float32 for degree on each wing
            (FlightControls, Brakes)      => 4, //float32 for degree on each side
                                                //Brakes should have torque sensor
                                                //Load on wheel sensor
            //Sensors
            (Fuel,         FlightControls) => 4, 
            (Heading,      FlightControls) => 2, 
            (Altimeter,    FlightControls) => 1,
            (Positioning,  FlightControls) => 3, 
            (Pitch,        FlightControls) => 6, 
            _ => 2, //2 words for anything unlisted
        }
    }

    pub fn on_sr(&self) -> (Address, u8) {
        use Address::*;
        match self {

            Weapons => (FlightControls, 20),
            _ => (FlightControls, 2),
            // I also need to know how many words to send. 
            // An i16 let's me use -1 as a sentinel value to indicate that the device will specify.  We could also just use any value.
        }
    }
}

impl From<u8> for Address {
    fn from(value: u8) -> Self {
        use Address::*;
        match value {
            value if value == BusControl as u8 => BusControl,
            value if value == FlightControls as u8 => FlightControls,
            value if value == Trim as u8 => Trim,
            value if value == Engine as u8 => Engine,
            value if value == Flaps as u8 => Flaps,
            value if value == LandingGear as u8 => LandingGear,
            value if value == Weapons as u8 => Weapons,
            value if value == Radar as u8 => Radar,
            value if value == Rover as u8 => Rover,
            value if value == Radio as u8 => Radio,
            value if value == Rudder as u8 => Rudder,
            value if value == Ailerons as u8 => Ailerons,
            value if value == Elevators as u8 => Elevators,
            value if value == Slats as u8 => Slats,
            value if value == Spoilers as u8 => Spoilers,
            value if value == Brakes as u8 => Brakes,
            value if value == Fuel as u8 => Fuel,
            value if value == Heading as u8 => Heading,
            value if value == Altimeter as u8 => Altimeter,
            value if value == Positioning as u8 => Positioning, //GPS
            value if value == Pitch as u8 => Pitch,
            value if value == ClimateControl as u8 => ClimateControl,
            value if value == Tailhook as u8 => Tailhook,
            value if value == Gyro as u8 => Gyro,
            value if value == Climate as u8 => Climate,
            _ => Broadcast,
        }
    }
}

#[allow(unused)]
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub enum MsgPri {
    Immediate,
    VeryHigh,
    High,
    Medium,
    Low,
    VeryLow,
    Lowest,
}

#[allow(unused)]
impl MsgPri {
    pub fn delay(&self) -> u32 {
        // The amount of delay to reach a desired message frequency.
        // delays will be harmonic frequencies that double at each drop in priority
        // 50Hz -- 1/50 = 0.02s -- 0.02 * 1000 * 1000 * 1000 = 20_000_000ns
        use MsgPri::*;
        match self {
            Immediate   =>           0, // send this immediately
            VeryHigh    =>  20_000_000, // 50Hz
            High        =>  40_000_000, // 25Hz
            Medium      =>  80_000_000, // 12.5Hz
            Low         => 160_000_000, // 6.25Hz
            VeryLow     => 320_000_000, // 3.125Hz
            Lowest      => 640_000_000, // 1.5625Hz
            _ => 0, // /infty Hz
        }
    }
}

#[allow(unused)]
#[derive(Clone, Debug, Copy, PartialEq)]
pub enum AttackType {
    Benign = 0,
    AtkCollisionAttackAgainstTheBus = 1,
    AtkCollisionAttackAgainstAnRT = 2,
    AtkDataThrashingAgainstRT = 3,
    AtkMITMAttackOnRTs = 4,
    AtkShutdownAttackRT = 5,
    AtkFakeStatusReccmd = 6,
    AtkFakeStatusTrcmd = 7,
    AtkDesynchronizationAttackOnRT = 8,
    AtkDataCorruptionAttack = 9,
    AtkCommandInvalidationAttack = 10,
}

