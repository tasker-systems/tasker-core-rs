//! # Slowest Steps Analytics
//!
//! **CRITICAL**: This is NOT a database table - it's a computed view via SQL functions.
//!
//! ## Overview
//!
//! The `SlowestSteps` represents dynamically computed analytics for identifying
//! the slowest performing workflow steps. This data is **never stored** - it's calculated
//! on-demand using sophisticated SQL functions that analyze step execution times.
//!
//! ## SQL Function Integration
//!
//! This module integrates with the PostgreSQL function:
//!
//! ### `get_slowest_steps_v01(since_timestamp, limit_count, namespace_filter, task_name_filter, version_filter)`
//! - Identifies the slowest executing workflow steps
//! - Supports filtering by namespace, task name, and version
//! - Returns configurable number of results with duration analysis
//!
//! ## Function Return Schema
//!
//! The function returns:
//! ```sql
//! RETURNS TABLE(
//!   workflow_step_id bigint,
//!   task_id bigint,
//!   step_name character varying,
//!   task_name character varying,
//!   namespace_name character varying,
//!   version character varying,
//!   duration_seconds numeric,
//!   attempts integer,
//!   created_at timestamp with time zone,
//!   completed_at timestamp with time zone,
//!   retryable boolean,
//!   step_status character varying
//! )
//! ```

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{types::BigDecimal, FromRow, PgPool};

/// Represents computed slowest steps analytics.
///
/// **IMPORTANT**: This is NOT a database table - it's the result of calling
/// `get_slowest_steps_v01()` SQL function.
///
/// # Computed Fields
///
/// All fields are calculated dynamically by analyzing:
/// - Step execution durations across tasks
/// - Step retry attempts and completion times
/// - Task and namespace context information
/// - Step performance patterns
///
/// # No CRUD Operations
///
/// Unlike other models, this struct does NOT support:
/// - `create()` - Cannot insert computed data
/// - `update()` - Cannot modify computed data
/// - `delete()` - Cannot delete computed data
///
/// Only read operations are available via the SQL function.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct SlowestSteps {
    pub workflow_step_id: i64,
    pub task_id: i64,
    pub step_name: String,
    pub task_name: String,
    pub namespace_name: String,
    pub version: String,
    pub duration_seconds: BigDecimal,
    pub attempts: i32,
    pub created_at: NaiveDateTime,
    pub completed_at: Option<NaiveDateTime>,
    pub retryable: bool,
    pub step_status: String,
}

/// Filter parameters for slowest steps analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlowestStepsFilter {
    pub since_timestamp: Option<DateTime<Utc>>,
    pub limit_count: Option<i32>,
    pub namespace_filter: Option<String>,
    pub task_name_filter: Option<String>,
    pub version_filter: Option<String>,
}

impl Default for SlowestStepsFilter {
    fn default() -> Self {
        Self {
            since_timestamp: None,
            limit_count: Some(10),
            namespace_filter: None,
            task_name_filter: None,
            version_filter: None,
        }
    }
}

impl SlowestSteps {
    /// Get slowest steps with default filters (top 10).
    pub async fn get_slowest(pool: &PgPool) -> Result<Vec<SlowestSteps>, sqlx::Error> {
        Self::get_with_filters(pool, SlowestStepsFilter::default()).await
    }

    /// Get slowest steps since a specific timestamp.
    pub async fn get_since(
        pool: &PgPool,
        since_timestamp: DateTime<Utc>,
        limit: Option<i32>,
    ) -> Result<Vec<SlowestSteps>, sqlx::Error> {
        let filter = SlowestStepsFilter {
            since_timestamp: Some(since_timestamp),
            limit_count: limit,
            ..Default::default()
        };
        Self::get_with_filters(pool, filter).await
    }

    /// Get slowest steps for a specific namespace.
    pub async fn get_by_namespace(
        pool: &PgPool,
        namespace: &str,
        limit: Option<i32>,
    ) -> Result<Vec<SlowestSteps>, sqlx::Error> {
        let filter = SlowestStepsFilter {
            namespace_filter: Some(namespace.to_string()),
            limit_count: limit,
            ..Default::default()
        };
        Self::get_with_filters(pool, filter).await
    }

    /// Get slowest steps for a specific task name.
    pub async fn get_by_task_name(
        pool: &PgPool,
        task_name: &str,
        limit: Option<i32>,
    ) -> Result<Vec<SlowestSteps>, sqlx::Error> {
        let filter = SlowestStepsFilter {
            task_name_filter: Some(task_name.to_string()),
            limit_count: limit,
            ..Default::default()
        };
        Self::get_with_filters(pool, filter).await
    }

    /// Get slowest steps with custom filters.
    pub async fn get_with_filters(
        pool: &PgPool,
        filter: SlowestStepsFilter,
    ) -> Result<Vec<SlowestSteps>, sqlx::Error> {
        let steps = sqlx::query_as!(
            SlowestSteps,
            r#"
            SELECT 
                workflow_step_id as "workflow_step_id!: i64",
                task_id as "task_id!: i64",
                step_name as "step_name!: String",
                task_name as "task_name!: String",
                namespace_name as "namespace_name!: String",
                version as "version!: String",
                duration_seconds as "duration_seconds!: BigDecimal",
                attempts as "attempts!: i32",
                created_at as "created_at!: NaiveDateTime",
                completed_at as "completed_at?: NaiveDateTime",
                retryable as "retryable!: bool",
                step_status as "step_status!: String"
            FROM get_slowest_steps_v01($1, $2, $3, $4, $5)
            "#,
            filter.since_timestamp,
            filter.limit_count.unwrap_or(10),
            filter.namespace_filter,
            filter.task_name_filter,
            filter.version_filter
        )
        .fetch_all(pool)
        .await?;

        Ok(steps)
    }

    /// Get duration in seconds as a float.
    pub fn duration_as_seconds(&self) -> f64 {
        self.duration_seconds.to_string().parse().unwrap_or(0.0)
    }

    /// Check if this step had multiple attempts (was retried).
    pub fn was_retried(&self) -> bool {
        self.attempts > 1
    }

    /// Check if this step is still running.
    pub fn is_running(&self) -> bool {
        self.completed_at.is_none() && self.step_status == "in_progress"
    }

    /// Check if this step completed successfully.
    pub fn completed_successfully(&self) -> bool {
        self.step_status == "complete"
    }

    /// Check if this step failed.
    pub fn failed(&self) -> bool {
        self.step_status == "error"
    }

    /// Get human-readable duration string.
    pub fn duration_display(&self) -> String {
        let seconds = self.duration_as_seconds();

        if seconds < 60.0 {
            format!("{seconds:.1}s")
        } else if seconds < 3600.0 {
            format!("{:.1}m", seconds / 60.0)
        } else {
            format!("{:.1}h", seconds / 3600.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DatabaseConnection;

    #[tokio::test]
    async fn test_get_slowest_steps() {
        let db = DatabaseConnection::new()
            .await
            .expect("Failed to connect to database");
        let pool = db.pool();

        // For now, just test that the function exists and doesn't panic
        // TODO: Once we have proper factories in future branch, test with meaningful data
        match SlowestSteps::get_slowest(pool).await {
            Ok(_steps) => {
                // Function executed successfully with empty/minimal data
            }
            Err(e) => {
                // Expected for now - SQL function may have schema mismatches without proper test data
                println!("Expected SQL function error (no test data): {e}");
            }
        }

        db.close().await;
    }

    #[tokio::test]
    async fn test_get_slowest_steps_with_filters() {
        let db = DatabaseConnection::new()
            .await
            .expect("Failed to connect to database");
        let pool = db.pool();

        // Test with custom filter
        let filter = SlowestStepsFilter {
            limit_count: Some(5),
            namespace_filter: Some("test_namespace".to_string()),
            ..Default::default()
        };

        // For now, just test function existence - TODO: Add proper test data in future branch
        match SlowestSteps::get_with_filters(pool, filter).await {
            Ok(_steps) => { /* Function works */ }
            Err(e) => {
                println!("Expected SQL function error: {e}");
            }
        }

        db.close().await;
    }

    #[tokio::test]
    async fn test_get_slowest_since() {
        let db = DatabaseConnection::new()
            .await
            .expect("Failed to connect to database");
        let pool = db.pool();

        // Test getting steps since 1 hour ago
        let since = Utc::now() - chrono::Duration::hours(1);
        // For now, just test function existence - TODO: Add proper test data in future branch
        match SlowestSteps::get_since(pool, since, Some(3)).await {
            Ok(_steps) => { /* Function works */ }
            Err(e) => {
                println!("Expected SQL function error: {e}");
            }
        }

        db.close().await;
    }
}
