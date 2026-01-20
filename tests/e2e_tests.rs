//! End-to-end tests with running services.
//!
//! Requires: Orchestration + worker services running
//! Enable with: --features test-services

#![cfg(feature = "test-services")]

mod common;
mod e2e;
