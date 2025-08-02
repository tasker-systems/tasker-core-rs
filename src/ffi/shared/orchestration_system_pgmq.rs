//! # Simplified Orchestration System (pgmq-based)
//!
//! Simplified orchestration system for pgmq architecture, removing TCP complexity

use crate::database::DatabaseConnection;
use crate::messaging::PgmqClient;
use crate::orchestration::state_manager::StateManager;
use crate::orchestration::task_initializer::TaskInitializer;
use crate::orchestration::workflow_coordinator::WorkflowCoordinator;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::info;

/// Simplified orchestration system for pgmq architecture
pub struct OrchestrationSystemPgmq {
    pub database_pool: PgPool,
    pub pgmq_client: Arc<PgmqClient>,
    pub workflow_coordinator: WorkflowCoordinator,
    pub state_manager: StateManager,
    pub task_initializer: TaskInitializer,
}

impl OrchestrationSystemPgmq {
    /// Create new orchestration system
    pub async fn new() -> Result<Arc<Self>, Box<dyn std::error::Error + Send + Sync>> {
        info!("🚀 Initializing pgmq-based orchestration system");

        // Initialize database connection
        let db_connection = DatabaseConnection::new().await?;
        let database_pool = db_connection.pool().clone();

        // Initialize pgmq client with shared pool
        let pgmq_client = Arc::new(PgmqClient::new_with_pool(database_pool.clone()).await);

        // Initialize orchestration components
        let event_publisher = crate::events::EventPublisher::new();
        let config_manager = Arc::new(crate::orchestration::config::ConfigurationManager::new());
        let config = crate::orchestration::workflow_coordinator::WorkflowCoordinatorConfig::from_config_manager(&config_manager);

        let sql_executor =
            crate::database::sql_functions::SqlFunctionExecutor::new(database_pool.clone());
        let state_manager = StateManager::new(
            sql_executor.clone(),
            event_publisher.clone(),
            database_pool.clone(),
        );
        let task_initializer = TaskInitializer::with_state_manager_and_registry(
            database_pool.clone(),
            crate::orchestration::task_initializer::TaskInitializationConfig::default(),
            event_publisher.clone(),
            Arc::new(crate::registry::TaskHandlerRegistry::with_event_publisher(
                database_pool.clone(),
                event_publisher.clone(),
            )),
        );

        // Create workflow coordinator with pgmq client integration
        let workflow_coordinator = WorkflowCoordinator::new(
            database_pool.clone(),
            config,
            config_manager,
            event_publisher.clone(),
            crate::registry::TaskHandlerRegistry::with_event_publisher(
                database_pool.clone(),
                event_publisher.clone(),
            ),
            pgmq_client.clone(),
        );

        let system = Arc::new(Self {
            database_pool,
            pgmq_client,
            workflow_coordinator,
            state_manager,
            task_initializer,
        });

        info!("✅ pgmq-based orchestration system initialized");
        Ok(system)
    }

    /// Get database pool reference
    pub fn database_pool(&self) -> &PgPool {
        &self.database_pool
    }

    /// Get pgmq client reference
    pub fn pgmq_client(&self) -> Arc<PgmqClient> {
        self.pgmq_client.clone()
    }

    /// Enqueue ready steps for a task using pgmq architecture
    pub async fn enqueue_ready_steps(
        &self,
        task_id: i64,
    ) -> Result<
        crate::orchestration::types::TaskOrchestrationResult,
        Box<dyn std::error::Error + Send + Sync>,
    > {
        info!(
            task_id = task_id,
            "🚀 pgmq: Enqueueing ready steps for task"
        );

        // Use workflow coordinator to discover and enqueue steps
        let result = self
            .workflow_coordinator
            .execute_task_workflow(task_id)
            .await?;

        info!(
            task_id = task_id,
            result = ?result,
            "✅ pgmq: Task workflow execution completed"
        );

        Ok(result)
    }

    /// Initialize standard namespace queues
    pub async fn initialize_queues(
        &self,
        namespaces: &[&str],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("🏗️ pgmq: Initializing namespace queues");

        self.pgmq_client
            .initialize_namespace_queues(namespaces)
            .await?;

        info!("✅ pgmq: All namespace queues initialized");
        Ok(())
    }
}
