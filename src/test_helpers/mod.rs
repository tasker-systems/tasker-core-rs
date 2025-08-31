// Test Helpers Module - Integration Testing Infrastructure
//
// Provides shared testing utilities and setup functions for Rust integration tests.
// This module mirrors the Ruby SharedTestLoop functionality but is adapted for
// native Rust integration testing patterns.

pub mod shared_test_setup;
pub mod test_utils;

pub use shared_test_setup::{
    create_business_test_context, create_mathematical_test_context, create_test_task_request,
    SharedTestSetup, TaskExecutionSummary,
};

pub use test_utils::{
    get_test_database_url, setup_test_database_url, setup_test_db, setup_test_environment, MIGRATOR,
};
