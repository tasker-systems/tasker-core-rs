//! Insights Model Tests Module
//!
//! This module contains all insights model tests using SQLx native testing
//! for automatic database isolation. These tests cover SQL function wrappers
//! for analytics, performance monitoring, and system health metrics.

// Analytics and performance model tests
pub mod analytics_metrics;
pub mod slowest_steps;
pub mod slowest_tasks;
pub mod system_health_counts;
