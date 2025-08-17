//! # Finalization Attempt Model
//!
//! Audit trail for task finalization claiming attempts to support debugging and metrics.
//!
//! ## Overview
//!
//! The `FinalizationAttempt` model provides comprehensive audit logging for TAS-37
//! finalization claiming operations. Every claim attempt is recorded with full context
//! to enable debugging race conditions and monitoring system performance.
//!
//! ## Key Features
//!
//! - **Complete Audit Trail**: Records every claim attempt with outcome and context
//! - **UUID v7 Primary Key**: Consistent with other models, provides time-ordering
//! - **Race Condition Debugging**: Tracks contention patterns and claim failures
//! - **Metrics Integration**: Supports operational monitoring and alerting
//! - **Transaction Safety**: Records are inserted as part of claim transaction
//!
//! ## Database Schema
//!
//! Maps to `tasker_finalization_attempts` table:
//! ```sql
//! CREATE TABLE tasker_finalization_attempts (
//!   finalization_attempt_uuid UUID PRIMARY KEY DEFAULT uuid_generate_v7(),
//!   task_uuid UUID NOT NULL,
//!   processor_id VARCHAR NOT NULL,
//!   claimed BOOLEAN NOT NULL,
//!   already_claimed_by VARCHAR,
//!   task_state VARCHAR,
//!   message VARCHAR,
//!   attempted_at TIMESTAMP DEFAULT NOW() NOT NULL
//! );
//! ```
//!
//! ## TAS-37 Integration
//!
//! This model is automatically populated by the `claim_task_for_finalization()` SQL function
//! to provide audit trails for race condition prevention and system monitoring.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// Represents a finalization claim attempt with complete audit context.
///
/// Each record provides a complete audit trail of finalization claiming attempts,
/// enabling debugging of race conditions and system performance monitoring.
///
/// # Database Mapping
///
/// Maps directly to the `tasker_finalization_attempts` table:
/// ```sql
/// CREATE TABLE tasker_finalization_attempts (
///   finalization_attempt_uuid UUID PRIMARY KEY DEFAULT uuid_generate_v7(),
///   task_uuid UUID NOT NULL,
///   processor_id VARCHAR NOT NULL,
///   claimed BOOLEAN NOT NULL,
///   already_claimed_by VARCHAR,
///   task_state VARCHAR,
///   message VARCHAR,
///   attempted_at TIMESTAMP DEFAULT NOW() NOT NULL
/// );
/// ```
///
/// # Usage Patterns
///
/// Records are primarily created by SQL functions during claim attempts:
/// - Successful claims: `claimed = true, already_claimed_by = processor_id`
/// - Failed claims: `claimed = false, already_claimed_by = current_owner`
/// - Race conditions: `claimed = false, message = "Failed to claim - race condition"`
/// - Not ready: `claimed = false, message = "Task has no completed or failed steps"`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct FinalizationAttempt {
    /// Primary key - UUID v7 for time-ordered audit records
    pub finalization_attempt_uuid: Uuid,

    /// The task that was attempted to be claimed for finalization
    pub task_uuid: Uuid,

    /// Identifier of the processor attempting the claim
    pub processor_id: String,

    /// Whether the claim attempt succeeded
    pub claimed: bool,

    /// If claim failed, who currently owns the claim (if anyone)
    pub already_claimed_by: Option<String>,

    /// Task execution status at time of claim attempt
    pub task_state: Option<String>,

    /// Human-readable message explaining the claim result
    pub message: Option<String>,

    /// Timestamp when the claim attempt was made
    pub attempted_at: NaiveDateTime,
}

impl FinalizationAttempt {
    /// Find all finalization attempts for a specific task
    ///
    /// Returns attempts ordered by `attempted_at` descending (most recent first).
    ///
    /// # Arguments
    /// * `pool` - Database connection pool
    /// * `task_uuid` - UUID of the task to find attempts for
    ///
    /// # Example
    /// ```rust,no_run
    /// # use tasker_core::models::core::finalization_attempt::FinalizationAttempt;
    /// # use sqlx::PgPool;
    /// # use uuid::Uuid;
    /// # async fn example(pool: PgPool) -> Result<(), sqlx::Error> {
    /// let task_uuid = Uuid::new_v4();
    /// let attempts = FinalizationAttempt::find_by_task(&pool, task_uuid).await?;
    /// println!("Found {} attempts for task {}", attempts.len(), task_uuid);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn find_by_task(pool: &PgPool, task_uuid: Uuid) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            r#"
            SELECT
                finalization_attempt_uuid,
                task_uuid,
                processor_id,
                claimed,
                already_claimed_by,
                task_state,
                message,
                attempted_at
            FROM tasker_finalization_attempts
            WHERE task_uuid = $1
            ORDER BY attempted_at DESC
            "#,
        )
        .bind(task_uuid)
        .fetch_all(pool)
        .await
    }

    /// Find all finalization attempts by a specific processor
    ///
    /// Returns attempts ordered by `attempted_at` descending (most recent first).
    ///
    /// # Arguments
    /// * `pool` - Database connection pool
    /// * `processor_id` - ID of the processor to find attempts for
    /// * `limit` - Maximum number of results to return
    pub async fn find_by_processor(
        pool: &PgPool,
        processor_id: &str,
        limit: Option<i64>,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let limit_clause = limit.map(|l| format!("LIMIT {l}")).unwrap_or_default();

        let query = format!(
            r#"
            SELECT
                finalization_attempt_uuid,
                task_uuid,
                processor_id,
                claimed,
                already_claimed_by,
                task_state,
                message,
                attempted_at
            FROM tasker_finalization_attempts
            WHERE processor_id = $1
            ORDER BY attempted_at DESC
            {limit_clause}
            "#,
        );

        sqlx::query_as::<_, Self>(&query)
            .bind(processor_id)
            .fetch_all(pool)
            .await
    }

    /// Find successful claims by a specific processor
    ///
    /// Returns only attempts where `claimed = true`, useful for tracking
    /// which tasks a processor has successfully claimed.
    pub async fn find_successful_claims_by_processor(
        pool: &PgPool,
        processor_id: &str,
        limit: Option<i64>,
    ) -> Result<Vec<Self>, sqlx::Error> {
        let limit_clause = limit.map(|l| format!("LIMIT {l}")).unwrap_or_default();

        let query = format!(
            r#"
            SELECT
                finalization_attempt_uuid,
                task_uuid,
                processor_id,
                claimed,
                already_claimed_by,
                task_state,
                message,
                attempted_at
            FROM tasker_finalization_attempts
            WHERE processor_id = $1 AND claimed = true
            ORDER BY attempted_at DESC
            {limit_clause}
            "#,
        );

        sqlx::query_as::<_, Self>(&query)
            .bind(processor_id)
            .fetch_all(pool)
            .await
    }

    /// Get race condition statistics for monitoring
    ///
    /// Returns count of attempts grouped by claim result type for operational monitoring.
    ///
    /// # Returns
    /// A tuple of (successful_claims, failed_claims, race_conditions, not_ready_tasks)
    pub async fn get_claim_statistics(
        pool: &PgPool,
        since: Option<NaiveDateTime>,
    ) -> Result<(i64, i64, i64, i64), sqlx::Error> {
        let since_clause = since.map(|_| "WHERE attempted_at >= $1").unwrap_or("");

        let query = format!(
            r#"
            SELECT
                COUNT(CASE WHEN claimed = true THEN 1 END) as successful_claims,
                COUNT(CASE WHEN claimed = false AND message LIKE '%already claimed%' THEN 1 END) as failed_claims,
                COUNT(CASE WHEN claimed = false AND message LIKE '%race condition%' THEN 1 END) as race_conditions,
                COUNT(CASE WHEN claimed = false AND message LIKE '%no completed%' THEN 1 END) as not_ready_tasks
            FROM tasker_finalization_attempts
            {since_clause}
            "#,
        );

        let result = if let Some(since_time) = since {
            sqlx::query_as::<_, (i64, i64, i64, i64)>(&query)
                .bind(since_time)
                .fetch_one(pool)
                .await?
        } else {
            sqlx::query_as::<_, (i64, i64, i64, i64)>(&query)
                .fetch_one(pool)
                .await?
        };

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_finalization_attempt_serialization() {
        let attempt = FinalizationAttempt {
            finalization_attempt_uuid: Uuid::new_v4(),
            task_uuid: Uuid::new_v4(),
            processor_id: "test_processor_123".to_string(),
            claimed: true,
            already_claimed_by: Some("test_processor_123".to_string()),
            task_state: Some("processing".to_string()),
            message: Some("Successfully claimed for finalization".to_string()),
            attempted_at: chrono::Utc::now().naive_utc(),
        };

        let serialized = serde_json::to_string(&attempt).unwrap();
        let deserialized: FinalizationAttempt = serde_json::from_str(&serialized).unwrap();

        assert_eq!(
            attempt.finalization_attempt_uuid,
            deserialized.finalization_attempt_uuid
        );
        assert_eq!(attempt.task_uuid, deserialized.task_uuid);
        assert_eq!(attempt.processor_id, deserialized.processor_id);
        assert_eq!(attempt.claimed, deserialized.claimed);
    }

    #[test]
    fn test_failed_claim_structure() {
        let attempt = FinalizationAttempt {
            finalization_attempt_uuid: Uuid::new_v4(),
            task_uuid: Uuid::new_v4(),
            processor_id: "test_processor_456".to_string(),
            claimed: false,
            already_claimed_by: Some("other_processor_789".to_string()),
            task_state: Some("processing".to_string()),
            message: Some("Task already claimed for finalization".to_string()),
            attempted_at: chrono::Utc::now().naive_utc(),
        };

        // Verify failed claim structure
        assert!(!attempt.claimed);
        assert!(attempt.already_claimed_by.is_some());
        assert_ne!(attempt.processor_id, attempt.already_claimed_by.unwrap());
    }
}
