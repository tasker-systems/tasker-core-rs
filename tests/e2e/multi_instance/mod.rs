//! # TAS-73: Multi-Instance Integration Tests
//!
//! Tests that validate concurrent behavior across multiple orchestration
//! and worker instances. These tests require the multi-instance cluster
//! to be running.
//!
//! ## Prerequisites
//!
//! Start the cluster before running these tests:
//! ```bash
//! cargo make cluster-start
//! ```
//!
//! Or configure custom URLs via environment:
//! ```bash
//! TASKER_TEST_ORCHESTRATION_URLS=http://localhost:8080,http://localhost:8081
//! TASKER_TEST_WORKER_URLS=http://localhost:8100,http://localhost:8101
//! ```
//!
//! ## Test Categories
//!
//! - **Concurrent Task Creation**: Validates task creation across instances
//! - **Step Contention**: Validates atomic step claiming under load
//! - **Consistency**: Validates state consistency across all instances

mod concurrent_task_creation_test;
mod consistency_test;
