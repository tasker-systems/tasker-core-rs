//! # Service Layer Tests (TAS-76)
//!
//! Integration tests for the extracted service layer, validating that
//! TaskService, StepService, and HealthService work correctly with
//! real database pools and task templates.

mod health_service_tests;
mod step_service_tests;
mod task_service_tests;
