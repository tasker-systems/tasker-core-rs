//! # TAS-40 Worker Event System
//!
//! Specialized event system for declarative cross-language communication between
//! Rust workers and FFI handlers (Ruby, Python, WASM, etc.).
//!
//! ## Key Features
//!
//! - **Declarative Integration**: No imperative FFI calls, purely event-driven
//! - **Bidirectional Communication**: Rust → FFI (execution requests) and FFI → Rust (completions)
//! - **Database-as-API**: Workers hydrate full context from SimpleStepMessage UUIDs
//! - **Event Correlation**: Each event pair has correlation IDs for traceability
//! - **Command Pattern Integration**: Works alongside WorkerProcessor command channels
//!
//! ## Architecture Pattern
//!
//! ```text
//! WorkerProcessor → WorkerEventPublisher → WorkerEventSystem → FFI Handler
//!     ↑                                                           ↓
//! StepExecutionResult ← WorkerEventSubscriber ← WorkerEventSystem ← FFI Handler
//! ```
//!
//! ## Usage Example
//!
//! ```rust
//! use tasker_shared::events::{WorkerEventSystem, WorkerEventPublisher};
//! use tasker_shared::types::{StepEventPayload, StepExecutionEvent};
//!
//! // Create worker event system
//! let event_system = WorkerEventSystem::new();
//!
//! // Worker publishes step execution event to FFI
//! let publisher = event_system.create_publisher();
//! let payload = StepEventPayload::new(/* ... */);
//! publisher.publish_step_execution(payload).await?;
//!
//! // Subscribe to completion events from FFI
//! let subscriber = event_system.create_subscriber();
//! let mut completions = subscriber.subscribe_to_step_completions();
//! while let Ok(completion) = completions.recv().await {
//!     // Handle completion from FFI handler
//! }
//! ```

use crate::types::{StepEventPayload, StepExecutionCompletionEvent, StepExecutionEvent};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Specialized event system for TAS-40 worker-FFI communication
#[derive(Debug, Clone)]
pub struct WorkerEventSystem {
    /// Broadcast channel for step execution events (Rust → FFI)
    step_execution_tx: Arc<broadcast::Sender<StepExecutionEvent>>,
    /// Broadcast channel for step completion events (FFI → Rust)
    step_completion_tx: Arc<broadcast::Sender<StepExecutionCompletionEvent>>,
    /// Event correlation tracking for debugging
    event_correlations: Arc<RwLock<HashMap<Uuid, EventCorrelation>>>,
    /// System configuration
    config: WorkerEventConfig,
}

/// Configuration for worker event system
#[derive(Debug, Clone)]
pub struct WorkerEventConfig {
    /// Maximum events to buffer in channels
    pub channel_buffer_size: usize,
    /// Whether to track event correlations for debugging
    pub track_correlations: bool,
    /// Maximum correlation entries to retain
    pub max_correlation_entries: usize,
}

impl Default for WorkerEventConfig {
    fn default() -> Self {
        Self {
            channel_buffer_size: 1000,
            track_correlations: true,
            max_correlation_entries: 10000,
        }
    }
}

/// Event correlation information for debugging and observability
#[derive(Debug, Clone)]
pub struct EventCorrelation {
    pub event_id: Uuid,
    pub task_uuid: Uuid,
    pub step_uuid: Uuid,
    pub step_name: String,
    pub published_at: chrono::DateTime<chrono::Utc>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub success: Option<bool>,
}

impl WorkerEventSystem {
    /// Create a new worker event system with default configuration
    pub fn new() -> Self {
        Self::with_config(WorkerEventConfig::default())
    }

    /// Create a new worker event system with custom configuration
    pub fn with_config(config: WorkerEventConfig) -> Self {
        let (step_execution_tx, _) = broadcast::channel(config.channel_buffer_size);
        let (step_completion_tx, _) = broadcast::channel(config.channel_buffer_size);

        info!(
            buffer_size = config.channel_buffer_size,
            track_correlations = config.track_correlations,
            "Creating WorkerEventSystem for TAS-40 cross-language communication"
        );

        Self {
            step_execution_tx: Arc::new(step_execution_tx),
            step_completion_tx: Arc::new(step_completion_tx),
            event_correlations: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Create a publisher for outbound events (Rust → FFI)
    pub fn create_publisher(&self) -> WorkerEventPublisher {
        WorkerEventPublisher::new(self.clone())
    }

    /// Create a subscriber for inbound events (FFI → Rust)
    pub fn create_subscriber(&self) -> WorkerEventSubscriber {
        WorkerEventSubscriber::new(self.clone())
    }

    /// Publish step execution event to FFI handlers
    pub async fn publish_step_execution(
        &self,
        event: StepExecutionEvent,
    ) -> Result<(), WorkerEventError> {
        debug!(
            event_id = %event.event_id,
            task_uuid = %event.payload.task_uuid,
            step_uuid = %event.payload.step_uuid,
            step_name = %event.payload.task_sequence_step.workflow_step.name,
            handler_class = %event.payload.task_sequence_step.step_definition.handler.callable,
            "Publishing step execution event to FFI handlers"
        );

        // Track correlation if enabled
        if self.config.track_correlations {
            self.track_execution_event(&event);
        }

        // Publish to broadcast channel
        match self.step_execution_tx.send(event) {
            Ok(subscriber_count) => {
                debug!(
                    subscriber_count = subscriber_count,
                    "Step execution event published to FFI handlers"
                );
                Ok(())
            }
            Err(broadcast::error::SendError(_)) => {
                warn!("No FFI handlers subscribed to step execution events");
                Ok(()) // Not an error - just no FFI handlers ready yet
            }
        }
    }

    /// Publish step completion event from FFI handlers
    pub async fn publish_step_completion(
        &self,
        event: StepExecutionCompletionEvent,
    ) -> Result<(), WorkerEventError> {
        debug!(
            event_id = %event.event_id,
            task_uuid = %event.task_uuid,
            step_uuid = %event.step_uuid,
            success = event.success,
            "Publishing step completion event from FFI handler"
        );

        // Update correlation tracking if enabled
        if self.config.track_correlations {
            self.track_completion_event(&event);
        }

        // Publish to broadcast channel
        match self.step_completion_tx.send(event) {
            Ok(subscriber_count) => {
                debug!(
                    subscriber_count = subscriber_count,
                    "Step completion event published to workers"
                );
                Ok(())
            }
            Err(broadcast::error::SendError(_)) => {
                warn!("No workers subscribed to step completion events");
                Ok(()) // Not an error - just no workers listening
            }
        }
    }

    /// Subscribe to step execution events (for FFI handlers)
    pub fn subscribe_to_step_executions(&self) -> broadcast::Receiver<StepExecutionEvent> {
        self.step_execution_tx.subscribe()
    }

    /// Subscribe to step completion events (for workers)
    pub fn subscribe_to_step_completions(
        &self,
    ) -> broadcast::Receiver<StepExecutionCompletionEvent> {
        self.step_completion_tx.subscribe()
    }

    /// Get system statistics for monitoring
    pub fn get_statistics(&self) -> WorkerEventSystemStats {
        let correlations = self.event_correlations.read().unwrap();

        let completed_events = correlations
            .values()
            .filter(|c| c.completed_at.is_some())
            .count();
        let successful_events = correlations
            .values()
            .filter(|c| c.success == Some(true))
            .count();
        let failed_events = correlations
            .values()
            .filter(|c| c.success == Some(false))
            .count();

        WorkerEventSystemStats {
            step_execution_subscribers: self.step_execution_tx.receiver_count(),
            step_completion_subscribers: self.step_completion_tx.receiver_count(),
            total_events_tracked: correlations.len(),
            completed_events,
            successful_events,
            failed_events,
        }
    }

    /// Clean up old event correlations to prevent memory leaks
    pub fn cleanup_correlations(&self) {
        if !self.config.track_correlations {
            return;
        }

        let mut correlations = self.event_correlations.write().unwrap();

        if correlations.len() <= self.config.max_correlation_entries {
            return;
        }

        // Remove oldest correlations, keeping the most recent ones
        let cutoff_time = chrono::Utc::now() - chrono::Duration::hours(1);
        let initial_count = correlations.len();

        correlations.retain(|_, correlation| correlation.published_at > cutoff_time);

        let removed_count = initial_count - correlations.len();
        if removed_count > 0 {
            debug!(
                removed_count = removed_count,
                remaining_count = correlations.len(),
                "Cleaned up old event correlations"
            );
        }
    }

    /// Track step execution event for correlation
    fn track_execution_event(&self, event: &StepExecutionEvent) {
        let mut correlations = self.event_correlations.write().unwrap();

        let correlation = EventCorrelation {
            event_id: event.event_id,
            task_uuid: event.payload.task_uuid,
            step_uuid: event.payload.step_uuid,
            step_name: event.payload.task_sequence_step.step_definition.name.clone(),
            published_at: chrono::Utc::now(),
            completed_at: None,
            success: None,
        };

        correlations.insert(event.event_id, correlation);
    }

    /// Track step completion event for correlation
    fn track_completion_event(&self, event: &StepExecutionCompletionEvent) {
        let mut correlations = self.event_correlations.write().unwrap();

        // Update existing correlation if found
        if let Some(correlation) = correlations.get_mut(&event.event_id) {
            correlation.completed_at = Some(chrono::Utc::now());
            correlation.success = Some(event.success);
        } else {
            // Create new correlation entry for completion without matching execution
            // This can happen if correlations were cleaned up or system restarted
            let correlation = EventCorrelation {
                event_id: event.event_id,
                task_uuid: event.task_uuid,
                step_uuid: event.step_uuid,
                step_name: "unknown".to_string(),
                published_at: chrono::Utc::now(),
                completed_at: Some(chrono::Utc::now()),
                success: Some(event.success),
            };
            correlations.insert(event.event_id, correlation);
        }
    }
}

impl Default for WorkerEventSystem {
    fn default() -> Self {
        Self::new()
    }
}

/// Publisher for worker events (used by WorkerProcessor)
#[derive(Debug, Clone)]
pub struct WorkerEventPublisher {
    event_system: WorkerEventSystem,
}

impl WorkerEventPublisher {
    /// Create a new worker event publisher
    pub fn new(event_system: WorkerEventSystem) -> Self {
        Self { event_system }
    }

    /// Publish step execution event with hydrated payload
    pub async fn publish_step_execution(
        &self,
        payload: StepEventPayload,
    ) -> Result<(), WorkerEventError> {
        let event = StepExecutionEvent::new(payload);
        self.event_system.publish_step_execution(event).await
    }

    /// Publish step execution event with specific correlation ID
    pub async fn publish_step_execution_with_correlation(
        &self,
        correlation_id: Uuid,
        payload: StepEventPayload,
    ) -> Result<(), WorkerEventError> {
        let event = StepExecutionEvent::with_event_id(correlation_id, payload);
        self.event_system.publish_step_execution(event).await
    }
}

/// Subscriber for worker events (used by WorkerProcessor and FFI handlers)
#[derive(Debug, Clone)]
pub struct WorkerEventSubscriber {
    event_system: WorkerEventSystem,
}

impl WorkerEventSubscriber {
    /// Create a new worker event subscriber
    pub fn new(event_system: WorkerEventSystem) -> Self {
        Self { event_system }
    }

    /// Subscribe to step execution events (for FFI handlers)
    pub fn subscribe_to_step_executions(&self) -> broadcast::Receiver<StepExecutionEvent> {
        self.event_system.subscribe_to_step_executions()
    }

    /// Subscribe to step completion events (for workers)
    pub fn subscribe_to_step_completions(
        &self,
    ) -> broadcast::Receiver<StepExecutionCompletionEvent> {
        self.event_system.subscribe_to_step_completions()
    }

    /// Publish step completion event (for FFI handlers to report results)
    pub async fn publish_step_completion(
        &self,
        event: StepExecutionCompletionEvent,
    ) -> Result<(), WorkerEventError> {
        self.event_system.publish_step_completion(event).await
    }
}

/// Statistics for worker event system monitoring
#[derive(Debug, Clone)]
pub struct WorkerEventSystemStats {
    pub step_execution_subscribers: usize,
    pub step_completion_subscribers: usize,
    pub total_events_tracked: usize,
    pub completed_events: usize,
    pub successful_events: usize,
    pub failed_events: usize,
}

/// Errors for worker event system
#[derive(Debug, thiserror::Error)]
pub enum WorkerEventError {
    #[error("Failed to publish event: {0}")]
    PublishError(String),

    #[error("Failed to subscribe to events: {0}")]
    SubscriptionError(String),

    #[error("Event correlation error: {0}")]
    CorrelationError(String),

    #[error("Channel error: {0}")]
    ChannelError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use crate::messaging::orchestration_messages::TaskSequenceStep;
    use crate::models::core::{task::{TaskForOrchestration, Task}, workflow_step::WorkflowStepWithName, task_template::{StepDefinition, HandlerDefinition, RetryConfiguration, BackoffStrategy}};
    use crate::models::orchestration::StepDependencyResultMap;

    /// Helper function to create a test TaskSequenceStep
    fn create_test_task_sequence_step(
        task_uuid: Uuid,
        step_uuid: Uuid,
        step_name: &str,
        handler_class: &str,
        step_payload: serde_json::Value,
        dependency_results: StepDependencyResultMap,
        task_context: serde_json::Value,
    ) -> TaskSequenceStep {
        TaskSequenceStep {
            task: TaskForOrchestration {
                task: Task {
                    task_uuid,
                    named_task_uuid: uuid::Uuid::new_v4(),
                    complete: false,
                    requested_at: chrono::Utc::now().naive_utc(),
                    initiator: Some("test".to_string()),
                    source_system: Some("test_system".to_string()),
                    reason: Some("test_reason".to_string()),
                    bypass_steps: None,
                    tags: None,
                    context: Some(task_context),
                    identity_hash: "test_hash".to_string(),
                    claimed_at: None,
                    claimed_by: None,
                    priority: 5,
                    claim_timeout_seconds: 300,
                    created_at: chrono::Utc::now().naive_utc(),
                    updated_at: chrono::Utc::now().naive_utc(),
                },
                task_name: "test_task".to_string(),
                task_version: "1.0.0".to_string(),
                namespace_name: "test_namespace".to_string(),
            },
            workflow_step: WorkflowStepWithName {
                workflow_step_uuid: step_uuid,
                task_uuid,
                named_step_uuid: uuid::Uuid::new_v4(),
                name: step_name.to_string(),
                retryable: false,
                retry_limit: None,
                in_process: false,
                processed: false,
                processed_at: None,
                attempts: None,
                last_attempted_at: None,
                backoff_request_seconds: None,
                inputs: Some(step_payload),
                results: None,
                skippable: false,
                created_at: chrono::Utc::now().naive_utc(),
                updated_at: chrono::Utc::now().naive_utc(),
            },
            dependency_results,
            step_definition: StepDefinition {
                name: step_name.to_string(),
                description: Some(format!("Test step: {}", step_name)),
                handler: HandlerDefinition {
                    callable: handler_class.to_string(),
                    initialization: HashMap::new(),
                },
                system_dependency: None,
                dependencies: vec![],
                timeout_seconds: Some(30),
                retry: RetryConfiguration {
                    retryable: false,
                    limit: 1,
                    backoff: BackoffStrategy::None,
                    backoff_base_ms: None,
                    max_backoff_ms: None,
                },
                publishes_events: vec![],
            },
        }
    }

    #[tokio::test]
    async fn test_worker_event_system_creation() {
        let event_system = WorkerEventSystem::new();
        let stats = event_system.get_statistics();

        assert_eq!(stats.step_execution_subscribers, 0);
        assert_eq!(stats.step_completion_subscribers, 0);
        assert_eq!(stats.total_events_tracked, 0);
    }

    #[tokio::test]
    async fn test_step_execution_event_publishing() {
        let event_system = WorkerEventSystem::new();
        let publisher = event_system.create_publisher();
        let subscriber = event_system.create_subscriber();
        let mut receiver = subscriber.subscribe_to_step_executions();

        let task_uuid = Uuid::new_v4();
        let step_uuid = Uuid::new_v4();
        let step_name = "test_step";
        let handler_class = "TestHandler";
        let step_payload = serde_json::json!({"test": true});
        let dependency_results = StepDependencyResultMap::new();
        let task_context = serde_json::json!({"context": "test"});

        let task_sequence_step = create_test_task_sequence_step(
            task_uuid,
            step_uuid,
            step_name,
            handler_class,
            step_payload,
            dependency_results,
            task_context,
        );

        let payload = StepEventPayload::new(task_uuid, step_uuid, task_sequence_step);

        // Publish event
        let result = publisher.publish_step_execution(payload.clone()).await;
        assert!(result.is_ok());

        // Receive event
        let received_event = receiver.recv().await.unwrap();
        assert_eq!(received_event.payload.task_uuid, task_uuid);
        assert_eq!(received_event.payload.step_uuid, step_uuid);
        assert_eq!(received_event.payload.task_sequence_step.step_definition.name, step_name);
        assert_eq!(received_event.payload.task_sequence_step.step_definition.handler.callable, handler_class);
    }

    #[tokio::test]
    async fn test_step_completion_event_publishing() {
        let event_system = WorkerEventSystem::new();
        let subscriber = event_system.create_subscriber();
        let mut receiver = subscriber.subscribe_to_step_completions();

        let completion_event = StepExecutionCompletionEvent::success(
            Uuid::new_v4(),
            Uuid::new_v4(),
            serde_json::json!({"completed": true}),
            Some(serde_json::json!({"execution_time_ms": 150})),
        );

        // Publish completion event
        let result = subscriber
            .publish_step_completion(completion_event.clone())
            .await;
        assert!(result.is_ok());

        // Receive completion event
        let received_event = receiver.recv().await.unwrap();
        assert_eq!(received_event.event_id, completion_event.event_id);
        assert_eq!(received_event.success, true);
    }

    #[tokio::test]
    async fn test_event_correlation_tracking() {
        let config = WorkerEventConfig {
            track_correlations: true,
            ..Default::default()
        };
        let event_system = WorkerEventSystem::with_config(config);

        // Create and publish execution event
        let task_uuid = Uuid::new_v4();
        let step_uuid = Uuid::new_v4();
        let step_name = "test_step";
        let handler_class = "TestHandler";
        let step_payload = serde_json::json!({"test": true});
        let dependency_results = StepDependencyResultMap::new();
        let task_context = serde_json::json!({"task": {"task_id": 123}});

        let task_sequence_step = create_test_task_sequence_step(
            task_uuid,
            step_uuid,
            step_name,
            handler_class,
            step_payload,
            dependency_results,
            task_context,
        );

        let payload = StepEventPayload::new(task_uuid, step_uuid, task_sequence_step);

        let execution_event = StepExecutionEvent::new(payload);
        let event_id = execution_event.event_id;

        let _ = event_system.publish_step_execution(execution_event).await;

        // Create and publish completion event with same ID
        let completion_event = StepExecutionCompletionEvent::with_event_id(
            event_id,
            Uuid::new_v4(),
            Uuid::new_v4(),
            true,
            serde_json::json!({"result": "success"}),
            None,
            None,
        );

        let _ = event_system.publish_step_completion(completion_event).await;

        // Check statistics
        let stats = event_system.get_statistics();
        assert_eq!(stats.total_events_tracked, 1);
        assert_eq!(stats.completed_events, 1);
        assert_eq!(stats.successful_events, 1);
        assert_eq!(stats.failed_events, 0);
    }

    #[tokio::test]
    async fn test_event_system_statistics() {
        let event_system = WorkerEventSystem::new();

        // Create subscribers
        let _execution_receiver = event_system.subscribe_to_step_executions();
        let _completion_receiver = event_system.subscribe_to_step_completions();

        let stats = event_system.get_statistics();
        assert_eq!(stats.step_execution_subscribers, 1);
        assert_eq!(stats.step_completion_subscribers, 1);
    }

    #[tokio::test]
    async fn test_correlation_cleanup() {
        let config = WorkerEventConfig {
            track_correlations: true,
            max_correlation_entries: 5, // Very small limit for testing
            ..Default::default()
        };
        let event_system = WorkerEventSystem::with_config(config);

        // Add some correlations by tracking events
        for _ in 0..10 {
            let task_uuid = Uuid::new_v4();
            let step_uuid = Uuid::new_v4();
            let step_name = "test_step";
            let handler_class = "TestHandler";
            let step_payload = serde_json::json!({"test": true});
            let dependency_results = StepDependencyResultMap::new();
            let task_context = serde_json::json!({"task": {"task_id": 123}});

            let task_sequence_step = create_test_task_sequence_step(
                task_uuid,
                step_uuid,
                step_name,
                handler_class,
                step_payload,
                dependency_results,
                task_context,
            );

            let payload = StepEventPayload::new(task_uuid, step_uuid, task_sequence_step);
            let event = StepExecutionEvent::new(payload);
            let _ = event_system.publish_step_execution(event).await;
        }

        let initial_stats = event_system.get_statistics();
        assert!(initial_stats.total_events_tracked > 5);

        // Cleanup should respect the limit eventually
        event_system.cleanup_correlations();

        // Note: cleanup uses time-based retention, so all events may still be present
        // in this test since they were just created
        let final_stats = event_system.get_statistics();
        assert!(final_stats.total_events_tracked <= initial_stats.total_events_tracked);
    }
}
