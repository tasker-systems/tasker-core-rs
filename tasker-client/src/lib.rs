//! # Tasker Client Library
//!
//! **Comprehensive client library** for interacting with Tasker orchestration and worker APIs.
//! Provides both library interfaces for programmatic use and CLI commands for manual operations.
//!
//! This crate serves as the primary interface for external systems to interact with the
//! Tasker workflow orchestration platform. It handles HTTP communication, authentication,
//! error handling, and provides strongly-typed interfaces for all API endpoints.
//!
//! ## Features
//!
//! - **Orchestration API Client**: Full-featured client for task management, analytics, and monitoring
//! - **Worker API Client**: Interface for worker health checks and management operations
//! - **Authentication Support**: Built-in API key and JWT token authentication
//! - **Configuration Management**: Flexible configuration loading from environment or files
//! - **Error Handling**: Comprehensive error types with proper categorization
//! - **CLI Integration**: Command-line interface for operational tasks
//!
//! ## Quick Start
//!
//! ### Basic Task Creation
//!
//! ```rust,ignore
//! use tasker_client::{OrchestrationApiClient, OrchestrationApiConfig};
//! use tasker_shared::models::core::task_request::TaskRequest;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create client configuration
//!     let config = OrchestrationApiConfig::default();
//!     let client = OrchestrationApiClient::new(config)?;
//!
//!     // Create a new task
//!     let task_request = TaskRequest {
//!         name: "data_processing".to_string(),
//!         namespace: "analytics".to_string(),
//!         version: "1.0.0".to_string(),
//!         context: serde_json::json!({
//!             "input_file": "/data/input.csv",
//!             "output_dir": "/data/processed/"
//!         }),
//!     };
//!
//!     let response = client.create_task(task_request).await?;
//!     println!("Task created: {}", response.task_id);
//!
//!     Ok(())
//! }
//! ```
//!
//! ### Health Monitoring
//!
//! ```rust,ignore
//! use tasker_client::{OrchestrationApiClient, OrchestrationApiConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = OrchestrationApiConfig::default();
//!     let client = OrchestrationApiClient::new(config)?;
//!
//!     // Check orchestration health
//!     let health = client.health_check().await?;
//!     println!("System status: {:?}", health.status);
//!
//!     // Get detailed health information
//!     let detailed_health = client.detailed_health_check().await?;
//!     println!("Active tasks: {}", detailed_health.active_tasks);
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Architecture
//!
//! The library is organized into several modules:
//!
//! - [`api_clients`] - HTTP clients for orchestration and worker APIs
//! - [`config`] - Configuration management and loading utilities
//! - [`error`] - Comprehensive error types and handling
//!
//! ### API Clients
//!
//! - [`OrchestrationApiClient`] - Manages tasks, analytics, and orchestration operations
//! - [`WorkerApiClient`] - Monitors and manages worker instances
//!
//! ## Configuration
//!
//! The library supports multiple configuration methods:
//!
//! 1. **Environment Variables**: Automatic detection and loading
//! 2. **Configuration Files**: TOML-based configuration with environment overrides
//! 3. **Programmatic**: Direct configuration via structs
//!
//! ### Environment Variables
//!
//! - `ORCHESTRATION_URL` - Base URL for orchestration service
//! - `ORCHESTRATION_API_KEY` - API key for authentication
//! - `WORKER_URL` - Base URL for worker service
//! - `CLIENT_TIMEOUT_MS` - Request timeout in milliseconds
//!
//! ## Authentication
//!
//! The library supports multiple authentication methods:
//!
//! - **API Key**: Simple header-based authentication
//! - **JWT Tokens**: Token-based authentication with automatic refresh
//! - **No Authentication**: For development and internal networks
//!
//! ## Error Handling
//!
//! All client methods return [`ClientResult<T>`] which provides structured error handling:
//!
//! ```rust,ignore
//! use tasker_client::{ClientError, ClientResult};
//!
//! async fn handle_errors() -> ClientResult<()> {
//!     match client.create_task(request).await {
//!         Ok(response) => println!("Success: {}", response.task_id),
//!         Err(ClientError::Authentication) => println!("Check your API credentials"),
//!         Err(ClientError::Network(_)) => println!("Network connectivity issue"),
//!         Err(ClientError::Server(msg)) => println!("Server error: {}", msg),
//!         Err(e) => println!("Other error: {}", e),
//!     }
//!     Ok(())
//! }
//! ```

pub mod api_clients;
pub mod config;
pub mod error;
pub mod transport;

#[cfg(feature = "docs-gen")]
pub mod docs;

#[cfg(feature = "grpc")]
pub mod grpc_clients;

// Re-export commonly used types for convenience
pub use api_clients::{
    OrchestrationApiClient, OrchestrationApiConfig, WorkerApiClient, WorkerApiConfig,
};
pub use config::{ClientAuthConfig, ClientAuthMethod, ClientConfig, Transport};
pub use error::{ClientError, ClientResult};

// Re-export unified transport types for orchestration
#[cfg(feature = "grpc")]
pub use transport::GrpcOrchestrationClient;
pub use transport::{OrchestrationClient, RestOrchestrationClient, UnifiedOrchestrationClient};

// Re-export unified transport types for worker
#[cfg(feature = "grpc")]
pub use transport::GrpcWorkerClient;
pub use transport::{RestWorkerClient, UnifiedWorkerClient, WorkerClient};

// Re-export gRPC clients when feature is enabled
#[cfg(feature = "grpc")]
pub use grpc_clients::{
    AuthInterceptor, GrpcAuthConfig, GrpcClientConfig, OrchestrationGrpcClient, WorkerGrpcClient,
};
