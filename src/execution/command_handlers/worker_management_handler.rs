//! Worker Management Command Handler

use async_trait::async_trait;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

use crate::execution::command::{Command, CommandPayload, CommandSource, CommandType};
use crate::execution::command_router::CommandHandler;
use crate::execution::worker_pool::{WorkerPool, WorkerPoolError};
use serde_json;

/// Handler for worker lifecycle management commands
/// 
/// Processes worker registration, unregistration, and heartbeat commands.
/// Coordinates with the WorkerPool to maintain worker state and health.
/// 
/// # Supported Commands
/// 
/// - `RegisterWorker`: Register new worker with capabilities
/// - `UnregisterWorker`: Remove worker from pool
/// - `WorkerHeartbeat`: Update worker health and load status
/// 
/// # Examples
/// 
/// ```rust
/// use tasker_core::execution::command_handlers::WorkerManagementHandler;
/// use tasker_core::execution::worker_pool::WorkerPool;
/// use std::sync::Arc;
/// 
/// let worker_pool = Arc::new(WorkerPool::new());
/// let handler = WorkerManagementHandler::new(worker_pool);
/// ```
pub struct WorkerManagementHandler {
    /// Reference to the worker pool for state management
    worker_pool: Arc<WorkerPool>,
}

impl WorkerManagementHandler {
    /// Create new worker management handler
    pub fn new(worker_pool: Arc<WorkerPool>) -> Self {
        Self { worker_pool }
    }

    /// Handle worker registration command
    async fn handle_register_worker(
        &self,
        command: &Command,
        worker_capabilities: crate::execution::command::WorkerCapabilities,
    ) -> Result<Command, Box<dyn std::error::Error + Send + Sync>> {
        info!("Registering worker: {}", worker_capabilities.worker_id);
        
        match self.worker_pool.register_worker(worker_capabilities.clone()).await {
            Ok(()) => {
                info!("Worker registered successfully: {}", worker_capabilities.worker_id);
                
                // Create success response
                let response = command.create_response(
                    CommandType::WorkerRegistered,
                    CommandPayload::WorkerRegistered {
                        worker_id: worker_capabilities.worker_id.clone(),
                        assigned_pool: "default".to_string(),
                        queue_position: self.get_queue_position(&worker_capabilities.worker_id).await,
                    },
                    CommandSource::RustServer {
                        id: "worker_management_handler".to_string(),
                    },
                );
                
                Ok(response)
            }
            Err(e) => {
                error!("Worker registration failed: {}", e);
                
                let response = command.create_response(
                    CommandType::Error,
                    CommandPayload::Error {
                        error_type: "WorkerRegistrationError".to_string(),
                        message: format!("Failed to register worker {}: {}", worker_capabilities.worker_id, e),
                        details: None,
                        retryable: match e {
                            WorkerPoolError::InvalidCapabilities { .. } => false,
                            _ => true,
                        },
                    },
                    CommandSource::RustServer {
                        id: "worker_management_handler".to_string(),
                    },
                );
                
                Ok(response)
            }
        }
    }

    /// Handle worker unregistration command
    async fn handle_unregister_worker(
        &self,
        command: &Command,
        worker_id: String,
        reason: String,
    ) -> Result<Command, Box<dyn std::error::Error + Send + Sync>> {
        info!("Unregistering worker: {} (reason: {})", worker_id, reason);
        
        match self.worker_pool.unregister_worker(&worker_id).await {
            Ok(()) => {
                info!("Worker unregistered successfully: {}", worker_id);
                
                let response = command.create_response(
                    CommandType::WorkerUnregistered,
                    CommandPayload::WorkerUnregistered {
                        worker_id: worker_id.clone(),
                        unregistered_at: chrono::Utc::now().to_rfc3339(),
                        reason: reason.clone(),
                    },
                    CommandSource::RustServer {
                        id: "worker_management_handler".to_string(),
                    },
                );
                
                Ok(response)
            }
            Err(e) => {
                warn!("Worker unregistration failed: {}", e);
                
                let response = command.create_response(
                    CommandType::Error,
                    CommandPayload::Error {
                        error_type: "WorkerUnregistrationError".to_string(),
                        message: format!("Failed to unregister worker {}: {}", worker_id, e),
                        details: None,
                        retryable: false, // Unregistration failures are typically not retryable
                    },
                    CommandSource::RustServer {
                        id: "worker_management_handler".to_string(),
                    },
                );
                
                Ok(response)
            }
        }
    }

    /// Handle health check command
    async fn handle_health_check(
        &self,
        command: &Command,
        diagnostic_level: crate::execution::command::HealthCheckLevel,
    ) -> Result<Command, Box<dyn std::error::Error + Send + Sync>> {
        info!("Processing health check request with level: {:?}", diagnostic_level);
        
        // Get worker pool statistics
        let pool_stats = self.worker_pool.get_stats().await;
        
        let response_data = match diagnostic_level {
            crate::execution::command::HealthCheckLevel::Basic => {
                serde_json::json!({
                    "status": "healthy",
                    "total_workers": pool_stats.total_workers,
                    "healthy_workers": pool_stats.healthy_workers,
                    "current_load": pool_stats.current_load,
                })
            }
            crate::execution::command::HealthCheckLevel::Detailed => {
                serde_json::json!({
                    "status": "healthy",
                    "total_workers": pool_stats.total_workers,
                    "healthy_workers": pool_stats.healthy_workers,
                    "unhealthy_workers": pool_stats.unhealthy_workers,
                    "total_capacity": pool_stats.total_capacity,
                    "current_load": pool_stats.current_load,
                    "available_capacity": pool_stats.available_capacity,
                })
            }
            crate::execution::command::HealthCheckLevel::Full => {
                serde_json::json!({
                    "status": "healthy",
                    "total_workers": pool_stats.total_workers,
                    "healthy_workers": pool_stats.healthy_workers,
                    "unhealthy_workers": pool_stats.unhealthy_workers,
                    "total_capacity": pool_stats.total_capacity,
                    "current_load": pool_stats.current_load,
                    "available_capacity": pool_stats.available_capacity,
                    "total_steps_processed": pool_stats.total_steps_processed,
                    "successful_steps": pool_stats.successful_steps,
                    "failed_steps": pool_stats.failed_steps,
                    "namespace_distribution": pool_stats.namespace_distribution,
                })
            }
        };
        
        let response = command.create_response(
            CommandType::HealthCheckResult,
            CommandPayload::HealthCheckResult {
                status: "healthy".to_string(),
                uptime_seconds: 0, // TODO: Track actual uptime
                total_workers: pool_stats.total_workers,
                active_commands: pool_stats.current_load,
                diagnostics: Some(response_data),
            },
            CommandSource::RustServer {
                id: "worker_management_handler".to_string(),
            },
        );
        
        Ok(response)
    }

    /// Handle worker heartbeat command
    async fn handle_worker_heartbeat(
        &self,
        command: &Command,
        worker_id: String,
        current_load: usize,
        system_stats: Option<crate::execution::command::SystemStats>,
    ) -> Result<Command, Box<dyn std::error::Error + Send + Sync>> {
        debug!("Processing heartbeat for worker: {} (load: {})", worker_id, current_load);
        
        match self.worker_pool.update_worker_heartbeat(&worker_id, current_load).await {
            Ok(()) => {
                debug!("Heartbeat processed successfully for worker: {}", worker_id);
                
                // Get current worker state for response
                let worker_state = self.worker_pool.get_worker(&worker_id).await;
                
                let response = command.create_response(
                    CommandType::HeartbeatAcknowledged,
                    CommandPayload::HeartbeatAcknowledged {
                        worker_id: worker_id.clone(),
                        acknowledged_at: chrono::Utc::now().to_rfc3339(),
                        status: "healthy".to_string(),
                        next_heartbeat_in: Some(30), // Suggest next heartbeat in 30 seconds
                    },
                    CommandSource::RustServer {
                        id: "worker_management_handler".to_string(),
                    },
                );
                
                Ok(response)
            }
            Err(e) => {
                warn!("Heartbeat processing failed for worker {}: {}", worker_id, e);
                
                let response = command.create_response(
                    CommandType::Error,
                    CommandPayload::Error {
                        error_type: "HeartbeatError".to_string(),
                        message: format!("Failed to process heartbeat for worker {}: {}", worker_id, e),
                        details: None,
                        retryable: true, // Heartbeat failures are typically retryable
                    },
                    CommandSource::RustServer {
                        id: "worker_management_handler".to_string(),
                    },
                );
                
                Ok(response)
            }
        }
    }

    /// Get queue position for a worker (simplified implementation)
    async fn get_queue_position(&self, _worker_id: &str) -> usize {
        let stats = self.worker_pool.get_stats().await;
        // Simple queue position based on registration order
        // In a more sophisticated implementation, this could be based on
        // priority, capabilities, or other factors
        stats.total_workers
    }
}

#[async_trait]
impl CommandHandler for WorkerManagementHandler {
    async fn handle_command(&self, command: Command) -> Result<Option<Command>, Box<dyn std::error::Error + Send + Sync>> {
        debug!("WorkerManagementHandler processing command: {:?}", command.command_type);
        
        let response = match &command.payload {
            CommandPayload::RegisterWorker { worker_capabilities } => {
                self.handle_register_worker(&command, worker_capabilities.clone()).await?
            }
            
            CommandPayload::UnregisterWorker { worker_id, reason } => {
                self.handle_unregister_worker(&command, worker_id.clone(), reason.clone()).await?
            }
            
            CommandPayload::WorkerHeartbeat { worker_id, current_load, system_stats } => {
                self.handle_worker_heartbeat(&command, worker_id.clone(), *current_load, system_stats.clone()).await?
            }
            
            CommandPayload::HealthCheck { diagnostic_level } => {
                self.handle_health_check(&command, diagnostic_level.clone()).await?
            }
            
            _ => {
                error!("WorkerManagementHandler received unsupported command: {:?}", command.command_type);
                
                command.create_response(
                    CommandType::Error,
                    CommandPayload::Error {
                        error_type: "UnsupportedCommand".to_string(),
                        message: format!("WorkerManagementHandler does not support command type: {:?}", command.command_type),
                        details: None,
                        retryable: false,
                    },
                    CommandSource::RustServer {
                        id: "worker_management_handler".to_string(),
                    },
                )
            }
        };
        
        Ok(Some(response))
    }

    fn handler_name(&self) -> &str {
        "WorkerManagementHandler"
    }

    fn supported_commands(&self) -> Vec<CommandType> {
        vec![
            CommandType::RegisterWorker,
            CommandType::UnregisterWorker,
            CommandType::WorkerHeartbeat,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::command::{Command, CommandPayload, CommandSource, WorkerCapabilities};
    use std::collections::HashMap;

    fn create_test_capabilities(worker_id: &str) -> WorkerCapabilities {
        WorkerCapabilities {
            worker_id: worker_id.to_string(),
            max_concurrent_steps: 10,
            supported_namespaces: vec!["test".to_string()],
            step_timeout_ms: 30000,
            supports_retries: true,
            language_runtime: "ruby".to_string(),
            version: "3.1.0".to_string(),
            custom_capabilities: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_register_worker_success() {
        let worker_pool = Arc::new(WorkerPool::new());
        let handler = WorkerManagementHandler::new(worker_pool.clone());
        
        let capabilities = create_test_capabilities("test_worker");
        let command = Command::new(
            CommandType::RegisterWorker,
            CommandPayload::RegisterWorker {
                worker_capabilities: capabilities.clone(),
            },
            CommandSource::RubyWorker {
                id: "test_worker".to_string(),
            },
        );

        let result = handler.handle_command(command).await.unwrap();
        assert!(result.is_some());
        
        let response = result.unwrap();
        assert_eq!(response.command_type, CommandType::WorkerRegistered);
        
        // Verify worker was registered
        let worker = worker_pool.get_worker("test_worker").await;
        assert!(worker.is_some());
    }

    #[tokio::test]
    async fn test_register_worker_invalid_capabilities() {
        let worker_pool = Arc::new(WorkerPool::new());
        let handler = WorkerManagementHandler::new(worker_pool);
        
        let mut capabilities = create_test_capabilities("test_worker");
        capabilities.max_concurrent_steps = 0; // Invalid
        
        let command = Command::new(
            CommandType::RegisterWorker,
            CommandPayload::RegisterWorker {
                worker_capabilities: capabilities,
            },
            CommandSource::RubyWorker {
                id: "test_worker".to_string(),
            },
        );

        let result = handler.handle_command(command).await.unwrap();
        assert!(result.is_some());
        
        let response = result.unwrap();
        assert_eq!(response.command_type, CommandType::Error);
        
        // Verify error is not retryable for invalid capabilities
        if let CommandPayload::Error { retryable, .. } = response.payload {
            assert!(!retryable);
        } else {
            panic!("Expected Error payload");
        }
    }

    #[tokio::test]
    async fn test_unregister_worker() {
        let worker_pool = Arc::new(WorkerPool::new());
        let handler = WorkerManagementHandler::new(worker_pool.clone());
        
        // First register a worker
        let capabilities = create_test_capabilities("test_worker");
        worker_pool.register_worker(capabilities).await.unwrap();
        
        // Then unregister it
        let command = Command::new(
            CommandType::UnregisterWorker,
            CommandPayload::UnregisterWorker {
                worker_id: "test_worker".to_string(),
                reason: "Test shutdown".to_string(),
            },
            CommandSource::RubyWorker {
                id: "test_worker".to_string(),
            },
        );

        let result = handler.handle_command(command).await.unwrap();
        assert!(result.is_some());
        
        let response = result.unwrap();
        assert_eq!(response.command_type, CommandType::Success);
        
        // Verify worker was unregistered
        let worker = worker_pool.get_worker("test_worker").await;
        assert!(worker.is_none());
    }

    #[tokio::test]
    async fn test_worker_heartbeat() {
        let worker_pool = Arc::new(WorkerPool::new());
        let handler = WorkerManagementHandler::new(worker_pool.clone());
        
        // First register a worker
        let capabilities = create_test_capabilities("test_worker");
        worker_pool.register_worker(capabilities).await.unwrap();
        
        // Send heartbeat
        let command = Command::new(
            CommandType::WorkerHeartbeat,
            CommandPayload::WorkerHeartbeat {
                worker_id: "test_worker".to_string(),
                current_load: 5,
                system_stats: None,
            },
            CommandSource::RubyWorker {
                id: "test_worker".to_string(),
            },
        );

        let result = handler.handle_command(command).await.unwrap();
        assert!(result.is_some());
        
        let response = result.unwrap();
        assert_eq!(response.command_type, CommandType::Success);
        
        // Verify worker load was updated
        let worker = worker_pool.get_worker("test_worker").await.unwrap();
        assert_eq!(worker.current_load, 5);
    }

    #[tokio::test]
    async fn test_unsupported_command() {
        let worker_pool = Arc::new(WorkerPool::new());
        let handler = WorkerManagementHandler::new(worker_pool);
        
        let command = Command::new(
            CommandType::ExecuteBatch,
            CommandPayload::ExecuteBatch {
                batch_id: "test_batch".to_string(),
                steps: vec![],
            },
            CommandSource::RustOrchestrator {
                id: "test_coord".to_string(),
            },
        );

        let result = handler.handle_command(command).await.unwrap();
        assert!(result.is_some());
        
        let response = result.unwrap();
        assert_eq!(response.command_type, CommandType::Error);
        
        if let CommandPayload::Error { error_type, retryable, .. } = response.payload {
            assert_eq!(error_type, "UnsupportedCommand");
            assert!(!retryable);
        } else {
            panic!("Expected Error payload");
        }
    }
}