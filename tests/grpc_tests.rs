//! gRPC transport tests (TAS-177).
//!
//! Tests for gRPC client transport layer, including:
//! - Health endpoint tests
//! - REST/gRPC response parity tests
//! - Authentication tests
//!
//! Requires: Orchestration + worker services running with gRPC enabled
//! Enable with: --features test-services

#![cfg(all(feature = "test-services", feature = "grpc"))]

mod common;
mod grpc;
