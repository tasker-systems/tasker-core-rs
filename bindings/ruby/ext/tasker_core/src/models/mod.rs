//! # Ruby Model Wrappers
//!
//! Magnus-wrapped Ruby classes that provide type-safe access to Rust models
//! from Ruby code. These classes use `free_immediately` for memory safety
//! since they represent read-only snapshots from the database.

pub mod ruby_task;
pub mod ruby_step;
pub mod ruby_step_sequence;

pub use ruby_task::RubyTask;
pub use ruby_step::RubyStep;
pub use ruby_step_sequence::RubyStepSequence;
