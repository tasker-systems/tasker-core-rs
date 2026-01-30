//! Model Tests Module
//!
//! This module contains all model tests using SQLx native testing
//! for automatic database isolation.

#[path = "models/core/mod.rs"]
pub mod core;
#[path = "models/insights/mod.rs"]
pub mod insights;
#[path = "models/orchestration/mod.rs"]
pub mod orchestration;
