//! # TAS-73: Multi-Instance Integration Tests
//!
//! Tests that validate concurrent behavior across multiple orchestration
//! and worker instances. These tests require the multi-instance cluster
//! to be running.
//!
//! **Feature gate**: `test-cluster` (implies `test-services`)
//!
//! **NOT run in CI** - GitHub Actions resource constraints prevent running
//! multiple orchestration + worker instances. Run locally only.
//!
//! ## Prerequisites
//!
//! Start the cluster before running these tests:
//! ```bash
//! cargo make cluster-start
//! cargo make test-rust-cluster  # or: cargo make tc
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

#![cfg(feature = "test-cluster")]

mod concurrent_task_creation_test;
mod consistency_test;
