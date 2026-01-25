//! # Service Layer (TAS-168)
//!
//! Business logic services that encapsulate complex operations separate from
//! web handlers. This follows the separation of concerns principle:
//!
//! - **Handlers**: Request validation, permission checking, response formatting
//! - **Services**: Business logic, database queries, caching, aggregation
//!
//! ## Available Services
//!
//! - [`AnalyticsService`]: Cache-aware analytics with query delegation
//! - [`AnalyticsQueryService`]: Database queries and data aggregation

mod analytics_query_service;
mod analytics_service;

pub use analytics_query_service::AnalyticsQueryService;
pub use analytics_service::AnalyticsService;
