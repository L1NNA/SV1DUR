use simconnect;
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

    ( $( $name:ident <- $datum_name:literal : $datum_type:ident, $type:ty );*; ) => {

        #[allow(unused)]
        #[derive(Debug)]
        struct SensorData {
            $( $name : $type ),*
        }

        const SENSORS: [(&str, Unit); 0 $(+ define_sensors!(@1 $name))*] = [
            $( ( $datum_name, $datum_type ) ),*
        ];
    };
}

define_sensors! {
    // TIME
    absolute_time <- "ABSOLUTE TIME" : Seconds, i64;

    // INSTRUMENT CLUSTER
    indicated_airspeed <- "AIRSPEED INDICATED" : Knots, f64;
    indicated_altitude <- "INDICATED ALTITUDE" : Feet, f64;
    vertical_speed <- "VERTICAL SPEED" : FeetPerMinute, f64;
    heading_indicator <- "HEADING INDICATOR" : Radians, f64;
    plane_heading_gyro <- "PLANE HEADING DEGREES GYRO" : Radians, f64;
    wiskey_compass_indicaton <- "WISKEY COMPASS INDICATION DEGREES" : Degrees, f64;
    angle_of_attack_indicator <- "ANGLE OF ATTACK INDICATOR" : Radians, f64;

    fuel_total_quantity <- "FUEL TOTAL QUANTITY" : Gallons, f64;
    estimated_fuel_flow <- "ESTIMATED FUEL FLOW" : PoundsPerHour, f64;

    // SPEED DATA (WORLD)
    ground_velocity <- "GROUND VELOCITY" : Knots, f64;
    total_world_velocity <- "TOTAL WORLD VELOCITY" : FeetPerSecond, f64;

    velocity_world_x <- "VELOCITY WORLD X" : FeetPerSecond, f64;
    velocity_world_y <- "VELOCITY WORLD Y" : FeetPerSecond, f64;
    velocity_world_z <- "VELOCITY WORLD Z" : FeetPerSecond, f64;

    acceleration_world_x <- "ACCELERATION WORLD X" : FeetPerSecondSquared, f64;
    acceleration_world_y <- "ACCELERATION WORLD Y" : FeetPerSecondSquared, f64;
    acceleration_world_z <- "ACCELERATION WORLD Z" : FeetPerSecondSquared, f64;

    // SPEED DATA (PLANE)
    velocity_plane_x <- "VELOCITY BODY X" : FeetPerSecond, f64;
    velocity_plane_y <- "VELOCITY BODY Y" : FeetPerSecond, f64;
    velocity_plane_z <- "VELOCITY BODY Z" : FeetPerSecond, f64;

    acceleration_plane_x <- "ACCELERATION BODY X" : FeetPerSecondSquared, f64;
    acceleration_plane_y <- "ACCELERATION BODY Y" : FeetPerSecondSquared, f64;
    acceleration_plane_z <- "ACCELERATION BODY Z" : FeetPerSecondSquared, f64;

    // ANGLE OF ATTACK
    plane_pitch <- "PLANE PITCH DEGREES" : Radians, f64; // "Degrees"
    plane_bank <- "PLANE BANK DEGREES" : Radians, f64; // "Degrees"

    incidence_alpha <- "INCIDENCE ALPHA" : Radians, f64; // AoA
    incidence_beta <- "INCIDENCE BETA" : Radians, f64; // Sideslip

    // GPS DATA
    gps_latitude <- "GPS POSITION LAT" : Degrees, f64;
    gps_longitude <- "GPS POSITION LON" : Degrees, f64;
    gps_altitude <- "GPS POSITION ALT" : Meters, f64;

    plane_latitude <- "PLANE LATITUDE" : Degrees, f64;
    plane_longitude <- "PLANE LONGITUDE" : Degrees, f64;
    plane_altitude <- "PLANE ALTITUDE" : Feet, f64;

    // WEATHER DATA
    ambient_temperature <- "AMBIENT TEMPERATURE" : Celsius, f64;
    ambient_pressure <- "AMBIENT PRESSURE" : SlugsPerCubicFoot, f64;
    ambient_wind_velocity <- "AMBIENT WIND VELOCITY" : Knots, f64;
    ambient_wind_direction <- "AMBIENT WIND DIRECTION" : Degrees, f64;
    ambient_wind_x <- "AMBIENT WIND X" : MetersPerSecond, f64;
    ambient_wind_y <- "AMBIENT WIND Y" : MetersPerSecond, f64;
    ambient_wind_z <- "AMBIENT WIND Z" : MetersPerSecond, f64;
    total_air_temperature <- "TOTAL AIR TEMPERATURE" : Celsius, f64;
}


fn main() {
    let mut conn = simconnect::SimConnector::new();

    if ! conn.connect("Demo") {
        panic!("Failed to connect");
    }

    for (key, unit) in SENSORS {
        if ! conn.add_data_definition(0, key, unit.to_str(), unit.to_type(), u32::MAX) {
            panic!("Invalid key: {}", key);
        }
    }

    conn.request_data_on_sim_object(0, 0, 0, SECOND, 0, 0, 0, 0);

    loop {
        match conn.get_next_message() {
            Ok(simconnect::DispatchResult::SimobjectData(data)) => {
                unsafe {
                    if data.dwDefineID == 0 {
                        let sim_data: SensorData = transmute_copy(&data.dwData);
                        //println!("Indicated Airspeed: {:?}", sim_data.indicated_airspeed);
                        println!("{:?}", sim_data);
                    }
                }
            },
            _ => ()
        }

        sleep(Duration::from_millis(16));
    }
}
