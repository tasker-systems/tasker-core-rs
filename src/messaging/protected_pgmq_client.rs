//! # Circuit Breaker Protected PGMQ Client
//!
//! Wraps the standard PgmqClient with circuit breaker protection for fault tolerance.
//! This prevents cascade failures when queue operations encounter issues.

use crate::messaging::{PgmqClient, PgmqStepMessage, QueueMetrics};
use crate::resilience::{CircuitBreakerManager, CircuitBreakerError};
use crate::config::CircuitBreakerConfig;
use pgmq::types::Message;
use std::sync::Arc;
use tracing::warn;

/// PGMQ client with circuit breaker protection
#[derive(Debug, Clone)]
pub struct ProtectedPgmqClient {
    /// Underlying PGMQ client
    client: PgmqClient,
    
    /// Circuit breaker manager for fault tolerance
    circuit_manager: Arc<CircuitBreakerManager>,
    
    /// Component name for circuit breaker identification
    component_name: String,
}

impl ProtectedPgmqClient {
    /// Create new protected PGMQ client
    pub async fn new(
        database_url: &str,
        circuit_manager: Arc<CircuitBreakerManager>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client = PgmqClient::new(database_url).await?;
        
        Ok(Self {
            client,
            circuit_manager,
            component_name: "pgmq".to_string(),
        })
    }
    
    /// Create new protected PGMQ client with existing pool
    pub async fn new_with_pool(
        pool: sqlx::PgPool,
        circuit_manager: Arc<CircuitBreakerManager>,
    ) -> Self {
        let client = PgmqClient::new_with_pool(pool).await;
        
        Self {
            client,
            circuit_manager,
            component_name: "pgmq".to_string(),
        }
    }
    
    /// Create new protected PGMQ client with circuit breaker configuration
    pub async fn new_with_config(
        database_url: &str,
        circuit_config: &CircuitBreakerConfig,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client = PgmqClient::new(database_url).await?;
        let circuit_manager = Arc::new(CircuitBreakerManager::from_config(circuit_config));
        
        Ok(Self {
            client,
            circuit_manager,
            component_name: "pgmq".to_string(),
        })
    }
    
    /// Create new protected PGMQ client with existing pool and circuit breaker configuration
    pub async fn new_with_pool_and_config(
        pool: sqlx::PgPool,
        circuit_config: &CircuitBreakerConfig,
    ) -> Self {
        let client = PgmqClient::new_with_pool(pool).await;
        let circuit_manager = Arc::new(CircuitBreakerManager::from_config(circuit_config));
        
        Self {
            client,
            circuit_manager,
            component_name: "pgmq".to_string(),
        }
    }
    
    /// Create queue with circuit breaker protection
    pub async fn create_queue(
        &self,
        queue_name: &str,
    ) -> Result<(), CircuitBreakerError<Box<dyn std::error::Error + Send + Sync>>> {
        let circuit_breaker = self.circuit_manager.get_circuit_breaker(&self.component_name).await;
        
        circuit_breaker.call(|| async {
            self.client.create_queue(queue_name).await
        }).await
    }
    
    /// Send message with circuit breaker protection
    pub async fn send_message(
        &self,
        queue_name: &str,
        message: &PgmqStepMessage,
    ) -> Result<i64, CircuitBreakerError<Box<dyn std::error::Error + Send + Sync>>> {
        let circuit_breaker = self.circuit_manager.get_circuit_breaker(&self.component_name).await;
        
        circuit_breaker.call(|| async {
            self.client.send_message(queue_name, message).await
        }).await
    }
    
    /// Send generic JSON message with circuit breaker protection
    pub async fn send_json_message<T: serde::Serialize + Clone>(
        &self,
        queue_name: &str,
        message: &T,
    ) -> Result<i64, CircuitBreakerError<Box<dyn std::error::Error + Send + Sync>>> {
        let circuit_breaker = self.circuit_manager.get_circuit_breaker(&self.component_name).await;
        let message_clone = message.clone();
        
        circuit_breaker.call(move || async move {
            self.client.send_json_message(queue_name, &message_clone).await
        }).await
    }
    
    /// Read messages with circuit breaker protection
    pub async fn read_messages(
        &self,
        queue_name: &str,
        visibility_timeout: Option<i32>,
        qty: Option<i32>,
    ) -> Result<Vec<Message<serde_json::Value>>, CircuitBreakerError<Box<dyn std::error::Error + Send + Sync>>> {
        let circuit_breaker = self.circuit_manager.get_circuit_breaker(&self.component_name).await;
        
        circuit_breaker.call(|| async {
            self.client.read_messages(queue_name, visibility_timeout, qty).await
        }).await
    }
    
    /// Delete message with circuit breaker protection
    pub async fn delete_message(
        &self,
        queue_name: &str,
        message_id: i64,
    ) -> Result<(), CircuitBreakerError<Box<dyn std::error::Error + Send + Sync>>> {
        let circuit_breaker = self.circuit_manager.get_circuit_breaker(&self.component_name).await;
        
        circuit_breaker.call(|| async {
            self.client.delete_message(queue_name, message_id).await
        }).await
    }
    
    /// Archive message with circuit breaker protection
    pub async fn archive_message(
        &self,
        queue_name: &str,
        message_id: i64,
    ) -> Result<(), CircuitBreakerError<Box<dyn std::error::Error + Send + Sync>>> {
        let circuit_breaker = self.circuit_manager.get_circuit_breaker(&self.component_name).await;
        
        circuit_breaker.call(|| async {
            self.client.archive_message(queue_name, message_id).await
        }).await
    }
    
    /// Purge queue with circuit breaker protection
    pub async fn purge_queue(
        &self,
        queue_name: &str,
    ) -> Result<u64, CircuitBreakerError<Box<dyn std::error::Error + Send + Sync>>> {
        let circuit_breaker = self.circuit_manager.get_circuit_breaker(&self.component_name).await;
        
        circuit_breaker.call(|| async {
            self.client.purge_queue(queue_name).await
        }).await
    }
    
    /// Drop queue with circuit breaker protection
    pub async fn drop_queue(
        &self,
        queue_name: &str,
    ) -> Result<(), CircuitBreakerError<Box<dyn std::error::Error + Send + Sync>>> {
        let circuit_breaker = self.circuit_manager.get_circuit_breaker(&self.component_name).await;
        
        circuit_breaker.call(|| async {
            self.client.drop_queue(queue_name).await
        }).await
    }
    
    /// Get queue metrics with circuit breaker protection
    pub async fn queue_metrics(
        &self,
        queue_name: &str,
    ) -> Result<QueueMetrics, CircuitBreakerError<Box<dyn std::error::Error + Send + Sync>>> {
        let circuit_breaker = self.circuit_manager.get_circuit_breaker(&self.component_name).await;
        
        circuit_breaker.call(|| async {
            self.client.queue_metrics(queue_name).await
        }).await
    }
    
    /// Initialize namespace queues with circuit breaker protection
    pub async fn initialize_namespace_queues(
        &self,
        namespaces: &[&str],
    ) -> Result<(), CircuitBreakerError<Box<dyn std::error::Error + Send + Sync>>> {
        let circuit_breaker = self.circuit_manager.get_circuit_breaker(&self.component_name).await;
        
        circuit_breaker.call(|| async {
            self.client.initialize_namespace_queues(namespaces).await
        }).await
    }
    
    /// Get circuit breaker metrics for monitoring
    pub async fn circuit_breaker_metrics(&self) -> Option<crate::resilience::CircuitBreakerMetrics> {
        self.circuit_manager.get_component_metrics(&self.component_name).await
    }
    
    /// Get circuit breaker state
    pub async fn circuit_breaker_state(&self) -> crate::resilience::CircuitState {
        let circuit_breaker = self.circuit_manager.get_circuit_breaker(&self.component_name).await;
        circuit_breaker.state()
    }
    
    /// Force circuit breaker open (emergency stop)
    pub async fn force_circuit_open(&self) {
        let circuit_breaker = self.circuit_manager.get_circuit_breaker(&self.component_name).await;
        circuit_breaker.force_open().await;
        warn!("🚨 PGMQ circuit breaker forced open");
    }
    
    /// Force circuit breaker closed (emergency recovery)
    pub async fn force_circuit_closed(&self) {
        let circuit_breaker = self.circuit_manager.get_circuit_breaker(&self.component_name).await;
        circuit_breaker.force_closed().await;
        warn!("🚨 PGMQ circuit breaker forced closed");
    }
    
    /// Check if PGMQ operations are currently healthy
    pub async fn is_healthy(&self) -> bool {
        let circuit_breaker = self.circuit_manager.get_circuit_breaker(&self.component_name).await;
        circuit_breaker.is_healthy().await
    }
    
    /// Get access to underlying client for operations that don't need protection
    pub fn inner(&self) -> &PgmqClient {
        &self.client
    }
}

/// Convenience type alias for protected PGMQ errors
pub type ProtectedPgmqError = CircuitBreakerError<Box<dyn std::error::Error + Send + Sync>>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resilience::CircuitBreakerManager;
    use crate::config::{CircuitBreakerConfig, CircuitBreakerGlobalSettings, CircuitBreakerComponentConfig};
    use std::collections::HashMap;
    
    fn create_test_config() -> CircuitBreakerConfig {
        CircuitBreakerConfig {
            enabled: true,
            global_settings: CircuitBreakerGlobalSettings {
                max_circuit_breakers: 50,
                metrics_collection_interval_seconds: 30,
                auto_create_enabled: true,
                min_state_transition_interval_seconds: 1.0,
            },
            default_config: CircuitBreakerComponentConfig {
                failure_threshold: 5,
                timeout_seconds: 30,
                success_threshold: 2,
            },
            component_configs: HashMap::new(),
        }
    }

    #[tokio::test]
    #[ignore] // Requires PostgreSQL with pgmq extension
    async fn test_protected_client_creation() {
        let config = create_test_config();
        let database_url = "postgresql://tasker:tasker@localhost:5432/tasker_rust_test";
        let protected_client = ProtectedPgmqClient::new_with_config(database_url, &config).await;
        
        assert!(protected_client.is_ok());
        
        let client = protected_client.unwrap();
        assert_eq!(client.circuit_breaker_state().await, crate::resilience::CircuitState::Closed);
        assert!(client.is_healthy().await);
    }
    
    #[tokio::test]
    async fn test_circuit_breaker_integration() {
        // Create test configuration with fast timeouts
        let config = CircuitBreakerConfig {
            enabled: true,
            global_settings: CircuitBreakerGlobalSettings {
                max_circuit_breakers: 10,
                metrics_collection_interval_seconds: 1,
                auto_create_enabled: true,
                min_state_transition_interval_seconds: 0.05,
            },
            default_config: CircuitBreakerComponentConfig {
                failure_threshold: 1,
                timeout_seconds: 1, // 1 second - u64 required
                success_threshold: 1,
            },
            component_configs: HashMap::new(),
        };
        
        let circuit_manager = Arc::new(CircuitBreakerManager::from_config(&config));
        
        // Create mock pool for testing
        let pool = sqlx::PgPool::connect("sqlite::memory:").await.unwrap();
        let protected_client = ProtectedPgmqClient::new_with_pool(pool, circuit_manager).await;
        
        // Initially should be healthy
        assert!(protected_client.is_healthy().await);
        assert_eq!(protected_client.circuit_breaker_state().await, crate::resilience::CircuitState::Closed);
        
        // Test force operations
        protected_client.force_circuit_open().await;
        assert_eq!(protected_client.circuit_breaker_state().await, crate::resilience::CircuitState::Open);
        assert!(!protected_client.is_healthy().await);
        
        protected_client.force_circuit_closed().await;
        assert_eq!(protected_client.circuit_breaker_state().await, crate::resilience::CircuitState::Closed);
        assert!(protected_client.is_healthy().await);
    }
}

/// Implement PgmqClientTrait for circuit breaker protected PgmqClient
#[async_trait::async_trait]
impl crate::messaging::PgmqClientTrait for ProtectedPgmqClient {
    async fn create_queue(&self, queue_name: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.create_queue(queue_name).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
    
    async fn send_message(&self, queue_name: &str, message: &PgmqStepMessage) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        self.send_message(queue_name, message).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
    
    async fn send_json_message<T: serde::Serialize + Clone + Send + Sync>(&self, queue_name: &str, message: &T) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        self.send_json_message(queue_name, message).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
    
    async fn read_messages(&self, queue_name: &str, visibility_timeout: Option<i32>, qty: Option<i32>) -> Result<Vec<pgmq::types::Message<serde_json::Value>>, Box<dyn std::error::Error + Send + Sync>> {
        self.read_messages(queue_name, visibility_timeout, qty).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
    
    async fn delete_message(&self, queue_name: &str, message_id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.delete_message(queue_name, message_id).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
    
    async fn archive_message(&self, queue_name: &str, message_id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.archive_message(queue_name, message_id).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
    
    async fn purge_queue(&self, queue_name: &str) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
        self.purge_queue(queue_name).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
    
    async fn drop_queue(&self, queue_name: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.drop_queue(queue_name).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
    
    async fn queue_metrics(&self, queue_name: &str) -> Result<QueueMetrics, Box<dyn std::error::Error + Send + Sync>> {
        self.queue_metrics(queue_name).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
    
    async fn initialize_namespace_queues(&self, namespaces: &[&str]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.initialize_namespace_queues(namespaces).await.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
    }
    
    async fn enqueue_step(&self, namespace: &str, step_message: PgmqStepMessage) -> Result<i64, Box<dyn std::error::Error + Send + Sync>> {
        // ProtectedPgmqClient doesn't have enqueue_step, so delegate to inner client
        self.inner().enqueue_step(namespace, step_message).await
    }
    
    async fn process_namespace_queue(&self, namespace: &str, visibility_timeout: Option<i32>, batch_size: i32) -> Result<Vec<pgmq::types::Message<serde_json::Value>>, Box<dyn std::error::Error + Send + Sync>> {
        // ProtectedPgmqClient doesn't have process_namespace_queue, so delegate to inner client  
        self.inner().process_namespace_queue(namespace, visibility_timeout, batch_size).await
    }
    
    async fn complete_message(&self, namespace: &str, message_id: i64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // ProtectedPgmqClient doesn't have complete_message, so delegate to inner client
        self.inner().complete_message(namespace, message_id).await
    }
}