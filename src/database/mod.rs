//! # Database Operations
//!
//! High-performance database layer with SQLx integration and PostgreSQL function wrappers.
//!
//! ## Overview
//!
//! This module provides comprehensive database operations including:
//! - Connection management with automatic pooling
//! - Migration system with PostgreSQL advisory locks
//! - SQL function execution with compile-time verification
//! - Analytics and performance monitoring
//!
//! ## Key Components
//!
//! - [`connection`] - Database connection management and pooling
//! - [`migrations`] - Schema migration system with concurrency control
//! - [`sql_functions`] - PostgreSQL function wrappers with type safety
//!
//! ## SQL Function Integration
//!
//! The module wraps 8+ critical PostgreSQL functions:
//! - Task execution context and status
//! - Step readiness and dependency analysis
//! - System-wide analytics and performance metrics
//! - Health monitoring and capacity tracking
//!
//! ## Performance Features
//!
//! - **Connection Pooling**: Thread-safe SQLx integration
//! - **Compile-time Verification**: SQLx compile-time query validation
//! - **Function Optimization**: Direct PostgreSQL function calls
//! - **Batch Operations**: Efficient multi-record processing
//!
//! ## Example Usage
//!
//! ```rust,no_run
//! use tasker_core::database::{DatabaseConnection, SqlFunctionExecutor};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Initialize database connection
//! let db = DatabaseConnection::new().await?;
//! let pool = db.pool();
//!
//! // Execute SQL functions
//! let executor = SqlFunctionExecutor::new(pool.clone());
//! let metrics = executor.get_analytics_metrics(None).await?;
//! # Ok(())
//! # }
//! ```

pub mod connection;
pub mod connection_pool_strategies;
pub mod migrations;
pub mod optimized_queries;
pub mod sql_functions;

pub use connection::DatabaseConnection;
pub use connection_pool_strategies::{
    DeploymentPattern, HealthCheckResult, OptimizedDatabaseConnection, PoolConfiguration,
    PoolMetrics,
};
pub use migrations::DatabaseMigrations;
pub use optimized_queries::{
    OptimizedWorkerQueries, PoolStatistics, QueryResultCache,
};
pub use sql_functions::{
    ActiveWorkerResult, AnalyticsMetrics, DependencyLevel, FunctionRegistry, OptimalWorkerResult, 
    SlowestStepAnalysis, SlowestTaskAnalysis, SqlFunctionExecutor, StepReadinessStatus, 
    SystemHealthCounts, TaskExecutionContext, WorkerHealthResult, WorkerPoolStatistics,
};
