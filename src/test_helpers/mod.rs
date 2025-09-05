// Test Helpers Module - Docker Integration Testing Infrastructure
//
// Provides shared Docker-based testing utilities for Rust integration tests.
// Uses shared service pattern for fast, efficient testing.

pub mod docker_test_suite_manager;
pub mod test_utils;

// Docker Test Suite Manager - shared services across tests (recommended)
pub use docker_test_suite_manager::{
    cleanup_shared_services, create_mathematical_test_context,
    create_order_fulfillment_test_context, create_test_task_request, DockerTestClient,
    DockerTestResult, DockerTestSuiteManager,
};

pub use test_utils::{
    get_test_database_url, setup_test_database_url, setup_test_db, setup_test_environment, MIGRATOR,
};
