//! Command Handlers Module

//! Remaining command handlers for step execution and result processing.
//! Most TCP-based handlers have been removed in favor of pgmq architecture.

pub mod health_check_handler;
pub mod result_aggregation_handler;
pub mod task_initialization_handler;

// Re-export handlers for convenience
pub use health_check_handler::HealthCheckHandler;
pub use result_aggregation_handler::{ResultAggregationHandler, ResultHandlerConfig};
pub use task_initialization_handler::TaskInitializationHandler;
