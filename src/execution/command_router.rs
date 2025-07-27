//! Command Router for Unified Command Pattern

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::execution::command::{Command, CommandResult, CommandType};

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
    
    /// Router configuration
    config: CommandRouterConfig,
}

impl CommandRouter {
    /// Create new command router with default configuration
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
            command_history: Arc::new(RwLock::new(Vec::new())),
            config: CommandRouterConfig::default(),
        }
    }

    /// Create command router with custom configuration
    pub fn with_config(config: CommandRouterConfig) -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
            command_history: Arc::new(RwLock::new(Vec::new())),
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
            warn!("Replacing existing handler for command type: {:?}", command_type);
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
            warn!("Attempted to unregister non-existent handler: {:?}", command_type);
        }
        
        removed
    }

    /// Route and execute a command
    pub async fn route_command(&self, command: Command) -> Result<CommandResult, CommandRouterError> {
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
                CommandResult::error(command.command_id.clone(), e.to_string(), execution_time)
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
    async fn handle_command(&self, command: Command) -> Result<Option<Command>, Box<dyn std::error::Error + Send + Sync>>;
    
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
        async fn handle_command(&self, command: Command) -> Result<Option<Command>, Box<dyn std::error::Error + Send + Sync>> {
            // Simple mock response
            let response = command.create_response(
                CommandType::Success,
                CommandPayload::Success {
                    message: format!("Handled by {}", self.name),
                    data: None,
                },
                CommandSource::RustServer { id: "test_server".to_string() },
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
        router.register_handler(CommandType::RegisterWorker, handler).await.unwrap();
        
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

        router.register_handler(CommandType::RegisterWorker, handler).await.unwrap();

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
                },
            },
            CommandSource::RubyWorker { id: "test_worker".to_string() },
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
                batch_id: "test_batch".to_string(),
                steps: vec![],
            },
            CommandSource::RustOrchestrator { id: "test_coord".to_string() },
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

        router.register_handler(CommandType::HealthCheck, handler).await.unwrap();

        // Execute some commands
        for i in 0..5 {
            let command = Command::new(
                CommandType::HealthCheck,
                CommandPayload::HealthCheck {
                    diagnostic_level: crate::execution::command::HealthCheckLevel::Basic,
                },
                CommandSource::RustOrchestrator { id: format!("coord_{}", i) },
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
                },
            },
            CommandSource::RubyWorker { id: "test_worker".to_string() },
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