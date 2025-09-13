// Test Helpers Module - Integration Testing Infrastructure
//
// Provides modern testing utilities for Rust integration tests.
// Features Docker Compose integration for simplified testing.

pub mod integration_test_manager;
pub mod integration_test_utils;
pub mod test_utils;

pub use test_utils::{
    get_test_database_url, setup_test_database_url, setup_test_db, setup_test_environment, MIGRATOR,
};

// Primary integration test manager
pub use integration_test_manager::{ApiOnlyManager, IntegrationConfig, IntegrationTestManager};
pub use integration_test_utils::{create_task_request, wait_for_task_completion};
