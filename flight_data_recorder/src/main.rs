// vim: cc=100

use simconnect;
use sqlite;
use std::time::{Duration, Instant};
use std::thread::sleep;
use std::mem::transmute_copy;
use std::io::{Error, ErrorKind};
use std::io::prelude::*;

use winapi::um::processthreadsapi::{GetCurrentThread, SetThreadPriority};
use winapi::um::winbase::THREAD_PRIORITY_TIME_CRITICAL;

// SimConnect Aliases
type ScType = simconnect::SIMCONNECT_DATATYPE;
type ScPeriod = simconnect::SIMCONNECT_PERIOD;

const FLOAT64: ScType =
    simconnect::SIMCONNECT_DATATYPE_SIMCONNECT_DATATYPE_FLOAT64;

const INT64: ScType =
    simconnect::SIMCONNECT_DATATYPE_SIMCONNECT_DATATYPE_INT64;

#[allow(unused)]
const SIM_FRAME: ScPeriod =
    simconnect::SIMCONNECT_PERIOD_SIMCONNECT_PERIOD_SIM_FRAME;

#[allow(unused)]
const SECOND: ScPeriod =
    simconnect::SIMCONNECT_PERIOD_SIMCONNECT_PERIOD_SECOND;

#[allow(unused)]
type Boolean = bool;
type Integer = i64;
type Float = f64;

// Arbitrary IDs for Events
//const SYSTEM_EVENT_ID_SIM_START: u32 = 1;
const SYSTEM_EVENT_ID_SIM_STOP: u32 = 2;
//const SYSTEM_EVENT_ID_UNPAUSE: u32 = 3;
//const SYSTEM_EVENT_ID_PAUSE: u32 = 4;

// Unit Enumerations
macro_rules! define_units {
    ( $( $unit:ident : $type:ident = $string:literal ),+ , ) => {

        #[allow(unused)]
        #[derive(Debug)]
        enum Unit {
            $( $unit ),+
        }

        impl Unit {
            fn to_str(&self) -> &str {
                use Unit::*;
                match self {
                    $( $unit => $string ),+
                }
            }

            fn to_type(&self) -> ScType {
                use Unit::*;
                match self {
                    $( $unit => $type ),+
                }
            }
        }

        use Unit::*;
    }

}

define_units! {
    Bool: INT64 = "bool",
    Number: FLOAT64 = "number",

    Hours: INT64 = "hours",
    Minutes: INT64 = "minutes",
    Seconds: INT64 = "seconds",

    Radians: FLOAT64 = "radians",
    Degrees: FLOAT64 = "degrees",

    Celsius: FLOAT64 = "celsius",

    Knots: FLOAT64 = "knots",

    Meters: FLOAT64 = "meters",
    MetersPerSecond: FLOAT64 = "meters/second",

    Feet: FLOAT64 = "ft",
    FeetPerMinute: FLOAT64 = "feet/minute",
    FeetPerSecond: FLOAT64 = "feet/second",
    FeetPerSecondSquared: FLOAT64 = "feet per second squared",

    SlugsPerCubicFoot: FLOAT64 = "slugs/ft3",

    Pounds: FLOAT64 = "lbs",
    PoundsPerHour: FLOAT64 = "pounds per hour",

    Gallons: FLOAT64 = "gallons",

    // Apparently there is a difference between these units
    Position: FLOAT64 = "position", // Equivalent to "Percent over 100"
    Position16k: INT64 = "position 16K", // 16 bit int
    Position32k: INT64 = "position 32K", // 32 bit int
    Position128: INT64 = "position 128", // 0 to 128 (not in prepar3d docs)

    Percent: FLOAT64 = "percent", // -100 to 100
    PercentOver100: FLOAT64 = "percent over 100", // -1.0 to 1.0
}

macro_rules! define_sensors {
    ( @1 $_:tt ) => (1);

    ( @column $name:ident $type:ty ) => {
        concat!(stringify!($name), " ", stringify!($type))
    };

    ( @column $name:ident $type:ty , $($tail:tt)+ ) => {
        concat! {
            define_sensors!(@column $name $type), ", ",
            define_sensors!(@column $($tail)+)
        }
    };

    ( @insert $name:ident ) => {
        stringify!($name)
    };

    ( @insert $name:ident , $($names:ident),+ ) => {
        concat! {
            define_sensors!(@insert $name), ", ",
            define_sensors!(@insert $($names),+)
        }
    };

    ( @value $name:ident ) => {
        concat!(":", stringify!($name))
    };

    ( @value $name:ident , $($names:ident),+ ) => {
        concat! {
            define_sensors!(@value $name), ", ",
            define_sensors!(@value $($names),+)
        }
    };

    ( $( $name:ident
         <- $datum_name:literal
         in $datum_type:ident
         as $type:ty
      );+; ) => {

        #[allow(unused)]
        #[derive(Debug)]
        struct SensorData {
            $( $name : $type ),+
        }

        impl SensorData {
            const SENSORS: [(&'static str, Unit); 0 $(+ define_sensors!(@1 $name))+] = [
                $( ($datum_name, $datum_type) ),+
            ];

            const SQL_CREATE_TABLE_STATEMENT: &'static str = concat! {
                "create table sensor_data (",
                    "elapsed_ms Integer, ",
                    "delta_ms Integer, ",
                    define_sensors!(@column $( $name $type ),+),
                ")"
            };

            const SQL_INSERT_STATEMENT: &'static str = concat! {
                "insert into sensor_data (",
                    "elapsed_ms,",
                    "delta_ms,",
                    define_sensors!(@insert $($name),+),
                ") values (",
                    ":elapsed_ms,",
                    ":delta_ms,",
                    define_sensors!(@value $($name),+),
                ")"
            };

            #[inline(always)]
            fn persist(&self, stmt: &mut sqlite::Statement, elapsed: Duration, delta: Duration)
                    -> sqlite::Result<()> {

                stmt.bind_by_name(":elapsed_ms", elapsed.as_millis() as i64)?;
                stmt.bind_by_name(":delta_ms", delta.as_millis() as i64)?;
                $(stmt.bind_by_name(define_sensors!(@value $name), self.$name)?;)+
                stmt.next()?;
                stmt.reset()?;
                Ok(())
            }
        }
    };
}

define_sensors! {
    // TIME
    absolute_time <- "ABSOLUTE TIME" in Seconds as Integer;

    // CONTROL INFORMATION
    yoke_x_position <- "YOKE X POSITION" in Position as Float;
    yoke_y_position <- "YOKE Y POSITION" in Position as Float;

    yoke_x_position_ap <- "YOKE X POSITION WITH AP" in Position as Float;
    yoke_y_position_ap <- "YOKE Y POSITION WITH AP" in Position as Float;

    yoke_x_indicator <- "YOKE X INDICATOR" in Position as Float;
    yoke_y_indicator <- "YOKE Y INDICATOR" in Position as Float;

    rudder_position <- "RUDDER POSITION" in Position as Float;
    rudder_pedal_position <- "RUDDER PEDAL POSITION" in Position as Float;
    rudder_pedal_indicator <- "RUDDER PEDAL INDICATOR" in Position as Float;

    brake_right_position <- "BRAKE RIGHT POSITION" in Position as Float;
    brake_left_position <- "BRAKE LEFT POSITION" in Position as Float;

    throttle_level_position1 <- "GENERAL ENG THROTTLE LEVER POSITION:1" in Percent as Float;
    throttle_level_position2 <- "GENERAL ENG THROTTLE LEVER POSITION:2" in Percent as Float;

    spoiler_handle_position <- "SPOILERS HANDLE POSITION" in Position as Float;
    spoiler_right_position <- "SPOILERS RIGHT POSITION" in Percent as Float;
    spoiler_left_position <- "SPOILERS LEFT POSITION" in Percent as Float;

    flaps_handle_index <- "FLAPS HANDLE INDEX" in Number as Float;
    flaps_num_handle_positions <- "FLAPS NUM HANDLE POSITIONS" in Number as Float;

    // INSTRUMENT CLUSTER
    indicated_airspeed <- "AIRSPEED INDICATED" in Knots as Float;
    indicated_altitude <- "INDICATED ALTITUDE" in Feet as Float;
    vertical_speed <- "VERTICAL SPEED" in FeetPerMinute as Float;
    heading_indicator <- "HEADING INDICATOR" in Radians as Float;
    plane_heading_gyro <- "PLANE HEADING DEGREES GYRO" in Radians as Float;
    wiskey_compass_indicaton <- "WISKEY COMPASS INDICATION DEGREES" in Degrees as Float;
    angle_of_attack_indicator <- "ANGLE OF ATTACK INDICATOR" in Radians as Float;

    fuel_total_quantity <- "FUEL TOTAL QUANTITY" in Gallons as Float;
    estimated_fuel_flow <- "ESTIMATED FUEL FLOW" in PoundsPerHour as Float;

    // SPEED DATA (WORLD)
    ground_velocity <- "GROUND VELOCITY" in Knots as Float;
    total_world_velocity <- "TOTAL WORLD VELOCITY" in FeetPerSecond as Float;

    velocity_world_x <- "VELOCITY WORLD X" in FeetPerSecond as Float;
    velocity_world_y <- "VELOCITY WORLD Y" in FeetPerSecond as Float;
    velocity_world_z <- "VELOCITY WORLD Z" in FeetPerSecond as Float;

    acceleration_world_x <- "ACCELERATION WORLD X" in FeetPerSecondSquared as Float;
    acceleration_world_y <- "ACCELERATION WORLD Y" in FeetPerSecondSquared as Float;
    acceleration_world_z <- "ACCELERATION WORLD Z" in FeetPerSecondSquared as Float;

    // SPEED DATA (PLANE)
    velocity_plane_x <- "VELOCITY BODY X" in FeetPerSecond as Float;
    velocity_plane_y <- "VELOCITY BODY Y" in FeetPerSecond as Float;
    velocity_plane_z <- "VELOCITY BODY Z" in FeetPerSecond as Float;

    acceleration_plane_x <- "ACCELERATION BODY X" in FeetPerSecondSquared as Float;
    acceleration_plane_y <- "ACCELERATION BODY Y" in FeetPerSecondSquared as Float;
    acceleration_plane_z <- "ACCELERATION BODY Z" in FeetPerSecondSquared as Float;

    // ANGLE OF ATTACK
    plane_pitch <- "PLANE PITCH DEGREES" in  Radians as Float; // "Degrees"
    plane_bank <- "PLANE BANK DEGREES" in Radians as Float; // "Degrees"

    incidence_alpha <- "INCIDENCE ALPHA" in Radians as Float; // AoA
    incidence_beta <- "INCIDENCE BETA" in Radians as Float; // Sideslip

    // GPS DATA
    gps_latitude <- "GPS POSITION LAT" in Degrees as Float;
    gps_longitude <- "GPS POSITION LON" in Degrees as Float;
    gps_altitude <- "GPS POSITION ALT" in Meters as Float;

    plane_latitude <- "PLANE LATITUDE" in Degrees as Float;
    plane_longitude <- "PLANE LONGITUDE" in Degrees as Float;
    plane_altitude <- "PLANE ALTITUDE" in Feet as Float;

    // WEATHER DATA
    ambient_temperature <- "AMBIENT TEMPERATURE" in Celsius as Float;
    ambient_pressure <- "AMBIENT PRESSURE" in SlugsPerCubicFoot as Float;
    ambient_wind_velocity <- "AMBIENT WIND VELOCITY" in Knots as Float;
    ambient_wind_direction <- "AMBIENT WIND DIRECTION" in Degrees as Float;
    ambient_wind_x <- "AMBIENT WIND X" in MetersPerSecond as Float;
    ambient_wind_y <- "AMBIENT WIND Y" in MetersPerSecond as Float;
    ambient_wind_z <- "AMBIENT WIND Z" in MetersPerSecond as Float;
    total_air_temperature <- "TOTAL AIR TEMPERATURE" in Celsius as Float;
}

struct Spinner {
    counter: u8,
    divider: u8,
    index: usize
}

impl Spinner {
    const CHARS: [char; 4] = [ '|', '/', '-', '\\' ];

    fn new(divider: u8) -> Self {
        Spinner {
            counter: 0,
            divider: divider,
            index: 0
        }
    }

    fn step(&mut self) {
        self.counter = (self.counter + 1) % self.divider;
        if self.counter == 0 {
            self.index = (self.index + 1) % 4;
            print!("\u{08}{}", Spinner::CHARS[self.index]);
            std::io::stdout().flush().unwrap();
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut sim_conn = simconnect::SimConnector::new();

    if ! sim_conn.connect("SV1DUR Flight Data Recorder") {
        return Err(Box::new(Error::new(ErrorKind::Other, "Failed to connect to flight simulator")));
    }

    let time = chrono::Utc::now().format("%F-%H-%M-%S");
    let conn = sqlite::open(format!("flight_data_{}.sqlite", time))?;
    conn.execute(SensorData::SQL_CREATE_TABLE_STATEMENT)?;

    let mut stmt = conn.prepare(SensorData::SQL_INSERT_STATEMENT)?;

    for (key, unit) in SensorData::SENSORS {
        if ! sim_conn.add_data_definition(0, key, unit.to_str(), unit.to_type(), u32::MAX) {
            panic!("Invalid key: {}", key);
        }
    }

    sim_conn.request_data_on_sim_object(0, 0, 0, SIM_FRAME, 0, 0, 0, 0);
    //sim_conn.subscribe_to_system_event(SYSTEM_EVENT_ID_SIM_START, "SimStart");
    sim_conn.subscribe_to_system_event(SYSTEM_EVENT_ID_SIM_STOP, "SimStop");
    //sim_conn.subscribe_to_system_event(SYSTEM_EVENT_ID_UNPAUSE, "Unpaused");
    //sim_conn.subscribe_to_system_event(SYSTEM_EVENT_ID_PAUSE, "Paused");

    unsafe {
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_TIME_CRITICAL as i32);
    }

    println!("Recording in progress. Exit or stop simulator to quit.");
    let mut spinner = Spinner::new(8);

    let start_instant = Instant::now();
    let mut delta_instant = Instant::now();

    loop {
        use simconnect::DispatchResult::*;
        match sim_conn.get_next_message() {
            Ok(SimobjectData(data)) => {
                unsafe {
                    if data.dwDefineID == 0 {
                        let sim_data: SensorData = transmute_copy(&data.dwData);
                        sim_data.persist(&mut stmt, start_instant.elapsed(), delta_instant.elapsed())?;
                    }
                }

                delta_instant = Instant::now();
            },
            Ok(Event(event)) => {
                match event.uEventID {
                    SYSTEM_EVENT_ID_SIM_STOP => {
                        println!("Simulation stopped. Exiting.");
                        break;
                    }
                    _ => ()
                }
            },
            Ok(Quit(_)) => {
                println!("Flight Simulator has exited.");
                break;
            },
            _ => ()
        }

        spinner.step();
        sleep(Duration::from_millis(2));
    }

    Ok(())
}
