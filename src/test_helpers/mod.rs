// Test Helpers Module - Integration Testing Infrastructure
//
// Provides modern testing utilities for Rust integration tests using testcontainers-rs.

pub mod test_utils;
pub mod testcontainers;
pub mod integration_test_suite;

pub use test_utils::{
    get_test_database_url, setup_test_database_url, setup_test_db, setup_test_environment, MIGRATOR,
};

// Re-export main testcontainers types for convenience
pub use testcontainers::{
    BuildMode, PgmqPostgres, OrchestrationService, WorkerService,
};

// Re-export integration test suite types for convenience
pub use integration_test_suite::{
    IntegrationTestSuite, ApiOnlyTestSuite,
};
