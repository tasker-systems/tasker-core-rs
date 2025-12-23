//! TypeScript Worker E2E Tests
//!
//! End-to-end tests for TypeScript handler implementations through the orchestration API.
//!
//! These tests validate that TypeScript handlers work correctly in the distributed system
//! by calling the orchestration API and verifying task/step states.
//!
//! ## Test Coverage
//!
//! | Module                      | Tests | Description                                    |
//! |-----------------------------|-------|------------------------------------------------|
//! | batch_processing_test       | 2     | CSV batch processing with parallel workers     |
//! | conditional_approval_test   | 5     | Decision point routing based on amount         |
//! | diamond_workflow_test       | 3     | Parallel branches with convergence             |
//! | domain_event_publishing_test| 3     | Domain event fire-and-forget verification      |
//! | error_scenarios_test        | 3     | Success, permanent, and retryable errors       |
//! | linear_workflow_test        | 3     | Sequential step execution                      |
//!
//! ## Prerequisites
//!
//! Tests require the TypeScript worker to be running on port 8084.
//! Use docker-compose -f docker/docker-compose.test.yml to start all services.
//!
//! ## Running Tests
//!
//! ```bash
//! # Run all TypeScript E2E tests
//! cargo test --test e2e typescript
//!
//! # Run specific test module
//! cargo test --test e2e typescript::diamond_workflow_test
//!
//! # Run with output
//! cargo test --test e2e typescript -- --nocapture
//! ```

mod batch_processing_test;
mod conditional_approval_test;
mod diamond_workflow_test;
mod domain_event_publishing_test;
mod error_scenarios_test;
mod linear_workflow_test;
