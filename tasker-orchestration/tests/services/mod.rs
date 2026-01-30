//! # Service Layer Tests (TAS-76, TAS-63)
//!
//! Integration tests for the extracted service layer, validating that
//! services work correctly with real database pools and task templates.
//!
//! TAS-63: Added query service and analytics service integration tests.

mod analytics_service_tests;
mod health_service_tests;
mod step_query_service_tests;
mod step_service_tests;
mod task_query_service_tests;
mod task_service_tests;
mod template_query_service_tests;
