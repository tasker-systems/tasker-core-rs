//! Integration Tests for Tasker Core
//!
//! This module contains all integration tests using SQLx's native testing facilities.
//! Each test gets its own isolated database instance with automatic cleanup.

mod config;
mod database;
mod models;
mod state_machine;
