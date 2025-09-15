//! # Tasker Core
//!
//! **Tasker Core** provides essential testing infrastructure and database utilities
//! for the Tasker workflow orchestration system. This crate is primarily focused on
//! integration testing support and shared testing utilities across the Tasker ecosystem.
//!
//! ## Features
//!
//! - **Database Testing**: Simplified PostgreSQL test database setup and management
//! - **Integration Testing**: Comprehensive integration test infrastructure with Docker support
//! - **Environment Management**: Test environment configuration and validation
//! - **Migration Support**: Database schema migrations for testing scenarios
//!
//! ## Quick Start
//!
//! The most common use case is setting up a test database for integration tests:
//!
//! ```ignore
//! use tasker_core::{setup_test_db, setup_test_environment};
//! use sqlx::PgPool;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Set up test environment and get database pool
//! setup_test_environment();
//! let pool: PgPool = setup_test_db().await;
//!
//! // Use pool for testing...
//! let row: (i32,) = sqlx::query_as("SELECT 1")
//!     .fetch_one(&pool)
//!     .await?;
//! assert_eq!(row.0, 1);
//! # Ok(())
//! # }
//! ```
//!
//! ## Architecture
//!
//! The crate is organized into focused modules:
//!
//! - [`test_helpers`] - Core testing utilities and database setup
//! - [`test_helpers::integration_test_manager`] - Docker-based integration testing
//! - [`test_helpers::integration_test_utils`] - Task creation and workflow testing utilities
//!
//! ## Environment Variables
//!
//! The following environment variables control test behavior:
//!
//! - `DATABASE_URL` - PostgreSQL connection string for tests
//! - `TASKER_ENV` - Environment name (test, development, production)
//! - `TEST_ALLOW_DESTRUCTIVE` - Allow destructive test operations
//!
//! ## Examples
//!
//! ### Basic Database Testing
//!
//! ```ignore
//! use tasker_core::{get_test_database_url, setup_test_db};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Get test database URL
//! let db_url = get_test_database_url();
//! println!("Testing with database: {}", db_url);
//!
//! // Create configured test database pool
//! let pool = setup_test_db().await;
//!
//! // Pool is ready for testing...
//! # Ok(())
//! # }
//! ```
//!
//! ### Integration Test Management
//!
//! ```ignore
//! use tasker_core::test_helpers::{IntegrationTestManager, IntegrationConfig};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = IntegrationConfig {
//!     orchestration_url: "http://localhost:8080".to_string(),
//!     worker_url: Some("http://localhost:8081".to_string()),
//!     skip_health_check: false,
//!     health_timeout_seconds: 30,
//!     health_retry_interval_seconds: 2,
//! };
//!
//! let manager = IntegrationTestManager::setup_with_config(config).await?;
//!
//! // Use manager for comprehensive integration testing
//! let orchestration_url = &manager.orchestration_url;
//! # Ok(())
//! # }
//! ```

pub mod test_helpers;

pub use test_helpers::{
    get_test_database_url, setup_test_database_url, setup_test_db, setup_test_environment, MIGRATOR,
};
