//! Orchestration Tests Module
//!
//! External integration tests for the tasker-orchestration crate.
//! These tests verify orchestration-specific functionality including:
//!
//! - Command pattern integration
//! - Messaging and queue integration
//! - Web API endpoints
//! - State management for orchestration
//! - Task initialization and cycle detection
//! - Service layer (TAS-76)
#[path = "messaging/mod.rs"]
pub mod messaging;

#[path = "services/mod.rs"]
pub mod services;

#[path = "web/mod.rs"]
pub mod web;
