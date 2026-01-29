//! gRPC server implementation for Tasker Worker.
//!
//! This module provides a gRPC API alongside the existing REST API, reusing
//! the same service layer and security infrastructure.
//!
//! # Architecture
//!
//! The gRPC server mirrors the REST API structure:
//! - **State**: `WorkerGrpcState` wraps the same services as `WorkerWebState`
//! - **Interceptors**: Auth interceptor mirrors REST auth middleware
//! - **Services**: gRPC service implementations delegate to existing service layer
//!
//! # Usage
//!
//! The gRPC server is started alongside the HTTP server in the bootstrap process.
//! By default, it runs on port 9100 (configurable via `worker.grpc.bind_address`).
//!
//! ```bash
//! # Test with grpcurl
//! grpcurl -plaintext localhost:9100 list
//! grpcurl -plaintext localhost:9100 tasker.v1.HealthService/CheckLiveness
//! ```
//!
//! # Configuration
//!
//! ```toml
//! [worker.grpc]
//! enabled = true
//! bind_address = "0.0.0.0:9100"
//! ```

pub mod interceptors;
pub mod server;
pub mod services;
pub mod state;

pub use server::{GrpcServer, GrpcServerHandle};
pub use state::WorkerGrpcState;
