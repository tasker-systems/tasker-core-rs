//! # Domain Event Workflow Integration Tests
//!
//! TAS-65: Integration tests for domain event publishing workflow.
//!
//! These tests validate the orchestration layer integration for workflows
//! that declare domain events. They verify:
//!
//! - Task initialization with event-declaring steps
//! - Step dependency resolution (linear 4-step chain)
//! - Step execution flow and state transitions
//! - Failure path handling and retry eligibility
//! - Event publication condition evaluation
//!
//! ## Note on Event Publishing
//!
//! Actual event publishing occurs at the worker level (post-step-execution).
//! E2E tests in `tests/e2e/ruby/domain_event_publishing_test.rs` validate
//! the full event publishing flow. These integration tests focus on the
//! orchestration logic that enables event publishing.
//!
//! ## Test Organization
//!
//! - `happy_path.rs`: Success scenarios where all steps complete
//! - `failure_path.rs`: Failure scenarios testing error states and retry logic

mod failure_path;
mod happy_path;
