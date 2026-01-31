//! CLI module for the Tasker CLI tool
//!
//! This module organizes all CLI-related functionality including
//! command structures and their handlers.

pub mod commands;

pub use commands::{
    handle_auth_command, handle_config_command, handle_dlq_command, handle_docs_command,
    handle_system_command, handle_task_command, handle_worker_command,
};
