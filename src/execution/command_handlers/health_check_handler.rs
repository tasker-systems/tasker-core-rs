//! Health Check Command Handler (Stub)

use crate::execution::command::{Command, CommandType};
use crate::execution::command_router::CommandHandler;
use async_trait::async_trait;

/// Handler for health check commands (placeholder implementation)
pub struct HealthCheckHandler;

impl HealthCheckHandler {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CommandHandler for HealthCheckHandler {
    async fn handle_command(
        &self,
        _command: Command,
    ) -> Result<Option<Command>, Box<dyn std::error::Error + Send + Sync>> {
        // TODO: Implement health check logic
        Ok(None)
    }

    fn handler_name(&self) -> &str {
        "HealthCheckHandler"
    }

    fn supported_commands(&self) -> Vec<CommandType> {
        vec![CommandType::HealthCheck]
    }
}
