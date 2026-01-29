//! gRPC interceptors for cross-cutting concerns.
//!
//! Interceptors are the gRPC equivalent of HTTP middleware.
//! They process requests before they reach service handlers.

pub mod auth;

pub use auth::AuthInterceptor;
