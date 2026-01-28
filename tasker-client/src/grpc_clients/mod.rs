//! # gRPC Client Library
//!
//! Feature-gated gRPC client implementations for Tasker orchestration and worker APIs.
//! These clients mirror the REST API clients, providing an alternative transport protocol.
//!
//! # Feature Flag
//!
//! This module is only available when the `grpc` feature is enabled:
//!
//! ```toml
//! [dependencies]
//! tasker-client = { version = "0.1", features = ["grpc"] }
//! ```
//!
//! # Quick Start
//!
//! ## Orchestration Client
//!
//! ```rust,ignore
//! use tasker_client::grpc_clients::{OrchestrationGrpcClient, GrpcClientConfig};
//! use tasker_shared::models::core::task_request::TaskRequest;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create client with default configuration
//!     let client = OrchestrationGrpcClient::connect("http://localhost:9090").await?;
//!
//!     // Create a task
//!     let task = client.create_task(TaskRequest {
//!         name: "example".to_string(),
//!         namespace: "default".to_string(),
//!         version: "1.0.0".to_string(),
//!         context: serde_json::json!({}),
//!     }).await?;
//!
//!     println!("Created task: {}", task.task_uuid);
//!     Ok(())
//! }
//! ```
//!
//! ## Worker Client
//!
//! ```rust,ignore
//! use tasker_client::grpc_clients::WorkerGrpcClient;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let client = WorkerGrpcClient::connect("http://localhost:9100").await?;
//!
//!     let health = client.health_check().await?;
//!     println!("Worker status: {}", health.status);
//!     Ok(())
//! }
//! ```

mod common;
mod conversions;
mod orchestration_grpc_client;
mod worker_grpc_client;

pub use common::{AuthInterceptor, GrpcAuthConfig, GrpcClientConfig};
pub use orchestration_grpc_client::OrchestrationGrpcClient;
pub use worker_grpc_client::WorkerGrpcClient;
