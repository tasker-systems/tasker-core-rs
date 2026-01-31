//! # TAS-148: Orchestration Command Types
//!
//! This module contains the command types and result structures for orchestration operations.
//! These types are separated from the command processing logic for clarity and maintainability.
//!
//! ## Command Pattern
//!
//! The `OrchestrationCommand` enum represents all commands that can be sent to the
//! orchestration command processor. Each command variant includes a response channel
//! for async communication of results.
//!
//! ## Result Types
//!
//! Each command has a corresponding result type that encodes the possible outcomes:
//! - `TaskInitializeResult`: Task initialization outcomes
//! - `StepProcessResult`: Step result processing outcomes
//! - `TaskReadinessResult`: Task readiness processing metrics
//! - `TaskFinalizationResult`: Task finalization outcomes
//!
//! ## Provider Abstraction
//!
//! TAS-133 introduced provider-agnostic messaging types:
//! - `QueuedMessage<T>`: Provider-agnostic message with explicit `MessageHandle`
//! - `MessageEvent`: Signal-only notification for PGMQ large message flow

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::oneshot;
use uuid::Uuid;

use tasker_shared::messaging::service::{MessageEvent, QueuedMessage};
use tasker_shared::messaging::{StepExecutionResult, TaskRequestMessage};
use tasker_shared::TaskerResult;

/// Type alias for command response channels
pub type CommandResponder<T> = oneshot::Sender<TaskerResult<T>>;

/// Commands for orchestration operations (TAS-40 Command Pattern)
///
/// These commands replace direct method calls with async command pattern,
/// eliminating polling while preserving sophisticated orchestration logic.
#[derive(Debug)]
pub enum OrchestrationCommand {
    /// Initialize a new task - delegates to TaskRequestProcessor
    InitializeTask {
        request: TaskRequestMessage, // Use existing message format
        resp: CommandResponder<TaskInitializeResult>,
    },
    /// Process a step execution result - delegates to StepResultProcessor
    ProcessStepResult {
        result: StepExecutionResult, // Use existing result format
        resp: CommandResponder<StepProcessResult>,
    },
    /// Finalize a completed task - uses FinalizationClaimer for atomic operation
    FinalizeTask {
        task_uuid: Uuid,
        resp: CommandResponder<TaskFinalizationResult>,
    },
    /// Process step result from message - delegates full message lifecycle to worker
    ///
    /// TAS-133: Uses provider-agnostic QueuedMessage with explicit MessageHandle
    ProcessStepResultFromMessage {
        message: QueuedMessage<serde_json::Value>,
        resp: CommandResponder<StepProcessResult>,
    },
    /// Initialize task from message - delegates full message lifecycle to worker
    ///
    /// TAS-133: Uses provider-agnostic QueuedMessage with explicit MessageHandle
    InitializeTaskFromMessage {
        message: QueuedMessage<serde_json::Value>,
        resp: CommandResponder<TaskInitializeResult>,
    },
    /// Finalize task from message - delegates full message lifecycle to worker
    ///
    /// TAS-133: Uses provider-agnostic QueuedMessage with explicit MessageHandle
    FinalizeTaskFromMessage {
        message: QueuedMessage<serde_json::Value>,
        resp: CommandResponder<TaskFinalizationResult>,
    },
    /// Process step result from message event - delegates full message lifecycle to worker
    ///
    /// TAS-133: Uses provider-agnostic MessageEvent for multi-backend support
    ProcessStepResultFromMessageEvent {
        message_event: MessageEvent,
        resp: CommandResponder<StepProcessResult>,
    },
    /// Initialize task from message event - delegates full message lifecycle to worker
    ///
    /// TAS-133: Uses provider-agnostic MessageEvent for multi-backend support
    InitializeTaskFromMessageEvent {
        message_event: MessageEvent,
        resp: CommandResponder<TaskInitializeResult>,
    },
    /// Finalize task from message event - delegates full message lifecycle to worker
    ///
    /// TAS-133: Uses provider-agnostic MessageEvent for multi-backend support
    FinalizeTaskFromMessageEvent {
        message_event: MessageEvent,
        resp: CommandResponder<TaskFinalizationResult>,
    },
    /// Process task readiness event from PostgreSQL LISTEN/NOTIFY
    /// Delegates to TaskClaimStepEnqueuer for atomic task claiming and step enqueueing
    ProcessTaskReadiness {
        task_uuid: Uuid,
        namespace: String,
        priority: i32,
        ready_steps: i32,
        triggered_by: String, // "step_transition", "task_start", "fallback_polling"
        step_uuid: Option<Uuid>, // Present for step_transition triggers
        step_state: Option<String>, // Present for step_transition triggers
        task_state: Option<String>, // Present for task_start triggers
        resp: CommandResponder<TaskReadinessResult>,
    },
    /// Get orchestration processing statistics
    GetProcessingStats {
        resp: CommandResponder<OrchestrationProcessingStats>,
    },
    /// Perform health check
    HealthCheck {
        resp: CommandResponder<SystemHealth>,
    },
    /// Shutdown orchestration processor
    Shutdown { resp: CommandResponder<()> },
}

/// Result types matching existing orchestration patterns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskInitializeResult {
    Success { task_uuid: Uuid, message: String },
    Failed { error: String },
    Skipped { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StepProcessResult {
    Success { message: String },
    Failed { error: String },
    Skipped { reason: String },
}

/// Result of processing a task readiness event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskReadinessResult {
    pub task_uuid: Uuid,
    pub namespace: String,
    pub steps_enqueued: u32,
    pub steps_discovered: u32,
    pub triggered_by: String,
    pub processing_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskFinalizationResult {
    Success {
        task_uuid: Uuid,
        final_status: String,
        completion_time: Option<chrono::DateTime<chrono::Utc>>,
    },
    NotClaimed {
        reason: String,
        already_claimed_by: Option<Uuid>,
    },
    Failed {
        error: String,
    },
}

/// Serializable snapshot of orchestration processing statistics
///
/// This is the API-facing type returned by `GetProcessingStats` commands.
/// For lock-free hot-path counting, see `AtomicProcessingStats`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationProcessingStats {
    pub task_requests_processed: u64,
    pub step_results_processed: u64,
    pub tasks_finalized: u64,
    pub tasks_ready_processed: u64,
    pub processing_errors: u64,
}

/// Lock-free statistics tracker for orchestration command processing
///
/// Uses `AtomicU64` counters for SWMR (Single Writer, Multiple Reader) access.
/// The command processor loop is the single writer; health checks and API
/// responses read via `snapshot()` which produces a serializable
/// `OrchestrationProcessingStats`.
#[derive(Debug, Default)]
pub struct AtomicProcessingStats {
    pub(crate) task_requests_processed: AtomicU64,
    pub(crate) step_results_processed: AtomicU64,
    pub(crate) tasks_finalized: AtomicU64,
    pub(crate) tasks_ready_processed: AtomicU64,
    pub(crate) processing_errors: AtomicU64,
}

impl AtomicProcessingStats {
    /// Create a serializable snapshot of current statistics
    pub fn snapshot(&self) -> OrchestrationProcessingStats {
        OrchestrationProcessingStats {
            task_requests_processed: self.task_requests_processed.load(Ordering::Relaxed),
            step_results_processed: self.step_results_processed.load(Ordering::Relaxed),
            tasks_finalized: self.tasks_finalized.load(Ordering::Relaxed),
            tasks_ready_processed: self.tasks_ready_processed.load(Ordering::Relaxed),
            processing_errors: self.processing_errors.load(Ordering::Relaxed),
        }
    }
}

/// TAS-75: Enhanced system health status
///
/// This struct contains comprehensive health information derived from
/// cached health status data updated by the background StatusEvaluator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemHealth {
    /// Overall health status: "healthy", "degraded", or "unhealthy"
    pub status: String,

    /// Whether the database is connected (from cached DB health check)
    pub database_connected: bool,

    /// Whether message queues are healthy (not in Critical/Overflow)
    pub message_queues_healthy: bool,

    /// Number of active orchestration processors
    pub active_processors: u32,

    // TAS-75: Enhanced health fields from cached status
    /// Circuit breaker state for database operations
    pub circuit_breaker_open: bool,

    /// Number of consecutive circuit breaker failures
    pub circuit_breaker_failures: u32,

    /// Command channel saturation percentage (0.0-100.0)
    pub command_channel_saturation_percent: f64,

    /// Whether backpressure is currently active
    pub backpressure_active: bool,

    /// Queue depth tier: "Unknown", "Normal", "Warning", "Critical", "Overflow"
    pub queue_depth_tier: String,

    /// Maximum queue depth across all monitored queues
    pub queue_depth_max: i64,

    /// Name of the queue with the highest depth
    pub queue_depth_worst_queue: String,

    /// Whether health data has been evaluated (false means Unknown state)
    pub health_evaluated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    // --- TaskInitializeResult ---

    #[test]
    fn test_task_initialize_result_success() {
        let uuid = Uuid::now_v7();
        let result = TaskInitializeResult::Success {
            task_uuid: uuid,
            message: "Task created".to_string(),
        };

        assert!(matches!(result, TaskInitializeResult::Success { .. }));
        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["Success"]["task_uuid"], uuid.to_string());
        assert_eq!(json["Success"]["message"], "Task created");
    }

    #[test]
    fn test_task_initialize_result_failed() {
        let result = TaskInitializeResult::Failed {
            error: "validation failed".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: TaskInitializeResult = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, TaskInitializeResult::Failed { .. }));
    }

    #[test]
    fn test_task_initialize_result_skipped() {
        let result = TaskInitializeResult::Skipped {
            reason: "duplicate".to_string(),
        };

        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["Skipped"]["reason"], "duplicate");
    }

    #[test]
    fn test_task_initialize_result_clone() {
        let uuid = Uuid::now_v7();
        let result = TaskInitializeResult::Success {
            task_uuid: uuid,
            message: "cloned".to_string(),
        };
        let cloned = result.clone();
        let original_json = serde_json::to_string(&result).unwrap();
        let cloned_json = serde_json::to_string(&cloned).unwrap();
        assert_eq!(original_json, cloned_json);
    }

    // --- StepProcessResult ---

    #[test]
    fn test_step_process_result_all_variants() {
        let success = StepProcessResult::Success {
            message: "step completed".to_string(),
        };
        let failed = StepProcessResult::Failed {
            error: "timeout".to_string(),
        };
        let skipped = StepProcessResult::Skipped {
            reason: "already processed".to_string(),
        };

        assert!(matches!(success, StepProcessResult::Success { .. }));
        assert!(matches!(failed, StepProcessResult::Failed { .. }));
        assert!(matches!(skipped, StepProcessResult::Skipped { .. }));
    }

    #[test]
    fn test_step_process_result_serialization_roundtrip() {
        let result = StepProcessResult::Success {
            message: "processed".to_string(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: StepProcessResult = serde_json::from_str(&json).unwrap();
        assert!(matches!(deserialized, StepProcessResult::Success { .. }));
    }

    // --- TaskReadinessResult ---

    #[test]
    fn test_task_readiness_result_construction() {
        let uuid = Uuid::now_v7();
        let result = TaskReadinessResult {
            task_uuid: uuid,
            namespace: "fulfillment".to_string(),
            steps_enqueued: 5,
            steps_discovered: 8,
            triggered_by: "step_transition".to_string(),
            processing_time_ms: 42,
        };

        assert_eq!(result.task_uuid, uuid);
        assert_eq!(result.namespace, "fulfillment");
        assert_eq!(result.steps_enqueued, 5);
        assert_eq!(result.steps_discovered, 8);
    }

    #[test]
    fn test_task_readiness_result_serialization_roundtrip() {
        let result = TaskReadinessResult {
            task_uuid: Uuid::now_v7(),
            namespace: "test".to_string(),
            steps_enqueued: 3,
            steps_discovered: 10,
            triggered_by: "task_start".to_string(),
            processing_time_ms: 100,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: TaskReadinessResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.namespace, "test");
        assert_eq!(deserialized.steps_enqueued, 3);
    }

    // --- TaskFinalizationResult ---

    #[test]
    fn test_task_finalization_result_success() {
        let uuid = Uuid::now_v7();
        let now = Utc::now();
        let result = TaskFinalizationResult::Success {
            task_uuid: uuid,
            final_status: "complete".to_string(),
            completion_time: Some(now),
        };

        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["Success"]["final_status"], "complete");
    }

    #[test]
    fn test_task_finalization_result_success_no_completion_time() {
        let uuid = Uuid::now_v7();
        let result = TaskFinalizationResult::Success {
            task_uuid: uuid,
            final_status: "error".to_string(),
            completion_time: None,
        };

        let json = serde_json::to_string(&result).unwrap();
        let deserialized: TaskFinalizationResult = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            deserialized,
            TaskFinalizationResult::Success { .. }
        ));
    }

    #[test]
    fn test_task_finalization_result_not_claimed() {
        let result = TaskFinalizationResult::NotClaimed {
            reason: "already finalized".to_string(),
            already_claimed_by: Some(Uuid::now_v7()),
        };

        let json = serde_json::to_value(&result).unwrap();
        assert!(json["NotClaimed"]["already_claimed_by"].is_string());
    }

    #[test]
    fn test_task_finalization_result_not_claimed_unknown_claimer() {
        let result = TaskFinalizationResult::NotClaimed {
            reason: "race condition".to_string(),
            already_claimed_by: None,
        };

        let json = serde_json::to_value(&result).unwrap();
        assert!(json["NotClaimed"]["already_claimed_by"].is_null());
    }

    #[test]
    fn test_task_finalization_result_failed() {
        let result = TaskFinalizationResult::Failed {
            error: "database error".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("database error"));
    }

    // --- OrchestrationProcessingStats ---

    #[test]
    fn test_orchestration_processing_stats_construction() {
        let stats = OrchestrationProcessingStats {
            task_requests_processed: 1000,
            step_results_processed: 5000,
            tasks_finalized: 800,
            tasks_ready_processed: 1200,
            processing_errors: 10,
        };

        assert_eq!(stats.task_requests_processed, 1000);
        assert_eq!(stats.step_results_processed, 5000);
        assert_eq!(stats.processing_errors, 10);
    }

    #[test]
    fn test_orchestration_processing_stats_serialization() {
        let stats = OrchestrationProcessingStats {
            task_requests_processed: 0,
            step_results_processed: 0,
            tasks_finalized: 0,
            tasks_ready_processed: 0,
            processing_errors: 0,
        };

        let json = serde_json::to_string(&stats).unwrap();
        let deserialized: OrchestrationProcessingStats = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.task_requests_processed, 0);
    }

    // --- AtomicProcessingStats ---

    #[test]
    fn test_atomic_processing_stats_default() {
        let stats = AtomicProcessingStats::default();
        let snapshot = stats.snapshot();
        assert_eq!(snapshot.task_requests_processed, 0);
        assert_eq!(snapshot.step_results_processed, 0);
        assert_eq!(snapshot.tasks_finalized, 0);
        assert_eq!(snapshot.tasks_ready_processed, 0);
        assert_eq!(snapshot.processing_errors, 0);
    }

    #[test]
    fn test_atomic_processing_stats_increment_and_snapshot() {
        let stats = AtomicProcessingStats::default();
        stats
            .task_requests_processed
            .fetch_add(5, std::sync::atomic::Ordering::Relaxed);
        stats
            .processing_errors
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.task_requests_processed, 5);
        assert_eq!(snapshot.processing_errors, 1);
        assert_eq!(snapshot.step_results_processed, 0);
    }

    // --- SystemHealth ---

    #[test]
    fn test_system_health_healthy_state() {
        let health = SystemHealth {
            status: "healthy".to_string(),
            database_connected: true,
            message_queues_healthy: true,
            active_processors: 4,
            circuit_breaker_open: false,
            circuit_breaker_failures: 0,
            command_channel_saturation_percent: 15.0,
            backpressure_active: false,
            queue_depth_tier: "Normal".to_string(),
            queue_depth_max: 50,
            queue_depth_worst_queue: "step_results".to_string(),
            health_evaluated: true,
        };

        assert_eq!(health.status, "healthy");
        assert!(health.database_connected);
        assert!(health.message_queues_healthy);
        assert!(!health.circuit_breaker_open);
        assert!(!health.backpressure_active);
        assert!(health.health_evaluated);
    }

    #[test]
    fn test_system_health_degraded_state() {
        let health = SystemHealth {
            status: "degraded".to_string(),
            database_connected: true,
            message_queues_healthy: false,
            active_processors: 2,
            circuit_breaker_open: false,
            circuit_breaker_failures: 3,
            command_channel_saturation_percent: 85.0,
            backpressure_active: true,
            queue_depth_tier: "Warning".to_string(),
            queue_depth_max: 5000,
            queue_depth_worst_queue: "task_requests".to_string(),
            health_evaluated: true,
        };

        assert_eq!(health.status, "degraded");
        assert!(health.backpressure_active);
        assert_eq!(health.queue_depth_tier, "Warning");
    }

    #[test]
    fn test_system_health_serialization_roundtrip() {
        let health = SystemHealth {
            status: "unhealthy".to_string(),
            database_connected: false,
            message_queues_healthy: false,
            active_processors: 0,
            circuit_breaker_open: true,
            circuit_breaker_failures: 10,
            command_channel_saturation_percent: 99.5,
            backpressure_active: true,
            queue_depth_tier: "Overflow".to_string(),
            queue_depth_max: 100000,
            queue_depth_worst_queue: "all".to_string(),
            health_evaluated: true,
        };

        let json = serde_json::to_string(&health).unwrap();
        let deserialized: SystemHealth = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.status, "unhealthy");
        assert!(!deserialized.database_connected);
        assert!(deserialized.circuit_breaker_open);
        assert_eq!(deserialized.command_channel_saturation_percent, 99.5);
    }

    #[test]
    fn test_system_health_unevaluated() {
        let health = SystemHealth {
            status: "unknown".to_string(),
            database_connected: false,
            message_queues_healthy: false,
            active_processors: 0,
            circuit_breaker_open: false,
            circuit_breaker_failures: 0,
            command_channel_saturation_percent: 0.0,
            backpressure_active: false,
            queue_depth_tier: "Unknown".to_string(),
            queue_depth_max: 0,
            queue_depth_worst_queue: String::new(),
            health_evaluated: false,
        };

        assert!(!health.health_evaluated);
        assert_eq!(health.status, "unknown");
    }
}
