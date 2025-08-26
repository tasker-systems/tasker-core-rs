//! # Shared Test Utilities Module
//!
//! Common utilities for workspace-level integration tests.

pub mod test_utilities;

// Re-export commonly used items
pub use test_utilities::{IntegrationTestFoundation, IntegrationTestUtils};