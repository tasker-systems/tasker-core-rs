# Doctest Patterns for Database-Dependent Code

This document outlines the patterns we use for writing effective doctests in a database-heavy codebase.

## Pattern 1: Pure Functions (Recommended) ✅

For functions that don't require database access, write normal testable doctests:

```rust
/// Generates a deterministic identity hash for task deduplication.
/// 
/// # Example
/// 
/// ```rust
/// use serde_json::json;
/// use tasker_core::models::Task;
/// 
/// let context = Some(json!({"order_id": 12345}));
/// let hash = Task::generate_identity_hash(1, &context);
/// 
/// assert_eq!(hash.len(), 64); // SHA-256 hex string
/// assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
/// ```
pub fn generate_identity_hash(named_task_id: i32, context: &Option<serde_json::Value>) -> String
```

## Pattern 2: No-Run Examples with Integration Test References ✅

For database-dependent functions, use `no_run` to ensure compilation without execution:

```rust
/// Gets current analytics metrics from the database.
/// 
/// # Example Usage
/// 
/// ```rust,no_run
/// use sqlx::PgPool;
/// use tasker_core::models::insights::AnalyticsMetrics;
/// 
/// # async fn example(pool: PgPool) -> Result<(), sqlx::Error> {
/// let metrics = AnalyticsMetrics::get_current(&pool).await?;
/// 
/// if let Some(metrics) = metrics {
///     println!("Active tasks: {}", metrics.active_tasks_count);
/// }
/// # Ok(())
/// # }
/// ```
/// 
/// For complete working examples with database setup, see the integration tests in 
/// `tests/models/analytics_metrics.rs`.
pub async fn get_current(pool: &PgPool) -> Result<Option<AnalyticsMetrics>, sqlx::Error>
```

## Pattern 3: Mock/Example Data Pattern ✅

For complex types, show how to construct them with example data:

```rust
/// System health metrics with computed health scores.
/// 
/// # Example
/// 
/// ```rust
/// use tasker_core::models::insights::SystemHealthCounts;
/// 
/// // Example with healthy system metrics
/// let health = SystemHealthCounts {
///     total_tasks: 100,
///     complete_tasks: 90,
///     error_tasks: 2,
///     pending_tasks: 8,
///     // ... other fields
/// };
/// 
/// assert!(health.is_healthy());
/// assert!(health.overall_health_score() > 85.0);
/// ```
pub struct SystemHealthCounts { /* ... */ }
```

## Pattern 4: Compile-Only with Detailed Comments 🔧

For very complex database operations, use compile-only examples with explanatory comments:

```rust
/// Creates a new task with workflow steps and dependencies.
/// 
/// ```rust,compile_fail
/// // This example shows the API but requires database setup
/// use tasker_core::models::{Task, NewTask};
/// use serde_json::json;
/// 
/// // 1. First, you would set up your database pool:
/// // let pool = setup_database_pool().await?;
/// 
/// // 2. Create a task with context
/// let new_task = NewTask {
///     named_task_id: 1,
///     context: Some(json!({"order_id": 12345})),
///     // ... other fields
/// };
/// 
/// // 3. Create the task
/// // let task = Task::create(&pool, new_task).await?;
/// ```
/// 
/// **Real Examples**: See `tests/models/task.rs` for complete working examples
/// with database setup using `#[sqlx::test]`.
pub async fn create(pool: &PgPool, new_task: NewTask) -> Result<Task, sqlx::Error>
```

## Pattern 5: Doctest Helpers (Advanced) 🚀

For shared setup code, use `#[cfg(doctest)]` helpers:

```rust
#[cfg(doctest)]
/// Helper function for doctests - not included in production builds
async fn example_pool() -> sqlx::PgPool {
    // This function only exists during doctest compilation
    // and can be used in doctests that need a pool
    todo!("This is just for doctest compilation")
}

/// Database-dependent function with helper
/// 
/// ```rust,no_run
/// # use sqlx::PgPool;
/// # async fn example_pool() -> PgPool { todo!() }
/// # 
/// # async fn example() -> Result<(), sqlx::Error> {
/// let pool = example_pool().await;
/// let metrics = AnalyticsMetrics::get_current(&pool).await?;
/// # Ok(())
/// # }
/// ```
```

## Guidelines for Choosing Patterns

### Use Pattern 1 (Pure Functions) when:
- Function doesn't require database access
- Function is deterministic 
- Function can be tested with simple inputs

### Use Pattern 2 (No-Run + Integration Reference) when:
- Function requires database access
- You want to show realistic usage
- You have corresponding integration tests

### Use Pattern 3 (Mock/Example Data) when:
- Showing how to construct complex types
- Demonstrating computed properties
- Testing business logic without I/O

### Use Pattern 4 (Compile-Only) when:
- API is complex and requires setup
- You want to show the full workflow
- Direct testing isn't practical

### Use Pattern 5 (Doctest Helpers) when:
- Multiple doctests need similar setup
- You want to test actual database operations
- You can provide meaningful test fixtures

## Best Practices

1. **Always prefer runnable examples** when possible
2. **Reference integration tests** for complete examples
3. **Use meaningful variable names** that show intent
4. **Include error handling** to show proper usage
5. **Add assertions** to demonstrate expected behavior
6. **Keep examples focused** on the specific API being documented

## Migration Strategy

1. **Start with pure functions** - convert `ignore` to runnable tests
2. **Add no-run examples** for database functions with integration test references  
3. **Create mock examples** for complex types to show usage patterns
4. **Add compile-only examples** only when other patterns don't work
5. **Consider doctest helpers** for frequently used patterns

This approach gives developers confidence in the examples while maintaining excellent documentation quality.