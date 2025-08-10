//! Integration Tests for Tasker Core
//!
//! This module contains all integration tests using SQLx's native testing facilities.
//! Each test gets its own isolated database instance with automatic cleanup.

mod common;
mod config;
mod database;
mod factories;
// mod ffi;
mod models;
mod orchestration;
// mod sql_functions;
// mod state_machine;
