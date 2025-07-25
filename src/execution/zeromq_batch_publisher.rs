//! # ZeroMQ Batch Publisher
//!
//! Publishes step execution batches to Ruby BatchStepExecutionOrchestrator
//! using the dual result pattern architecture.
//!
//! This component is owned by the OrchestrationSystem and provides the Rust side
//! of the ZeroMQ pub-sub communication pattern.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tracing::{debug, error, info, warn};
use zmq::{Context, Socket, SocketType};

/// Configuration for ZeroMQ batch publishing
#[derive(Clone, Debug)]
pub struct BatchPublisherConfig {
    /// Endpoint to bind for publishing batches (e.g., "inproc://batches")
    pub batch_endpoint: String,
    /// Endpoint to connect for receiving results (e.g., "inproc://results")
    pub result_endpoint: String,
    /// High water mark for publishing socket
    pub send_hwm: i32,
    /// High water mark for receiving socket
    pub recv_hwm: i32,
}

impl Default for BatchPublisherConfig {
    fn default() -> Self {
        Self {
            batch_endpoint: "tcp://127.0.0.1:5555".to_string(),
            result_endpoint: "tcp://127.0.0.1:5556".to_string(),
            send_hwm: 1000,
            recv_hwm: 1000,
        }
    }
}

/// Batch message sent to Ruby BatchStepExecutionOrchestrator
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BatchMessage {
    pub batch_id: String,
    pub protocol_version: String,
    pub created_at: String,
    pub steps: Vec<StepData>,
}

/// Individual step data within a batch
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StepData {
    pub step_id: i64,
    pub task_id: i64,
    pub step_name: String,
    pub handler_class: String,
    pub handler_config: serde_json::Value,
    pub task_context: serde_json::Value,
    pub previous_results: serde_json::Value,
    pub metadata: serde_json::Value,
}

/// Result message received from Ruby BatchStepExecutionOrchestrator
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ResultMessage {
    pub message_type: String, // "partial_result" or "batch_completion"
    pub batch_id: String,
    pub step_id: Option<i64>,
    pub status: Option<String>,
    pub output: Option<serde_json::Value>,
    pub execution_time_ms: Option<i64>,
    pub worker_id: Option<String>,
    pub error: Option<serde_json::Value>,
    pub retryable: Option<bool>,

    // Batch completion fields
    pub total_steps: Option<usize>,
    pub completed_steps: Option<usize>,
    pub failed_steps: Option<usize>,
    pub step_summaries: Option<Vec<serde_json::Value>>,
    pub completed_at: Option<String>,
}

/// ZeroMQ batch publisher for communicating with Ruby orchestrator
pub struct BatchPublisher {
    /// ZMQ context - currently unused but needed for future socket management and cleanup
    #[allow(dead_code)]
    context: Arc<Context>,
    batch_socket: Arc<Mutex<Socket>>,
    result_socket: Arc<Mutex<Socket>>,
    config: BatchPublisherConfig,
}

// SAFETY: BatchPublisher is safe to send/sync because all ZMQ operations are protected by Mutex
unsafe impl Send for BatchPublisher {}
unsafe impl Sync for BatchPublisher {}

impl BatchPublisher {
    /// Create a new batch publisher with shared ZMQ context
    pub fn new(context: Arc<Context>, config: BatchPublisherConfig) -> Result<Self, zmq::Error> {
        info!(
            "🚀 Creating BatchPublisher with endpoints: batch={}, result={}",
            config.batch_endpoint, config.result_endpoint
        );

        // Create batch publishing socket (PUB) - Rust owns this
        let batch_socket = context.socket(SocketType::PUB)?;
        batch_socket.set_sndhwm(config.send_hwm)?;

        // Bind to batch endpoint - Ruby will connect to this
        batch_socket.bind(&config.batch_endpoint)?;
        info!("✅ Batch socket bound to: {}", config.batch_endpoint);

        // Create result subscribing socket (SUB) - connects to Ruby's PUB socket
        let result_socket = context.socket(SocketType::SUB)?;
        result_socket.set_rcvhwm(config.recv_hwm)?;

        // Subscribe to all result message types
        result_socket.set_subscribe(b"partial_result")?;
        result_socket.set_subscribe(b"batch_completion")?;
        result_socket.set_subscribe(b"batch_error")?;

        // Connect to Ruby result endpoint
        result_socket.connect(&config.result_endpoint)?;
        info!("✅ Result socket connected to: {}", config.result_endpoint);

        // Allow sockets to establish connections
        std::thread::sleep(std::time::Duration::from_millis(100));

        Ok(Self {
            context,
            batch_socket: Arc::new(Mutex::new(batch_socket)),
            result_socket: Arc::new(Mutex::new(result_socket)),
            config,
        })
    }

    /// Publish a batch message to Ruby BatchStepExecutionOrchestrator
    pub fn publish_batch(&self, batch: BatchMessage) -> Result<(), zmq::Error> {
        let message_json = serde_json::to_string(&batch).map_err(|e| {
            error!("Failed to serialize batch message: {}", e);
            zmq::Error::EPROTO
        })?;

        let full_message = format!("steps {message_json}");

        debug!(
            "📤 Publishing batch {} with {} steps ({} bytes)",
            batch.batch_id,
            batch.steps.len(),
            full_message.len()
        );

        let batch_socket = self.batch_socket.lock().unwrap();
        batch_socket.send(&full_message, 0)?;

        info!("✅ Published batch {} successfully", batch.batch_id);

        Ok(())
    }

    /// Receive result messages from Ruby (non-blocking)
    pub fn receive_result(&self) -> Result<Option<ResultMessage>, zmq::Error> {
        let result_socket = self.result_socket.lock().unwrap();
        match result_socket.recv_string(zmq::DONTWAIT) {
            Ok(Ok(message)) => {
                debug!(
                    "📥 Received result message: {}",
                    &message[..100.min(message.len())]
                );

                // Parse the message (format: "topic json_data")
                if let Some((_topic, json_data)) = message.split_once(' ') {
                    match serde_json::from_str::<ResultMessage>(json_data) {
                        Ok(result) => {
                            debug!(
                                "✅ Parsed result message: type={}, batch_id={}",
                                result.message_type, result.batch_id
                            );
                            Ok(Some(result))
                        }
                        Err(e) => {
                            warn!("Failed to parse result message JSON: {}", e);
                            Err(zmq::Error::EPROTO)
                        }
                    }
                } else {
                    warn!("Invalid result message format: missing topic separator");
                    Err(zmq::Error::EPROTO)
                }
            }
            Ok(Err(_bytes)) => {
                warn!("Received non-UTF8 result message");
                Err(zmq::Error::EPROTO)
            }
            Err(zmq::Error::EAGAIN) => {
                // No message available (non-blocking)
                Ok(None)
            }
            Err(e) => {
                error!("Error receiving result message: {}", e);
                Err(e)
            }
        }
    }

    /// Get configuration for this publisher
    pub fn config(&self) -> &BatchPublisherConfig {
        &self.config
    }

    /// Check if sockets are connected and operational
    pub fn health_check(&self) -> bool {
        // Simple health check - could be enhanced with actual connectivity tests
        true
    }
}

impl Drop for BatchPublisher {
    fn drop(&mut self) {
        debug!("🧹 Cleaning up BatchPublisher sockets");
        // ZMQ sockets are automatically cleaned up when dropped
        // The explicit close() method doesn't exist in current zmq crate version
    }
}

/// Builder for BatchPublisher to make construction easier
pub struct BatchPublisherBuilder {
    config: BatchPublisherConfig,
}

impl BatchPublisherBuilder {
    pub fn new() -> Self {
        Self {
            config: BatchPublisherConfig::default(),
        }
    }

    pub fn batch_endpoint(mut self, endpoint: String) -> Self {
        self.config.batch_endpoint = endpoint;
        self
    }

    pub fn result_endpoint(mut self, endpoint: String) -> Self {
        self.config.result_endpoint = endpoint;
        self
    }

    pub fn send_hwm(mut self, hwm: i32) -> Self {
        self.config.send_hwm = hwm;
        self
    }

    pub fn recv_hwm(mut self, hwm: i32) -> Self {
        self.config.recv_hwm = hwm;
        self
    }

    pub fn build(self, context: Arc<Context>) -> Result<BatchPublisher, zmq::Error> {
        BatchPublisher::new(context, self.config)
    }
}

impl Default for BatchPublisherBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_message_serialization() {
        let batch = BatchMessage {
            batch_id: "test_batch_123".to_string(),
            protocol_version: "2.0".to_string(),
            created_at: "2025-01-24T12:00:00Z".to_string(),
            steps: vec![StepData {
                step_id: 1001,
                task_id: 500,
                step_name: "validate_order".to_string(),
                handler_class: "OrderValidator".to_string(),
                handler_config: serde_json::json!({"timeout_seconds": 30}),
                task_context: serde_json::json!({"order_id": "order_123"}),
                previous_results: serde_json::json!({}),
                metadata: serde_json::json!({"sequence": 1, "total_steps": 2}),
            }],
        };

        let serialized = serde_json::to_string(&batch).unwrap();
        let deserialized: BatchMessage = serde_json::from_str(&serialized).unwrap();

        assert_eq!(batch.batch_id, deserialized.batch_id);
        assert_eq!(batch.steps.len(), deserialized.steps.len());
        assert_eq!(batch.steps[0].step_id, deserialized.steps[0].step_id);
    }

    #[test]
    fn test_result_message_parsing() {
        let result_json = r#"{
            "message_type": "partial_result",
            "batch_id": "test_batch_123", 
            "step_id": 1001,
            "status": "completed",
            "output": {"result": "success"},
            "execution_time_ms": 150,
            "worker_id": "worker_1"
        }"#;

        let result: ResultMessage = serde_json::from_str(result_json).unwrap();

        assert_eq!(result.message_type, "partial_result");
        assert_eq!(result.batch_id, "test_batch_123");
        assert_eq!(result.step_id, Some(1001));
        assert_eq!(result.status, Some("completed".to_string()));
    }
}
