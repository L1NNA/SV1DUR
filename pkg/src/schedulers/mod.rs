pub mod bus_controller;
// use bus_controller::{Address, HercScheduler, FighterScheduler};
pub mod generic;
pub use generic::{Proto, Scheduler, DefaultScheduler, EmptyScheduler};
pub use bus_controller::{FighterScheduler, HercScheduler};