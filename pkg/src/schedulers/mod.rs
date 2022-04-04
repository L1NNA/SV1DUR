mod bus_controller;
mod generic;
mod fighter;


pub use generic::{Proto, Scheduler, DefaultScheduler, EmptyScheduler};
pub use fighter::{FighterBCScheduler, eval_fighter_sim};