//! Command Handlers Module

//! Command handlers for the unified command pattern architecture.
//!
//! This module contains all command handlers that implement the CommandHandler
//! trait to process specific command types through the CommandRouter.

pub mod batch_execution_sender;
pub mod health_check_handler;
pub mod result_aggregation_handler;
pub mod task_initialization_handler;
pub mod worker_management_handler;

// Re-export handlers for convenience
pub use batch_execution_sender::{BatchExecutionSender, BatchSendError, BatchSendResult};
pub use health_check_handler::HealthCheckHandler;
pub use result_aggregation_handler::{ResultAggregationHandler, ResultHandlerConfig};
pub use task_initialization_handler::TaskInitializationHandler;
pub use worker_management_handler::WorkerManagementHandler;
