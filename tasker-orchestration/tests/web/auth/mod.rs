//! # Auth Enforcement E2E Tests
//!
//! TAS-150: Comprehensive tests for API-level security enforcement.
//! Validates JWT and API key authentication plus per-handler permission checks.
//!
//! Tests are organized by resource:
//! - `common` - Health endpoints, no-credentials, invalid/expired/malformed tokens
//! - `tasks` - Task CRUD permission enforcement
//! - `workflow_steps` - Step read/resolve permission enforcement
//! - `analytics` - Analytics read permission enforcement
//! - `config` - Config read permission enforcement
//! - `handlers` - Handler registry permission enforcement
//! - `dlq` - Dead letter queue permission enforcement
//! - `api_keys` - API key authentication and permission enforcement

pub mod analytics;
pub mod api_keys;
pub mod common;
pub mod config;
pub mod dlq;
pub mod handlers;
pub mod tasks;
pub mod workflow_steps;
