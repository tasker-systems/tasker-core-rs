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
//!

pub mod messaging;
pub mod state_manager;
pub mod task_initialization_cycle_detection_test;
pub mod web;
