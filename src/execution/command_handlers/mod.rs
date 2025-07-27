//! Command Handlers Module

//! Command handlers for the unified command pattern architecture.
//! 
//! This module contains all command handlers that implement the CommandHandler
//! trait to process specific command types through the CommandRouter.

pub mod worker_management_handler;
pub mod batch_execution_handler;
pub mod result_aggregation_handler;
pub mod task_initialization_handler;
pub mod health_check_handler;

// Re-export handlers for convenience
pub use worker_management_handler::WorkerManagementHandler;
pub use batch_execution_handler::BatchExecutionHandler;
pub use result_aggregation_handler::ResultAggregationHandler;
pub use task_initialization_handler::TaskInitializationHandler;
pub use health_check_handler::HealthCheckHandler;