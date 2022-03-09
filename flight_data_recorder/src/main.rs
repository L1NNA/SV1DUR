// vim: cc=100

use simconnect;
use sqlite;
use std::time::Duration;
use std::thread::sleep;
use std::mem::transmute_copy;

// SimConnect Aliases
type ScType = simconnect::SIMCONNECT_DATATYPE;
type ScPeriod = simconnect::SIMCONNECT_PERIOD;

const FLOAT64: ScType =
    simconnect::SIMCONNECT_DATATYPE_SIMCONNECT_DATATYPE_FLOAT64;

const INT64: ScType =
    simconnect::SIMCONNECT_DATATYPE_SIMCONNECT_DATATYPE_INT64;

const SIM_FRAME: ScPeriod =
    simconnect::SIMCONNECT_PERIOD_SIMCONNECT_PERIOD_SIM_FRAME;

const SECOND: ScPeriod =
    simconnect::SIMCONNECT_PERIOD_SIMCONNECT_PERIOD_SECOND;

type Boolean = bool;
type Integer = i64;
type Float = f64;

#[repr(u32)]
enum SystemEvent {
    Sim,
    Paused,
}

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
    Bool: INT64 = "Bool",

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
        concat!(":", stringify!($name))
    };

    ( @insert $name:ident , $($names:ident),+ ) => {
        concat! {
            define_sensors!(@insert $name), ", ",
            define_sensors!(@insert $($names),+)
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
                $( ( $datum_name, $datum_type ) ),+
            ];

            const SQL_CREATE_TABLE_STATEMENT: &'static str = concat! {
                "create table sensor_data (",
                    define_sensors!(@column $( $name $type ),+),
                ")"
            };

            const SQL_INSERT_STATEMENT: &'static str = concat! {
                "insert into sensor_data values (",
                    define_sensors!(@insert $( $name ),+),
                ")"
            };

            #[inline(always)]
            fn persist(&self, statement: &mut sqlite::Statement) -> sqlite::Result<()> {
                $(
                    statement.bind_by_name(
                        define_sensors!(@insert $name), self.$name
                    )?;
                )+
                Ok(())
            }
        }
    };
}

define_sensors! {
    // TIME
    absolute_time <- "ABSOLUTE TIME" in Seconds as Integer;

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

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn main() -> Result<()> {
    let mut sim_conn = simconnect::SimConnector::new();

    if ! sim_conn.connect("SV1DUR Flight Data Recorder") {
        return Err("Failed to connect to flight simulator");
    }

    let dirname = chrono::Utc::now().format("%F-%H-%M-%S-%f").to_string();
    std::fs::create_dir(std::path::PathBuf::from(&dirname))?;

    let mut conn = sqlite::open(format!("{}/flight_data.sqlite", dirname))?;
    conn.execute(SensorData::SQL_CREATE_TABLE_STATEMENT)?;

    let mut stmt = conn.prepare(SensorData::SQL_INSERT_STATEMENT)?;

    for (key, unit) in SensorData::SENSORS {
        if ! sim_conn.add_data_definition(0, key, unit.to_str(), unit.to_type(), u32::MAX) {
            panic!("Invalid key: {}", key);
        }
    }

    sim_conn.request_data_on_sim_object(0, 0, 0, SECOND, 0, 0, 0, 0);
    sim_conn.subscribe_to_system_events(SystemEvent::Sim as u32, "Sim");
    sim_conn.subscribe_to_system_events(SystemEvent::Paused as u32, "Paused");

    let mut paused = true;
    println!("-- -- -- -- -- -- -- --");

    loop {
        use simconnect::DispatchResult::*;
        match sim_conn.get_next_message() {
            Ok(SimobjectData(data)) => {
                if paused {
                    continue;
                }

                unsafe {
                    if data.dwDefineID == 0 {
                        let sim_data: SensorData = transmute_copy(&data.dwData);
                        sim_data.persist(&stmt);
                    }
                }
            },
            Ok(Event(event)) => {
                unsafe {
                    use SystemEvent::*;
                    match event.uEventID {
                        Sim => {
                            paused = event.dwData == 0;

                            if paused {
                                println!("Simulation has stopped.");
                                break;
                            }
                        },
                        Paused => {
                            paused = event.dwData == 1;
                        },
                        _ => ()
                    }
                }
            },
            Ok(Quit(_)) => {
                println!("Flight Simulator has exited.");
                break;
            },
            _ => ()
        }

        sleep(Duration::from_millis(16));
    }

    Ok(())
}
