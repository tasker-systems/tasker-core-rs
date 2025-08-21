//! # Insights and Analytics Models
//!
//! This module contains models for system insights, analytics, and performance monitoring.
//! These models provide computed views via SQL functions for understanding system behavior,
//! performance bottlenecks, and health metrics.
//!
//! ## Analytics Models
//!
//! All models in this module are **computed views** - they represent the output of SQL functions
//! that analyze the underlying task and step data to provide insights. None of these models
//! support create, update, or delete operations since they represent computed analytics.
//!
//! ### Core Analytics
//! - `AnalyticsMetrics`: System-wide performance and health metrics
//! - `SystemHealthCounts`: Real-time system health and capacity monitoring
//!
//! ### Performance Analysis  
//! - `SlowestSteps`: Identifies performance bottlenecks at the step level
//! - `SlowestTasks`: Task-level performance analysis and optimization insights
//!
//! ## Usage Patterns
//!
//! These models are typically used for:
//! - Dashboard and monitoring displays
//! - Performance optimization and bottleneck identification
//! - System health monitoring and alerting
//! - Capacity planning and resource optimization
//! - Historical trend analysis and reporting

pub mod analytics_metrics;
pub mod slowest_steps;
pub mod slowest_tasks;
pub mod system_health_counts;

// Re-export for easy access
pub use analytics_metrics::AnalyticsMetrics;
pub use slowest_steps::{SlowestSteps, SlowestStepsFilter};
pub use slowest_tasks::{SlowestTasks, SlowestTasksFilter};
pub use system_health_counts::{SystemHealthCounts, SystemHealthSummary};
