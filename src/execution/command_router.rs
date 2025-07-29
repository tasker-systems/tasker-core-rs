//! Command Router for Unified Command Pattern

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::execution::command::{Command, CommandPayload, CommandResult, CommandType};
use crate::models::core::worker_command_audit::{
    CommandDirection, NewWorkerCommandAudit, WorkerCommandAudit,
};
use sqlx::PgPool;

/// Central command routing and dispatch system
///
/// The CommandRouter manages command handlers and provides unified routing
/// for all command types. It supports handler registration, command validation,
/// and lifecycle management with proper error handling.
///
/// # Examples
///
/// ```rust
/// use tasker_core::execution::command_router::*;
/// use tasker_core::execution::command::*;
/// use std::sync::Arc;
///
/// #[tokio::main]
/// async fn main() {
///     let router = CommandRouter::new();
///
///     // Register handler would be done here
///     // router.register_handler(CommandType::RegisterWorker, handler).await;
///
///     let command = Command::new(
///         CommandType::RegisterWorker,
///         CommandPayload::RegisterWorker { /* ... */ },
///         CommandSource::RubyWorker { id: "worker_1".to_string() }
///     );
///
///     match router.route_command(command).await {
///         Ok(result) => println!("Command executed: {:?}", result),
///         Err(e) => println!("Command failed: {}", e),
///     }
/// }
/// ```
pub struct CommandRouter {
    /// Registry of command handlers by command type
    handlers: Arc<RwLock<HashMap<CommandType, Arc<dyn CommandHandler>>>>,

    /// Command execution history for debugging and replay
    command_history: Arc<RwLock<Vec<CommandExecution>>>,

    /// Database pool for command audit trail (optional for backward compatibility)
    db_pool: Option<PgPool>,

    /// Router configuration
    config: CommandRouterConfig,
}

impl CommandRouter {
    /// Create new command router with default configuration (no database audit)
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
            command_history: Arc::new(RwLock::new(Vec::new())),
            db_pool: None,
            config: CommandRouterConfig::default(),
        }
    }

    /// Create new command router with database audit trail support
    pub fn with_database_audit(db_pool: PgPool) -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
            command_history: Arc::new(RwLock::new(Vec::new())),
            db_pool: Some(db_pool),
            config: CommandRouterConfig::default(),
        }
    }

    /// Create command router with custom configuration (no database audit)
    pub fn with_config(config: CommandRouterConfig) -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
            command_history: Arc::new(RwLock::new(Vec::new())),
            db_pool: None,
            config,
        }
    }

    /// Register a command handler for a specific command type
    pub async fn register_handler(
        &self,
        command_type: CommandType,
        handler: Arc<dyn CommandHandler>,
    ) -> Result<(), CommandRouterError> {
        let mut handlers = self.handlers.write().await;

        if handlers.contains_key(&command_type) {
            warn!(
                "Replacing existing handler for command type: {:?}",
                command_type
            );
        }

        handlers.insert(command_type.clone(), handler);
        info!("Registered command handler for type: {:?}", command_type);

        Ok(())
    }

    /// Unregister a command handler
    pub async fn unregister_handler(&self, command_type: &CommandType) -> bool {
        let mut handlers = self.handlers.write().await;
        let removed = handlers.remove(command_type).is_some();

        if removed {
            info!("Unregistered command handler for type: {:?}", command_type);
        } else {
            warn!(
                "Attempted to unregister non-existent handler: {:?}",
                command_type
            );
        }

        removed
    }

    /// Route and execute a command
    pub async fn route_command(
        &self,
        command: Command,
    ) -> Result<CommandResult, CommandRouterError> {
        let start_time = std::time::Instant::now();

        debug!(
            "Routing command: type={:?}, id={}, correlation_id={:?}",
            command.command_type, command.command_id, command.correlation_id
        );

        // Validate command
        self.validate_command(&command)?;

        // Find handler
        let handlers = self.handlers.read().await;
        let handler = handlers
            .get(&command.command_type)
            .ok_or_else(|| CommandRouterError::HandlerNotFound {
                command_type: command.command_type.clone(),
            })?
            .clone();
        drop(handlers);

        // Execute command
        let execution_start = std::time::Instant::now();
        let result = match handler.handle_command(command.clone()).await {
            Ok(result) => {
                let execution_time = execution_start.elapsed().as_millis() as u64;
                info!(
                    "Command executed successfully: type={:?}, id={}, time={}ms",
                    command.command_type, command.command_id, execution_time
                );
                CommandResult::success(command.command_id.clone(), result, execution_time)
            }
            Err(e) => {
                let execution_time = execution_start.elapsed().as_millis() as u64;
                error!(
                    "Command execution failed: type={:?}, id={}, error={}, time={}ms",
                    command.command_type, command.command_id, e, execution_time
                );

                // Create error response to send back to client
                let error_response = command.create_response(
                    CommandType::Error,
                    crate::execution::command::CommandPayload::Error {
                        error_type: "CommandHandlerError".to_string(),
                        message: e.to_string(),
                        details: None,
                        retryable: false,
                    },
                    crate::execution::command::CommandSource::RustServer {
                        id: "command_router".to_string(),
                    },
                );

                CommandResult {
                    command_id: command.command_id.clone(),
                    success: false,
                    response: Some(error_response),
                    error: Some(e.to_string()),
                    execution_time_ms: execution_time,
                }
            }
        };

        // Record execution history if enabled
        if self.config.enable_history {
            let execution = CommandExecution {
                command: command.clone(),
                result: result.clone(),
                timestamp: chrono::Utc::now(),
            };

            let mut history = self.command_history.write().await;
            history.push(execution);

            // Limit history size
            if history.len() > self.config.max_history_size {
                let excess = history.len() - self.config.max_history_size;
                history.drain(0..excess);
            }
        }

        // Audit command to database if enabled
        if let Some(ref db_pool) = self.db_pool {
            self.audit_command_to_database(db_pool, &command, &result)
                .await;
        }

        let total_time = start_time.elapsed().as_millis() as u64;
        debug!(
            "Command routing completed: id={}, total_time={}ms",
            command.command_id, total_time
        );

        Ok(result)
    }

    /// Get command execution history
    pub async fn get_command_history(&self) -> Vec<CommandExecution> {
        self.command_history.read().await.clone()
    }

    /// Get registered command types
    pub async fn get_registered_command_types(&self) -> Vec<CommandType> {
        self.handlers.read().await.keys().cloned().collect()
    }

    /// Check if handler is registered for command type
    pub async fn has_handler(&self, command_type: &CommandType) -> bool {
        self.handlers.read().await.contains_key(command_type)
    }

    /// Get router statistics
    pub async fn get_stats(&self) -> CommandRouterStats {
        let handlers = self.handlers.read().await;
        let history = self.command_history.read().await;

        let successful_commands = history.iter().filter(|e| e.result.success).count();
        let failed_commands = history.iter().filter(|e| !e.result.success).count();

        CommandRouterStats {
            registered_handlers: handlers.len(),
            total_commands_processed: history.len(),
            successful_commands,
            failed_commands,
            history_enabled: self.config.enable_history,
        }
    }

    /// Clear command history
    pub async fn clear_history(&self) {
        self.command_history.write().await.clear();
        info!("Command history cleared");
    }

    /// Extract task_id from command payload if available
    fn extract_task_id_from_payload(
        &self,
        payload: &CommandPayload,
    ) -> Option<i64> {
        match payload {
            CommandPayload::InitializeTask { .. } => {
                None
            }
            CommandPayload::ExecuteBatch { steps, .. } => {
                // Extract task_id from first step if available
                steps.first().map(|step| step.task_id)
            }
            CommandPayload::ReportPartialResult {
                step_id: _, ..
            } => {
                // Could be enhanced to lookup task_id from step_id if needed
                None
            }
            _ => None,
        }
    }

    /// Validate command before routing
    fn validate_command(&self, command: &Command) -> Result<(), CommandRouterError> {
        // Check command ID is not empty
        if command.command_id.is_empty() {
            return Err(CommandRouterError::InvalidCommand {
                reason: "Command ID cannot be empty".to_string(),
            });
        }

        // Check timeout if specified
        if let Some(timeout_ms) = command.metadata.timeout_ms {
            if timeout_ms == 0 {
                return Err(CommandRouterError::InvalidCommand {
                    reason: "Timeout cannot be zero".to_string(),
                });
            }
        }

        // Additional validation can be added here

        Ok(())
    }

    /// Audit command execution to database for observability and debugging
    async fn audit_command_to_database(
        &self,
        db_pool: &PgPool,
        command: &Command,
        result: &CommandResult,
    ) {
        // Extract worker ID from command metadata if available
        let worker_id = match &command.metadata.source {
            crate::execution::command::CommandSource::RubyWorker { id: worker_name } => {
                // Look up numeric worker ID from database based on worker name
                match sqlx::query!(
                    "SELECT worker_id FROM tasker_workers WHERE worker_name = $1",
                    worker_name
                )
                .fetch_optional(db_pool)
                .await
                {
                    Ok(Some(row)) => Some(row.worker_id),
                    Ok(None) => {
                        debug!("Worker '{}' not found in database for audit", worker_name);
                        None
                    }
                    Err(e) => {
                        warn!("Failed to lookup worker '{}' for audit: {}", worker_name, e);
                        None
                    }
                }
            }
            _ => None,
        };

        // Determine batch_id if this is a batch command
        let batch_id = match &command.payload {
            crate::execution::command::CommandPayload::ExecuteBatch { batch_id, .. } => {
                Some(batch_id.clone())
            }
            _ => None,
        };

        // Only audit if we have a worker_id (skip router-to-router commands)
        if let Some(worker_id) = worker_id {
            let audit_record = NewWorkerCommandAudit {
                worker_id,
                command_type: format!("{:?}", command.command_type),
                command_direction: CommandDirection::Received, // Router receives commands
                correlation_id: command.correlation_id.clone(),
                batch_id: batch_id.and_then(|id| id.parse::<i64>().ok()), // Convert string to i64
                task_id: self.extract_task_id_from_payload(&command.payload),
                step_count: match &command.payload {
                    CommandPayload::ExecuteBatch { steps, .. } => {
                        Some(steps.len() as i32)
                    }
                    _ => None,
                },
                execution_time_ms: Some(result.execution_time_ms as i64),
                response_status: Some(if result.success {
                    "success".to_string()
                } else {
                    "error".to_string()
                }),
                error_message: if result.success {
                    None
                } else {
                    result.error.clone()
                },
                metadata: Some(serde_json::json!({
                    "command_id": command.command_id,
                    "correlation_id": command.correlation_id,
                    "command_source": command.metadata.source,
                    "namespace": command.metadata.namespace,
                    "priority": command.metadata.priority,
                    "result_response": result.response,
                })),
            };

            // Insert audit record (ignore errors to not disrupt command processing)
            if let Err(e) = WorkerCommandAudit::create(db_pool, audit_record).await {
                warn!(
                    "Failed to audit command {} to database: {}",
                    command.command_id, e
                );
            } else {
                debug!(
                    "Successfully audited command {} to database",
                    command.command_id
                );
            }
        } else {
            debug!(
                "Skipping audit for command {} - no worker_id available",
                command.command_id
            );
        }
    }
}

impl Default for CommandRouter {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for command handlers
///
/// All command handlers must implement this trait to be registered
/// with the CommandRouter. Handlers are responsible for the actual
/// execution logic for specific command types.
#[async_trait]
pub trait CommandHandler: Send + Sync {
    /// Handle a command and return optional response
    async fn handle_command(
        &self,
        command: Command,
    ) -> Result<Option<Command>, Box<dyn std::error::Error + Send + Sync>>;

    /// Get handler name for debugging
    fn handler_name(&self) -> &str;

    /// Get supported command types
    fn supported_commands(&self) -> Vec<CommandType>;
}

/// Command router configuration
#[derive(Debug, Clone)]
pub struct CommandRouterConfig {
    /// Enable command execution history tracking
    pub enable_history: bool,

    /// Maximum number of command executions to keep in history
    pub max_history_size: usize,

    /// Default command timeout in milliseconds
    pub default_timeout_ms: u64,
}

impl Default for CommandRouterConfig {
    fn default() -> Self {
        Self {
            enable_history: true,
            max_history_size: 1000,
            default_timeout_ms: 30000,
        }
    }
}

/// Command execution record for history tracking
#[derive(Debug, Clone)]
pub struct CommandExecution {
    pub command: Command,
    pub result: CommandResult,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Command router statistics
#[derive(Debug, Clone)]
pub struct CommandRouterStats {
    pub registered_handlers: usize,
    pub total_commands_processed: usize,
    pub successful_commands: usize,
    pub failed_commands: usize,
    pub history_enabled: bool,
}

/// Command router errors
#[derive(Debug, thiserror::Error)]
pub enum CommandRouterError {
    #[error("Handler not found for command type: {command_type:?}")]
    HandlerNotFound { command_type: CommandType },

    #[error("Invalid command: {reason}")]
    InvalidCommand { reason: String },

    #[error("Handler registration failed: {reason}")]
    RegistrationFailed { reason: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::command::{CommandPayload, CommandSource, WorkerCapabilities};
    use std::collections::HashMap;

    /// Mock command handler for testing
    struct MockHandler {
        name: String,
        supported: Vec<CommandType>,
    }

    #[async_trait]
    impl CommandHandler for MockHandler {
        async fn handle_command(
            &self,
            command: Command,
        ) -> Result<Option<Command>, Box<dyn std::error::Error + Send + Sync>> {
            // Simple mock response
            let response = command.create_response(
                CommandType::Success,
                CommandPayload::Success {
                    message: format!("Handled by {}", self.name),
                    data: None,
                },
                CommandSource::RustServer {
                    id: "test_server".to_string(),
                },
            );
            Ok(Some(response))
        }

        fn handler_name(&self) -> &str {
            &self.name
        }

        fn supported_commands(&self) -> Vec<CommandType> {
            self.supported.clone()
        }
    }

    #[tokio::test]
    async fn test_command_router_registration() {
        let router = CommandRouter::new();

        let handler = Arc::new(MockHandler {
            name: "test_handler".to_string(),
            supported: vec![CommandType::RegisterWorker],
        });

        // Register handler
        router
            .register_handler(CommandType::RegisterWorker, handler)
            .await
            .unwrap();

        // Check handler is registered
        assert!(router.has_handler(&CommandType::RegisterWorker).await);
        assert!(!router.has_handler(&CommandType::ExecuteBatch).await);

        // Check registered types
        let types = router.get_registered_command_types().await;
        assert_eq!(types.len(), 1);
        assert!(types.contains(&CommandType::RegisterWorker));
    }

    #[tokio::test]
    async fn test_command_routing() {
        let router = CommandRouter::new();

        let handler = Arc::new(MockHandler {
            name: "test_handler".to_string(),
            supported: vec![CommandType::RegisterWorker],
        });

        router
            .register_handler(CommandType::RegisterWorker, handler)
            .await
            .unwrap();

        // Create test command
        let command = Command::new(
            CommandType::RegisterWorker,
            CommandPayload::RegisterWorker {
                worker_capabilities: WorkerCapabilities {
                    worker_id: "test_worker".to_string(),
                    max_concurrent_steps: 10,
                    supported_namespaces: vec!["test".to_string()],
                    step_timeout_ms: 30000,
                    supports_retries: true,
                    language_runtime: "rust".to_string(),
                    version: "1.0.0".to_string(),
                    custom_capabilities: HashMap::new(),
                    connection_info: None,
                    runtime_info: None,
                },
            },
            CommandSource::RubyWorker {
                id: "test_worker".to_string(),
            },
        );

        // Route command
        let result = router.route_command(command).await.unwrap();

        assert!(result.success);
        assert!(result.response.is_some());
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn test_handler_not_found() {
        let router = CommandRouter::new();

        let command = Command::new(
            CommandType::ExecuteBatch,
            CommandPayload::ExecuteBatch {
                task_template: TaskTemplate {
                    namespace_name: "test".to_string(),
                    task_name: "test".to_string(),
                    task_version: "1.0.0".to_string(),
                    task_handler_class: "test".to_string(),
                    task_handler_config: serde_json::json!({}),
                    task_handler_config_type: "json".to_string(),
                    task_handler_config_version: "1.0.0".to_string(),
                    task_handler_config_namespace: "test".to_string(),
                },
                batch_id: "test_batch".to_string(),
                steps: vec![],
            },
            CommandSource::RustOrchestrator {
                id: "test_coord".to_string(),
            },
        );

        let result = router.route_command(command).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            CommandRouterError::HandlerNotFound { command_type } => {
                assert_eq!(command_type, CommandType::ExecuteBatch);
            }
            _ => panic!("Expected HandlerNotFound error"),
        }
    }

    #[tokio::test]
    async fn test_command_history() {
        let config = CommandRouterConfig {
            enable_history: true,
            max_history_size: 10,
            default_timeout_ms: 30000,
        };

        let router = CommandRouter::with_config(config);

        let handler = Arc::new(MockHandler {
            name: "test_handler".to_string(),
            supported: vec![CommandType::HealthCheck],
        });

        router
            .register_handler(CommandType::HealthCheck, handler)
            .await
            .unwrap();

        // Execute some commands
        for i in 0..5 {
            let command = Command::new(
                CommandType::HealthCheck,
                CommandPayload::HealthCheck {
                    diagnostic_level: crate::execution::command::HealthCheckLevel::Basic,
                },
                CommandSource::RustOrchestrator {
                    id: format!("coord_{}", i),
                },
            );

            router.route_command(command).await.unwrap();
        }

        // Check history
        let history = router.get_command_history().await;
        assert_eq!(history.len(), 5);

        // Check stats
        let stats = router.get_stats().await;
        assert_eq!(stats.total_commands_processed, 5);
        assert_eq!(stats.successful_commands, 5);
        assert_eq!(stats.failed_commands, 0);
    }

    #[tokio::test]
    async fn test_invalid_command() {
        let router = CommandRouter::new();

        let mut command = Command::new(
            CommandType::RegisterWorker,
            CommandPayload::RegisterWorker {
                worker_capabilities: WorkerCapabilities {
                    worker_id: "test_worker".to_string(),
                    max_concurrent_steps: 10,
                    supported_namespaces: vec!["test".to_string()],
                    step_timeout_ms: 30000,
                    supports_retries: true,
                    language_runtime: "rust".to_string(),
                    version: "1.0.0".to_string(),
                    custom_capabilities: HashMap::new(),
                    connection_info: None,
                    runtime_info: None,
                },
            },
            CommandSource::RubyWorker {
                id: "test_worker".to_string(),
            },
        );

        // Make command invalid
        command.command_id = String::new();

        let result = router.route_command(command).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            CommandRouterError::InvalidCommand { reason } => {
                assert!(reason.contains("Command ID cannot be empty"));
            }
            _ => panic!("Expected InvalidCommand error"),
        }
    }
}
