//! gRPC interceptors for Worker services.
//!
//! Provides authentication and other cross-cutting concerns for gRPC services.

pub mod auth;

pub use auth::AuthInterceptor;
