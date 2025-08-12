//! # Task Enqueuer
//!
//! Framework-agnostic task enqueuing system for delegation-based architecture.
//!
//! ## Overview
//!
//! The TaskEnqueuer provides a unified interface for enqueuing and reenqueuing tasks
//! across different framework implementations (Rails, Python, Node.js, etc.) while
//! maintaining the delegation-based architecture where Rust handles orchestration
//! decisions and frameworks handle queue management.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
//! │ Rust            │    │ Event Publisher │    │ Framework       │
//! │ Orchestration   │───▶│ (Bridge)        │───▶│ Queue Manager   │
//! │ (TaskFinalizer) │    │                 │    │ (Rails/Python)  │
//! └─────────────────┘    └─────────────────┘    └─────────────────┘
//! ```
//!
//! ## Supported Targets
//!
//! - **Rust Native**: Direct async queue implementations
//! - **FFI**: C-compatible interface for Ruby, Python, etc.
//! - **WASM**: WebAssembly bindings for browser/Node.js
//! - **JNI**: Java Native Interface for JVM languages
//! - **Event Publishing**: Framework-agnostic event broadcasting
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tasker_core::orchestration::{TaskEnqueuer, EnqueueRequest, EnqueueHandler, EnqueuePriority};
//! use tasker_core::models::Task;
//! use sqlx::PgPool;
//!
//! # async fn example(pool: PgPool, task: Task) -> Result<(), Box<dyn std::error::Error>> {
//! let enqueuer = TaskEnqueuer::new(pool);
//!
//! let request = EnqueueRequest::new(task)
//!     .with_delay(30)
//!     .with_reason("Ready steps available")
//!     .with_priority(EnqueuePriority::Normal);
//!
//! enqueuer.enqueue(request).await?;
//! # Ok(())
//! # }
//! ```

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::PgPool;
use std::collections::HashMap;

use crate::events::publisher::EventPublisher;
use crate::models::Task;

/// Priority levels for task enqueuing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnqueuePriority {
    /// Critical priority - process immediately
    Critical,
    /// High priority - process before normal tasks
    High,
    /// Normal priority - standard processing
    Normal,
    /// Low priority - process after normal tasks
    Low,
}

impl Default for EnqueuePriority {
    fn default() -> Self {
        Self::Normal
    }
}

/// Type of enqueue operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnqueueOperation {
    /// Initial enqueue of a new task
    Enqueue,
    /// Re-enqueue an existing task
    Reenqueue,
    /// Delayed enqueue/reenqueue
    DelayedEnqueue,
}

/// Comprehensive enqueue request with task data and metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnqueueRequest {
    /// The task to enqueue
    pub task: Task,
    /// Type of enqueue operation
    pub operation: EnqueueOperation,
    /// Priority level for queue ordering
    pub priority: EnqueuePriority,
    /// Delay in seconds before processing (0 = immediate)
    pub delay_seconds: u32,
    /// Human-readable reason for enqueuing
    pub reason: String,
    /// Additional metadata for framework-specific handling
    pub metadata: HashMap<String, Value>,
    /// Timestamp when enqueue was requested
    pub requested_at: DateTime<Utc>,
    /// Queue name override (if different from default)
    pub queue_name: Option<String>,
    /// Maximum retry attempts for this enqueue operation
    pub max_retries: Option<u32>,
}

impl EnqueueRequest {
    /// Create a new enqueue request for a task
    pub fn new(task: Task) -> Self {
        Self {
            task,
            operation: EnqueueOperation::Enqueue,
            priority: EnqueuePriority::default(),
            delay_seconds: 0,
            reason: "Task ready for processing".to_string(),
            metadata: HashMap::new(),
            requested_at: Utc::now(),
            queue_name: None,
            max_retries: None,
        }
    }

    /// Create a reenqueue request
    pub fn reenqueue(task: Task) -> Self {
        Self {
            task,
            operation: EnqueueOperation::Reenqueue,
            priority: EnqueuePriority::default(),
            delay_seconds: 0,
            reason: "Task reenqueued for processing".to_string(),
            metadata: HashMap::new(),
            requested_at: Utc::now(),
            queue_name: None,
            max_retries: None,
        }
    }

    /// Set delay for enqueue operation
    pub fn with_delay(mut self, delay_seconds: u32) -> Self {
        self.delay_seconds = delay_seconds;
        if delay_seconds > 0 {
            self.operation = EnqueueOperation::DelayedEnqueue;
        }
        self
    }

    /// Set priority level
    pub fn with_priority(mut self, priority: EnqueuePriority) -> Self {
        self.priority = priority;
        self
    }

    /// Set reason for enqueuing
    pub fn with_reason<S: Into<String>>(mut self, reason: S) -> Self {
        self.reason = reason.into();
        self
    }

    /// Add metadata
    pub fn with_metadata<K: Into<String>>(mut self, key: K, value: Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }

    /// Set queue name override
    pub fn with_queue<S: Into<String>>(mut self, queue_name: S) -> Self {
        self.queue_name = Some(queue_name.into());
        self
    }

    /// Set maximum retry attempts
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = Some(max_retries);
        self
    }

    /// Check if this is a delayed enqueue
    pub fn is_delayed(&self) -> bool {
        self.delay_seconds > 0
    }

    /// Calculate when the task should be processed
    pub fn process_at(&self) -> DateTime<Utc> {
        self.requested_at + chrono::Duration::seconds(self.delay_seconds as i64)
    }
}

/// Result of an enqueue operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnqueueResult {
    /// Task UUID that was enqueued
    pub task_uuid: uuid::Uuid,
    /// Operation that was performed
    pub operation: EnqueueOperation,
    /// Queue the task was added to
    pub queue_name: String,
    /// Job ID from the queue system (if available)
    pub job_id: Option<String>,
    /// Estimated processing time
    pub process_at: DateTime<Utc>,
    /// Whether the operation was successful
    pub success: bool,
    /// Additional result metadata
    pub metadata: HashMap<String, Value>,
}

/// Errors that can occur during enqueue operations
#[derive(Debug, thiserror::Error)]
pub enum EnqueueError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Event publishing error: {0}")]
    EventPublishing(String),

    #[error("Handler error: {0}")]
    Handler(String),

    #[error("Queue unavailable: {0}")]
    QueueUnavailable(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Timeout waiting for enqueue: {0}")]
    Timeout(String),
}

/// Trait for implementing different enqueue handlers
///
/// This trait allows for multiple implementations targeting different frameworks:
/// - Direct Rust implementations
/// - FFI bridges to other languages
/// - Event-based systems
/// - Test mocks
#[async_trait]
pub trait EnqueueHandler: Send + Sync {
    /// Handle an enqueue request
    async fn handle_enqueue(&self, request: EnqueueRequest) -> Result<EnqueueResult, EnqueueError>;

    /// Get the handler name for identification
    fn handler_name(&self) -> &'static str;

    /// Check if the handler is available/healthy
    async fn is_available(&self) -> bool {
        true // Default implementation
    }

    /// Get supported queue names
    fn supported_queues(&self) -> Vec<String> {
        vec!["default".to_string()] // Default implementation
    }
}

/// Event-based enqueue handler using EventPublisher
///
/// This handler publishes enqueue events to the EventPublisher system,
/// allowing framework implementations to subscribe and handle the actual
/// queue operations.
#[derive(Debug, Clone)]
pub struct EventBasedEnqueueHandler {
    event_publisher: EventPublisher,
    handler_id: String,
}

impl EventBasedEnqueueHandler {
    /// Create a new event-based handler
    pub fn new(event_publisher: EventPublisher) -> Self {
        Self {
            event_publisher,
            handler_id: "event_based".to_string(),
        }
    }

    /// Create with custom handler ID
    pub fn with_id<S: Into<String>>(event_publisher: EventPublisher, handler_id: S) -> Self {
        Self {
            event_publisher,
            handler_id: handler_id.into(),
        }
    }
}

#[async_trait]
impl EnqueueHandler for EventBasedEnqueueHandler {
    async fn handle_enqueue(&self, request: EnqueueRequest) -> Result<EnqueueResult, EnqueueError> {
        let event_name = format!(
            "task.{}",
            match request.operation {
                EnqueueOperation::Enqueue => "enqueue",
                EnqueueOperation::Reenqueue => "reenqueue",
                EnqueueOperation::DelayedEnqueue => "delayed_enqueue",
            }
        );

        let event_data = serde_json::json!({
            "handler_id": self.handler_id,
            "request": request,
            "published_at": Utc::now()
        });

        self.event_publisher
            .publish(&event_name, event_data)
            .await
            .map_err(|_| {
                EnqueueError::EventPublishing("Failed to publish enqueue event".to_string())
            })?;

        // For event-based handling, we return a result indicating the event was published
        // The actual enqueue result would come from the framework subscriber
        let task_uuid = request.task.task_uuid;
        let operation = request.operation;
        let queue_name = request
            .queue_name
            .clone()
            .unwrap_or_else(|| "default".to_string());
        let process_at = request.process_at();

        Ok(EnqueueResult {
            task_uuid,
            operation,
            queue_name,
            job_id: None, // Would be set by framework handler
            process_at,
            success: true,
            metadata: HashMap::from([
                ("handler".into(), Value::String(self.handler_id.clone())),
                ("event_published".into(), Value::Bool(true)),
            ]),
        })
    }

    fn handler_name(&self) -> &'static str {
        "EventBasedEnqueueHandler"
    }

    fn supported_queues(&self) -> Vec<String> {
        vec![
            "default".to_string(),
            "high_priority".to_string(),
            "low_priority".to_string(),
        ]
    }
}

/// Direct Rust enqueue handler for native implementations
///
/// This handler provides a direct Rust implementation that can be used
/// for native Rust queue systems or as a foundation for other implementations.
#[derive(Debug, Clone)]
pub struct DirectEnqueueHandler {}

impl DirectEnqueueHandler {
    /// Create a new direct handler
    pub fn new(_pool: PgPool) -> Self {
        Self {}
    }
}

#[async_trait]
impl EnqueueHandler for DirectEnqueueHandler {
    async fn handle_enqueue(&self, request: EnqueueRequest) -> Result<EnqueueResult, EnqueueError> {
        // Direct implementation - logs enqueue operations without external queue system
        // This serves as a fallback handler when event-based delegation is not sufficient

        use tracing::info;

        info!(
            target: "task_enqueuer",
            task_uuid = %request.task.task_uuid,
            operation = ?request.operation,
            queue = %request.queue_name.as_deref().unwrap_or("default"),
            delay_seconds = request.delay_seconds,
            reason = %request.reason,
            "DirectEnqueueHandler processing enqueue request"
        );

        // This implementation provides the EnqueueHandler interface for systems that
        // prefer direct coordination rather than event-based delegation.
        // Future enhancements could integrate with:
        // - tokio-cron-scheduler for delayed jobs
        // - Redis/database-backed queues
        // - In-memory priority queues

        let task_uuid = request.task.task_uuid;
        let operation = request.operation;
        let queue_name = request
            .queue_name
            .clone()
            .unwrap_or_else(|| "default".to_string());
        let process_at = request.process_at();

        Ok(EnqueueResult {
            task_uuid,
            operation,
            queue_name,
            job_id: Some(format!("rust-job-{task_uuid}")),
            process_at,
            success: true,
            metadata: HashMap::from([
                ("handler".into(), Value::String("direct".to_string())),
                (
                    "implementation".into(),
                    Value::String("rust_native".to_string()),
                ),
            ]),
        })
    }

    fn handler_name(&self) -> &'static str {
        "DirectEnqueueHandler"
    }

    fn supported_queues(&self) -> Vec<String> {
        vec![
            "default".to_string(),
            "critical".to_string(),
            "background".to_string(),
        ]
    }
}

/// Main task enqueuer that orchestrates different handlers
pub struct TaskEnqueuer {
    event_publisher: EventPublisher,
    handlers: Vec<Box<dyn EnqueueHandler>>,
    default_handler: String,
}

impl TaskEnqueuer {
    /// Create a new task enqueuer with default handlers
    pub fn new(pool: PgPool) -> Self {
        let event_publisher = EventPublisher::with_capacity(1000);
        let handlers: Vec<Box<dyn EnqueueHandler>> = vec![
            Box::new(EventBasedEnqueueHandler::new(event_publisher.clone())),
            Box::new(DirectEnqueueHandler::new(pool.clone())),
        ];

        Self {
            event_publisher,
            handlers,
            default_handler: "EventBasedEnqueueHandler".to_string(),
        }
    }

    /// Create with custom event publisher
    pub fn with_event_publisher(pool: PgPool, event_publisher: EventPublisher) -> Self {
        let handlers: Vec<Box<dyn EnqueueHandler>> = vec![
            Box::new(EventBasedEnqueueHandler::new(event_publisher.clone())),
            Box::new(DirectEnqueueHandler::new(pool.clone())),
        ];

        Self {
            event_publisher,
            handlers,
            default_handler: "EventBasedEnqueueHandler".to_string(),
        }
    }

    /// Add a custom handler
    pub fn add_handler(&mut self, handler: Box<dyn EnqueueHandler>) {
        self.handlers.push(handler);
    }

    /// Set the default handler by name
    pub fn set_default_handler<S: Into<String>>(&mut self, handler_name: S) {
        self.default_handler = handler_name.into();
    }

    /// Enqueue a task using the default handler
    pub async fn enqueue(&self, request: EnqueueRequest) -> Result<EnqueueResult, EnqueueError> {
        self.enqueue_with_handler(request, &self.default_handler)
            .await
    }

    /// Enqueue a task using a specific handler
    pub async fn enqueue_with_handler(
        &self,
        request: EnqueueRequest,
        handler_name: &str,
    ) -> Result<EnqueueResult, EnqueueError> {
        let handler = self
            .handlers
            .iter()
            .find(|h| h.handler_name() == handler_name)
            .ok_or_else(|| EnqueueError::Handler(format!("Handler '{handler_name}' not found")))?;

        handler.handle_enqueue(request).await
    }

    /// List available handlers
    pub fn available_handlers(&self) -> Vec<&'static str> {
        self.handlers.iter().map(|h| h.handler_name()).collect()
    }

    /// Get the event publisher for framework integration
    pub fn event_publisher(&self) -> &EventPublisher {
        &self.event_publisher
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Task;
    use uuid::Uuid;

    #[test]
    fn test_enqueue_request_creation() {
        let task = Task {
            task_uuid: Uuid::new_v4(),
            named_task_uuid: Uuid::new_v4(),
            complete: false,
            requested_at: Utc::now().naive_utc(),
            initiator: Some("test".to_string()),
            source_system: Some("test_system".to_string()),
            reason: None,
            bypass_steps: None,
            tags: None,
            context: None,
            identity_hash: "test_hash".to_string(),
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
            claimed_at: None,
            claimed_by: None,
            priority: 0,
            claim_timeout_seconds: 60,
        };

        let request = EnqueueRequest::new(task.clone())
            .with_delay(30)
            .with_priority(EnqueuePriority::High)
            .with_reason("Test enqueue")
            .with_queue("test_queue");

        assert!(request.task.task_uuid.to_string().len() > 0);
        assert_eq!(request.delay_seconds, 30);
        assert_eq!(request.priority, EnqueuePriority::High);
        assert_eq!(request.reason, "Test enqueue");
        assert_eq!(request.queue_name, Some("test_queue".to_string()));
        assert_eq!(request.operation, EnqueueOperation::DelayedEnqueue);
        assert!(request.is_delayed());
    }

    #[test]
    fn test_enqueue_priority_default() {
        assert_eq!(EnqueuePriority::default(), EnqueuePriority::Normal);
    }

    #[test]
    fn test_reenqueue_request() {
        let task = Task {
            task_uuid: Uuid::new_v4(),
            named_task_uuid: Uuid::new_v4(),
            complete: false,
            requested_at: Utc::now().naive_utc(),
            initiator: None,
            source_system: None,
            reason: None,
            bypass_steps: None,
            tags: None,
            context: None,
            identity_hash: "reenqueue_test_hash".to_string(),
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
            claimed_at: None,
            claimed_by: None,
            priority: 0,
            claim_timeout_seconds: 60,
        };

        let request = EnqueueRequest::reenqueue(task);
        assert_eq!(request.operation, EnqueueOperation::Reenqueue);
        assert_eq!(request.reason, "Task reenqueued for processing");
    }
}
