//! # Service Layer (TAS-168, TAS-76)
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
//! - [`TaskService`]: Task creation, retrieval, cancellation business logic (TAS-76)
//! - [`TaskQueryService`]: Task database queries with execution context (TAS-76)
//! - [`StepService`]: Step resolution and state machine operations (TAS-76)
//! - [`StepQueryService`]: Step database queries with readiness status (TAS-76)
//! - [`HealthService`]: Health check logic for orchestration system (TAS-76)
//! - [`TemplateQueryService`]: Template discovery and retrieval (TAS-76)

mod analytics_query_service;
mod analytics_service;
pub mod health;
pub mod shared;
pub mod step_query_service;
pub mod step_service;
pub mod task_query_service;
pub mod task_service;
pub mod template_query_service;

pub use analytics_query_service::AnalyticsQueryService;
pub use analytics_service::AnalyticsService;
pub use health::HealthService;
pub use shared::{SharedApiServices, SharedApiServicesError};
pub use step_query_service::{
    StepQueryError, StepQueryResult, StepQueryService, StepWithReadiness,
};
pub use step_service::{StepService, StepServiceError, StepServiceResult};
pub use task_query_service::{
    PaginatedTasksWithContext, TaskQueryError, TaskQueryResult, TaskQueryService, TaskWithContext,
};
pub use task_service::{TaskService, TaskServiceError, TaskServiceResult};
pub use template_query_service::{TemplateQueryError, TemplateQueryResult, TemplateQueryService};
