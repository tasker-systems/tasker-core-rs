//! # Modern testcontainers Integration Testing Infrastructure
//!
//! This module provides a complete replacement for the previous naive Docker integration
//! testing approach. It uses the industry-standard `testcontainers-rs` library to provide:
//!
//! ## Key Features
//!
//! - **Automatic Container Lifecycle**: No manual cleanup required
//! - **Container Isolation**: Each test gets isolated containers  
//! - **Three-Tier Build Strategy**: Fast/CI/Production build modes
//! - **PostgreSQL with PGMQ**: Pre-configured database with extensions
//! - **Service Integration**: Orchestration and Worker services
//! - **Dynamic Port Allocation**: No port conflicts
//! - **Health Checks**: Built-in service readiness validation
//!
//! ## Architecture
//!
//! ```text
//! Individual Container Services
//! ├── PgmqPostgres           # PostgreSQL + PGMQ extension
//! ├── OrchestrationService   # Orchestration API service  
//! └── WorkerService          # Worker service instances
//! ```
//!
//! ## Usage Patterns
//!
//! ### PostgreSQL Database Container
//!
//! ```rust
//! use tasker_core::test_helpers::testcontainers::PgmqPostgres;
//!
//! #[tokio::test]
//! #[ignore] // Only run when Docker is available
//! async fn test_postgres_integration() -> anyhow::Result<()> {
//!     let postgres = PgmqPostgres::new()
//!         .with_db_name("test_db")
//!         .with_user("test_user")
//!         .with_password("test_pass");
//!         
//!     let container = postgres.start()?;
//!     let db_url = postgres.connection_string(&container);
//!     
//!     // Test database operations with db_url...
//!     
//!     Ok(())
//!     // Container automatically cleaned up on drop
//! }
//! ```
//!
//! ### Orchestration Service Container
//!
//! ```rust
//! use tasker_core::test_helpers::testcontainers::{OrchestrationService, BuildMode};
//!
//! #[tokio::test] 
//! #[ignore] // Only run when Docker is available
//! async fn test_orchestration_service() -> anyhow::Result<()> {
//!     let database_url = "postgresql://tasker:tasker@localhost:5432/test_db".to_string();
//!     let service = OrchestrationService::new(database_url, BuildMode::Fast);
//!     
//!     let container = service.start()?;
//!     let service_url = service.service_url(&container);
//!     
//!     // Test orchestration API with service_url...
//!     
//!     Ok(())
//! }
//! ```
//!
//! ### Multi-Container Integration
//!
//! ```rust
//! use tasker_core::test_helpers::testcontainers::{
//!     PgmqPostgres, OrchestrationService, WorkerService, BuildMode
//! };
//!
//! #[tokio::test]
//! #[ignore] // Only run when Docker is available
//! async fn test_full_integration() -> anyhow::Result<()> {
//!     // Start PostgreSQL
//!     let postgres = PgmqPostgres::new();
//!     let postgres_container = postgres.start()?;
//!     let database_url = postgres.connection_string(&postgres_container);
//!     
//!     // Start orchestration service
//!     let orchestration = OrchestrationService::new(database_url.clone(), BuildMode::Fast);
//!     let orch_container = orchestration.start()?;
//!     let orch_url = orchestration.service_url(&orch_container);
//!     
//!     // Start worker service
//!     let worker = WorkerService::new(database_url, BuildMode::Fast, "worker-1".to_string())
//!         .with_orchestration_url(&orch_url);
//!     let worker_container = worker.start()?;
//!     
//!     // Run integration tests...
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Build Mode Strategy
//!
//! The module supports three build modes optimized for different scenarios:
//!
//! - **Fast** (default): Debug builds for local development
//! - **CI**: Optimized builds with sccache for CI environments  
//! - **Production**: Full optimization with cargo-chef
//!
//! ```rust
//! use tasker_core::test_helpers::testcontainers::{OrchestrationService, BuildMode};
//!
//! // Create service with specific build mode
//! let database_url = "postgresql://user:pass@localhost:5432/db".to_string();
//! let service = OrchestrationService::new(database_url, BuildMode::CI);
//! ```
//!
//! Control via environment variable for automatic detection:
//!
//! ```bash
//! # Fast local builds (default)
//! cargo test --test integration
//!
//! # CI-optimized builds  
//! TEST_BUILD_MODE=ci cargo test --test integration
//!
//! # Production builds
//! TEST_BUILD_MODE=production cargo test --test integration
//! ```
//!
//! ## Performance Characteristics
//!
//! - **Container Startup**: ~5-10 seconds for full suite (depending on build mode)
//! - **Image Building**: Cached after first build per mode
//! - **Test Isolation**: Complete isolation, parallel-safe
//! - **Resource Usage**: Minimal overhead, automatic cleanup
//!
//! ## Migration from Old Approach
//!
//! This module completely replaces:
//! - `docker_test_suite_manager.rs` (naive 627-line implementation)
//! - `docker-compose.integration.yml` (cached image issues)
//! - Manual container lifecycle management
//!
//! The new approach is:
//! - ✅ More reliable (automatic cleanup)
//! - ✅ More performant (parallel execution)
//! - ✅ More maintainable (industry standard)
//! - ✅ More flexible (multiple configurations)

// ImageFromDockerfile-based containers using centralized docker/build structure
pub mod postgres_container;
pub mod orchestration_container;
pub mod worker_container;
pub mod utils;

// Re-export main types for convenience
pub use orchestration_container::OrchestrationService;
pub use postgres_container::PgmqPostgres;
pub use worker_container::WorkerService;
pub use utils::BuildMode;

// Common result type
pub type TestContainerResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Default timeout for container operations
pub const DEFAULT_CONTAINER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// Default health check timeout
pub const DEFAULT_HEALTH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);