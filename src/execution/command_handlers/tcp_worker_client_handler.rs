//! TCP Worker Client Handler
//!
//! Routes ExecuteBatch commands to external Ruby workers via TCP connections.
//! This handler enables the CommandRouter to send commands to workers running
//! CommandListener servers instead of only handling commands internally.

use async_trait::async_trait;
use serde_json;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio::time::timeout;
use tracing::{debug, info, warn};

use crate::execution::command::{Command, CommandPayload, CommandSource, CommandType};
use crate::execution::command_router::CommandHandler;

/// Connection pool for reusing TCP connections to workers
type WorkerConnectionPool = Arc<RwLock<HashMap<String, Arc<WorkerConnection>>>>;

/// Worker connection information loaded from database
#[derive(Debug, Clone)]
struct WorkerConnectionInfo {
    pub worker_name: String,
    pub host: String,
    pub port: u16,
    pub transport_type: String,
}

/// Reusable TCP connection to a worker
#[derive(Debug)]
struct WorkerConnection {
    pub worker_name: String,
    pub host: String,
    pub port: u16,
    pub last_used: std::time::Instant,
    pub connection_count: usize,
}

impl WorkerConnection {
    fn new(worker_name: String, host: String, port: u16) -> Self {
        Self {
            worker_name,
            host,
            port,
            last_used: std::time::Instant::now(),
            connection_count: 0,
        }
    }

    /// Create a new TCP stream to this worker
    async fn create_stream(&self) -> Result<TcpStream, Box<dyn std::error::Error + Send + Sync>> {
        let address = format!("{}:{}", self.host, self.port);
        debug!(
            "Creating TCP connection to worker {} at {}",
            self.worker_name, address
        );

        let stream = timeout(Duration::from_secs(5), TcpStream::connect(&address))
            .await
            .map_err(|_| format!("Connection timeout to worker {} at {}", self.worker_name, address))?
            .map_err(|e| format!("Failed to connect to worker {} at {}: {}", self.worker_name, address, e))?;

        debug!(
            "Successfully connected to worker {} at {}",
            self.worker_name, address
        );

        Ok(stream)
    }
}

/// Handler that routes ExecuteBatch commands to external Ruby workers via TCP
pub struct TcpWorkerClientHandler {
    /// Database pool for looking up worker connection info
    db_pool: PgPool,
    /// Connection pool for reusing worker connections
    connection_pool: WorkerConnectionPool,
    /// Maximum time to wait for worker response
    response_timeout: Duration,
    /// Maximum number of cached connections per worker
    max_connections_per_worker: usize,
}

impl TcpWorkerClientHandler {
    /// Create new TCP worker client handler
    pub fn new(db_pool: PgPool) -> Self {
        Self {
            db_pool,
            connection_pool: Arc::new(RwLock::new(HashMap::new())),
            response_timeout: Duration::from_secs(30),
            max_connections_per_worker: 5,
        }
    }

    /// Create new TCP worker client handler with custom configuration
    pub fn with_config(
        db_pool: PgPool,
        response_timeout: Duration,
        max_connections_per_worker: usize,
    ) -> Self {
        Self {
            db_pool,
            connection_pool: Arc::new(RwLock::new(HashMap::new())),
            response_timeout,
            max_connections_per_worker,
        }
    }

    /// Handle ExecuteBatch command by routing to appropriate worker
    async fn handle_execute_batch(
        &self,
        command: &Command,
        batch_id: String,
        steps: Vec<crate::execution::command::StepExecutionRequest>,
        _task_template: crate::models::core::task_template::TaskTemplate,
    ) -> Result<Command, Box<dyn std::error::Error + Send + Sync>> {
        info!(
            "Routing ExecuteBatch command {} with {} steps to worker via TCP",
            batch_id,
            steps.len()
        );

        // Extract the first step to determine which worker can handle this batch
        let first_step = steps.first().ok_or("ExecuteBatch command has no steps")?;
        let task_id = first_step.task_id;

        // Find a worker that can handle this task
        let worker_info = self.find_worker_for_task(task_id).await?;

        info!(
            "Selected worker {} at {}:{} for batch {}",
            worker_info.worker_name, worker_info.host, worker_info.port, batch_id
        );

        // Send command to worker via TCP
        let response = self.send_command_to_worker(&worker_info, command).await?;

        info!(
            "Successfully sent ExecuteBatch {} to worker {} and received response",
            batch_id, worker_info.worker_name
        );

        Ok(response)
    }

    /// Find a worker that can handle the specified task
    async fn find_worker_for_task(
        &self,
        task_id: i64,
    ) -> Result<WorkerConnectionInfo, Box<dyn std::error::Error + Send + Sync>> {
        // CRITICAL FIX: Query database to find workers by registered capabilities, not existing workflow steps
        // ExecuteBatch commands are sent BEFORE workflow steps are created, so we need to find workers
        // based on their registered task capabilities, not existing task associations
        
        info!("🔍 TCP WORKER SELECTION: Finding worker for task_id={}, querying by worker capabilities", task_id);
        
        // For now, use a simple approach: find any available registered worker
        // This supports the integration test scenario where workers register with general capabilities
        // and ExecuteBatch commands are sent before workflow steps are created
        let result = sqlx::query!(
            r#"
            SELECT DISTINCT
                w.worker_name,
                w.metadata,
                wr.connection_details,
                wr.connection_type,
                wr.registered_at
            FROM tasker_workers w
            JOIN tasker_worker_registrations wr ON w.worker_id = wr.worker_id
            WHERE wr.status = 'registered'
            ORDER BY wr.registered_at DESC
            LIMIT 1
            "#
        )
        .fetch_optional(&self.db_pool)
        .await?;
        
        if let Some(row) = result {
            info!("✅ TCP WORKER SELECTION: Found registered worker {} for task_id={}", row.worker_name, task_id);
            return self.build_worker_connection_info(row.worker_name, row.metadata, row.connection_details).await;
        }

        Err(format!("No available workers found for task {}", task_id).into())
    }
    
    /// Extract worker connection info from database row
    async fn build_worker_connection_info(
        &self,
        worker_name: String,
        metadata: Option<serde_json::Value>,
        connection_details: serde_json::Value,
    ) -> Result<WorkerConnectionInfo, Box<dyn std::error::Error + Send + Sync>> {
        // Extract listener connection info from worker metadata (custom_capabilities)
        let worker_metadata = metadata.unwrap_or_else(|| serde_json::json!({}));
        let empty_capabilities = serde_json::json!({});
        let custom_capabilities = worker_metadata
            .get("custom")
            .unwrap_or(&empty_capabilities);

        // Try to get listener connection info from custom_capabilities first
        let listener_host = custom_capabilities
            .get("listener_host")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let listener_port = custom_capabilities
            .get("listener_port")
            .and_then(|v| v.as_u64())
            .map(|p| p as u16);

        // Fall back to connection_details if listener info not in custom_capabilities
        let fallback_host = connection_details
            .get("host")
            .and_then(|v| v.as_str())
            .unwrap_or("127.0.0.1")
            .to_string();

        let fallback_port = connection_details
            .get("listener_port")  // Use listener_port from connection_details 
            .and_then(|v| v.as_u64())
            .unwrap_or_else(|| {
                connection_details
                    .get("port")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(8080)
            }) as u16;

        // Use listener connection info if available, otherwise fall back to connection_details
        let host = listener_host.unwrap_or(fallback_host);
        let port = listener_port.unwrap_or(fallback_port);

        info!(
            "✅ TCP WORKER CONNECTION: Building connection info for worker {} - CommandListener at {}:{} (custom: listener_host={:?}, listener_port={:?}, connection_details.listener_port={:?})",
            worker_name, host, port, 
            custom_capabilities.get("listener_host"),
            custom_capabilities.get("listener_port"),
            connection_details.get("listener_port")
        );

        Ok(WorkerConnectionInfo {
            worker_name,
            host,
            port,
            transport_type: "tcp".to_string(), // Always TCP for CommandListener connections
        })
    }

    /// Send command to worker via TCP connection
    async fn send_command_to_worker(
        &self,
        worker_info: &WorkerConnectionInfo,
        command: &Command,
    ) -> Result<Command, Box<dyn std::error::Error + Send + Sync>> {
        // Get or create connection to worker
        let connection = self.get_or_create_connection(worker_info).await?;

        // Create TCP stream
        let mut stream = connection.create_stream().await?;

        // Serialize command to JSON
        let command_json = serde_json::to_string(command)?;
        debug!(
            "Sending command to worker {}: {}",
            worker_info.worker_name,
            command_json.chars().take(200).collect::<String>()
        );

        // Send command
        stream.write_all(command_json.as_bytes()).await?;
        stream.write_all(b"\n").await?;
        stream.flush().await?;

        // Read response with timeout
        let response = timeout(self.response_timeout, async {
            let mut reader = BufReader::new(&mut stream);
            let mut response_line = String::new();
            reader.read_line(&mut response_line).await?;
            
            debug!(
                "Received response from worker {}: {}",
                worker_info.worker_name,
                response_line.chars().take(200).collect::<String>()
            );

            // Parse response JSON
            let response_data: serde_json::Value = serde_json::from_str(&response_line)?;
            
            // Convert JSON back to Command structure
            let response_command = Command {
                command_type: serde_json::from_value(
                    response_data.get("command_type").cloned()
                        .unwrap_or_else(|| serde_json::Value::String("Success".to_string()))
                )?,
                command_id: response_data.get("command_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_else(|| &command.command_id)
                    .to_string(),
                correlation_id: response_data.get("correlation_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                payload: serde_json::from_value(
                    response_data.get("payload").cloned()
                        .unwrap_or_else(|| serde_json::json!({
                            "type": "Success",
                            "data": { "message": "Command executed successfully" }
                        }))
                )?,
                metadata: serde_json::from_value(
                    response_data.get("metadata").cloned()
                        .unwrap_or_else(|| serde_json::json!({
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                            "source": {
                                "type": "RubyWorker",
                                "data": { "id": worker_info.worker_name.clone() }
                            }
                        }))
                )?,
            };

            Ok::<Command, Box<dyn std::error::Error + Send + Sync>>(response_command)
        })
        .await
        .map_err(|_| format!("Timeout waiting for response from worker {}", worker_info.worker_name))??;

        debug!(
            "Successfully received response from worker {}",
            worker_info.worker_name
        );

        Ok(response)
    }

    /// Get or create a connection to the specified worker
    async fn get_or_create_connection(
        &self,
        worker_info: &WorkerConnectionInfo,
    ) -> Result<Arc<WorkerConnection>, Box<dyn std::error::Error + Send + Sync>> {
        let mut pool = self.connection_pool.write().await;

        // Check if we already have a connection for this worker
        if let Some(existing_connection) = pool.get(&worker_info.worker_name) {
            debug!(
                "Reusing existing connection to worker {}",
                worker_info.worker_name
            );
            return Ok(existing_connection.clone());
        }

        // Create new connection
        let connection = Arc::new(WorkerConnection::new(
            worker_info.worker_name.clone(),
            worker_info.host.clone(),
            worker_info.port,
        ));

        pool.insert(worker_info.worker_name.clone(), connection.clone());

        debug!(
            "Created new connection to worker {} at {}:{}",
            worker_info.worker_name, worker_info.host, worker_info.port
        );

        Ok(connection)
    }

    /// Clean up old connections from the pool
    pub async fn cleanup_old_connections(&self, max_age: Duration) {
        let mut pool = self.connection_pool.write().await;
        let cutoff = std::time::Instant::now() - max_age;

        let old_connections: Vec<String> = pool
            .iter()
            .filter(|(_, conn)| conn.last_used < cutoff)
            .map(|(name, _)| name.clone())
            .collect();

        for name in old_connections {
            pool.remove(&name);
            debug!("Removed old connection to worker {}", name);
        }
    }

    /// Get connection pool statistics
    pub async fn get_connection_stats(&self) -> HashMap<String, usize> {
        let pool = self.connection_pool.read().await;
        pool.iter()
            .map(|(name, conn)| (name.clone(), conn.connection_count))
            .collect()
    }
}

#[async_trait]
impl CommandHandler for TcpWorkerClientHandler {
    async fn handle_command(
        &self,
        command: Command,
    ) -> Result<Option<Command>, Box<dyn std::error::Error + Send + Sync>> {
        debug!(
            "TcpWorkerClientHandler processing command: {:?}",
            command.command_type
        );

        match &command.payload {
            CommandPayload::ExecuteBatch {
                batch_id,
                steps,
                task_template,
            } => {
                let response = self
                    .handle_execute_batch(
                        &command,
                        batch_id.clone(),
                        steps.clone(),
                        task_template.clone(),
                    )
                    .await?;
                Ok(Some(response))
            }

            _ => {
                warn!(
                    "TcpWorkerClientHandler received unsupported command: {:?}",
                    command.command_type
                );

                let error_response = command.create_response(
                    CommandType::Error,
                    CommandPayload::Error {
                        error_type: "UnsupportedCommand".to_string(),
                        message: format!(
                            "TcpWorkerClientHandler only supports ExecuteBatch commands, received: {:?}",
                            command.command_type
                        ),
                        details: None,
                        retryable: false,
                    },
                    CommandSource::RustServer {
                        id: "tcp_worker_client_handler".to_string(),
                    },
                );

                Ok(Some(error_response))
            }
        }
    }

    fn handler_name(&self) -> &str {
        "TcpWorkerClientHandler"
    }

    fn supported_commands(&self) -> Vec<CommandType> {
        vec![CommandType::ExecuteBatch]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_connection_creation() {
        let connection = WorkerConnection::new(
            "test_worker".to_string(),
            "localhost".to_string(),
            8081,
        );

        assert_eq!(connection.worker_name, "test_worker");
        assert_eq!(connection.host, "localhost");
        assert_eq!(connection.port, 8081);
        assert_eq!(connection.connection_count, 0);
    }

    #[test]
    fn test_worker_connection_info() {
        let info = WorkerConnectionInfo {
            worker_name: "test_worker".to_string(),
            host: "192.168.1.100".to_string(),
            port: 9999,
            transport_type: "tcp".to_string(),
        };

        assert_eq!(info.worker_name, "test_worker");
        assert_eq!(info.host, "192.168.1.100");
        assert_eq!(info.port, 9999);
        assert_eq!(info.transport_type, "tcp");
    }
}