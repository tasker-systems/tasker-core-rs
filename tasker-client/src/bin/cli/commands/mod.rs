//! Command handlers for the Tasker CLI
//!
//! This module contains all command handler implementations, decomposed by command category.

pub mod auth;
pub mod config;
pub mod dlq;
pub mod system;
pub mod task;
pub mod worker;

pub use auth::handle_auth_command;
pub use config::handle_config_command;
pub use dlq::handle_dlq_command;
pub use system::handle_system_command;
pub use task::handle_task_command;
pub use worker::handle_worker_command;
