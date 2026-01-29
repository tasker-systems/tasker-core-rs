//! gRPC server implementation for Tasker Orchestration.
//!
//! This module provides a gRPC API alongside the existing REST API, reusing
//! the same service layer and security infrastructure.
//!
//! # Architecture
//!
//! The gRPC server mirrors the REST API structure:
//! - **State**: `GrpcState` wraps the same services as `AppState`
//! - **Interceptors**: Auth interceptor mirrors REST auth middleware
//! - **Services**: gRPC service implementations delegate to existing service layer
//!
//! # Usage
//!
//! The gRPC server is started alongside the HTTP server in the bootstrap process.
//! By default, it runs on port 9090 (configurable via `orchestration.grpc.bind_address`).
//!
//! ```bash
//! # Test with grpcurl
//! grpcurl -plaintext localhost:9090 list
//! grpcurl -plaintext localhost:9090 tasker.v1.HealthService/CheckLiveness
//! ```
//!
//! # Configuration
//!
//! ```toml
//! [orchestration.grpc]
//! enabled = true
//! bind_address = "0.0.0.0:9090"
//! ```

pub mod conversions;
pub mod interceptors;
pub mod server;
pub mod services;
pub mod state;

pub use server::{GrpcServer, GrpcServerHandle};
pub use state::GrpcState;
