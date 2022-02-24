fn main() {
    let connection = sqlite::open(":memory:").unwrap();

    connection
        .execute(
            "
            CREATE TABLE users (Time INTEGER, SensorName TEXT, Value INTEGER);
            ",
        )
        .unwrap();
}