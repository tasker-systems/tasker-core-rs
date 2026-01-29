//! gRPC service implementations for Worker.
//!
//! Each service delegates to the corresponding service layer component,
//! ensuring consistent behavior between REST and gRPC APIs.
//!
//! Uses worker-specific proto types (WorkerHealthService, WorkerConfigService,
//! WorkerTemplateService) that match the worker REST API exactly.

mod config;
mod health;
mod templates;

pub use config::WorkerConfigServiceImpl;
pub use health::WorkerHealthServiceImpl;
pub use templates::WorkerTemplateServiceImpl;
