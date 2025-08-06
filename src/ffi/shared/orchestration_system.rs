//! # Shared Orchestration System
//!
//! Language-agnostic orchestration system core for the embedded FFI bridge.
//! Uses pgmq-based architecture for reliable, queue-based workflow orchestration.

use crate::database::DatabaseConnection;
use crate::messaging::PgmqClient;
use crate::orchestration::orchestration_loop::OrchestrationLoop;
use crate::orchestration::state_manager::StateManager;
use crate::orchestration::step_result_processor::StepResultProcessor;
use crate::orchestration::task_initializer::TaskInitializer;
use crate::orchestration::task_request_processor::TaskRequestProcessor;
use crate::orchestration::workflow_coordinator::WorkflowCoordinator;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

/// Orchestration system for embedded FFI bridge
pub struct OrchestrationSystem {
    pub database_pool: PgPool,
    pub pgmq_client: Arc<PgmqClient>,
    pub workflow_coordinator: WorkflowCoordinator,
    pub state_manager: StateManager,
    pub shared_task_initializer: Arc<TaskInitializer>,
    pub event_publisher: crate::events::EventPublisher,
    pub orchestration_loop: Arc<OrchestrationLoop>,
    pub task_request_processor: Arc<TaskRequestProcessor>,
    pub step_result_processor: Arc<StepResultProcessor>,
}

impl OrchestrationSystem {
    /// Create new orchestration system
    pub async fn new() -> Result<Arc<Self>, Box<dyn std::error::Error + Send + Sync>> {
        info!("🚀 Initializing orchestration system");

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
            pgmq_client.clone(),
        );

        // Create orchestration loop for continuous step enqueueing
        let orchestration_loop = OrchestrationLoop::new(
            database_pool.clone(),
            (*pgmq_client).clone(),
            format!("embedded-orchestrator-{}", Uuid::new_v4()),
        ).await?;

        // Create task handler registry for task request processor
        let task_handler_registry = Arc::new(crate::registry::TaskHandlerRegistry::with_event_publisher(
            database_pool.clone(),
            event_publisher.clone(),
        ));

        // Create task request processor configuration
        let task_request_config = crate::orchestration::task_request_processor::TaskRequestProcessorConfig {
            request_queue_name: "task_requests_queue".to_string(),
            polling_interval_seconds: 1,
            visibility_timeout_seconds: 300,
            batch_size: 10,
            max_processing_attempts: 3,
        };

        // Create task request processor for processing incoming task requests
        // We need to share the task_initializer, so wrap it in Arc
        let shared_task_initializer = Arc::new(task_initializer);
        let task_request_processor = TaskRequestProcessor::new(
            pgmq_client.clone(),
            task_handler_registry,
            Arc::clone(&shared_task_initializer),
            task_request_config,
        );

        // Create step result processor for processing step results
        let step_result_processor = StepResultProcessor::new(
            database_pool.clone(),
            (*pgmq_client).clone(),
        ).await?;

        let system = Arc::new(Self {
            database_pool,
            pgmq_client,
            workflow_coordinator,
            state_manager,
            shared_task_initializer,
            event_publisher,
            orchestration_loop: Arc::new(orchestration_loop),
            task_request_processor: Arc::new(task_request_processor),
            step_result_processor: Arc::new(step_result_processor),
        });

        info!("✅ Orchestration system initialized");
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

    /// Initialize a task using the embedded orchestration system
    pub async fn initialize_task(
        &self,
        task_request: crate::models::core::task_request::TaskRequest,
    ) -> Result<crate::orchestration::TaskInitializationResult, crate::error::TaskerError> {
        info!(
            namespace = %task_request.namespace,
            name = %task_request.name,
            version = %task_request.version,
            "🚀 EMBEDDED: Initializing task via orchestration system"
        );

        // Use the embedded task initializer with full registry support
        let result = self.shared_task_initializer.create_task_from_request(task_request).await
            .map_err(|e| crate::error::TaskerError::DatabaseError(format!("Task initialization failed: {}", e)))?;

        info!(
            task_id = result.task_id,
            step_count = result.step_count,
            "✅ EMBEDDED: Task initialized successfully via orchestration system"
        );

        Ok(result)
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

    /// Start the complete orchestration system (all processors)
    pub async fn start_orchestration_system(
        &self,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        info!("🚀 Starting complete orchestration system for embedded mode");
        
        // Start all three processors concurrently
        // This mirrors the full orchestration system architecture
        let orchestration_loop = Arc::clone(&self.orchestration_loop);
        let task_request_processor = Arc::clone(&self.task_request_processor);
        let step_result_processor = Arc::clone(&self.step_result_processor);

        let orchestration_handle = tokio::spawn(async move {
            info!("🔄 ORCHESTRATION_SYSTEM: Orchestration loop task started - beginning continuous operation");
            if let Err(e) = orchestration_loop.run_continuous().await {
                error!("❌ ORCHESTRATION_SYSTEM: Orchestration loop failed: {}", e);
            } else {
                info!("✅ ORCHESTRATION_SYSTEM: Orchestration loop completed successfully");
            }
        });

        let task_request_handle = tokio::spawn(async move {
            if let Err(e) = task_request_processor.start_processing_loop().await {
                tracing::error!("Task request processor failed: {}", e);
            }
        });

        let step_result_handle = tokio::spawn(async move {
            if let Err(e) = step_result_processor.start_processing_loop().await {
                tracing::error!("Step result processor failed: {}", e);
            }
        });

        info!("✅ All orchestration processors started");

        // Wait for all processors to complete (they should run indefinitely)
        let (orchestration_result, task_request_result, step_result_result) = tokio::join!(
            orchestration_handle,
            task_request_handle,
            step_result_handle
        );

        // Log any errors that caused the processors to exit
        if let Err(e) = orchestration_result {
            tracing::error!("Orchestration loop task panicked: {}", e);
        }
        if let Err(e) = task_request_result {
            tracing::error!("Task request processor task panicked: {}", e);
        }
        if let Err(e) = step_result_result {
            tracing::error!("Step result processor task panicked: {}", e);
        }

        Ok(())
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

/// Initialize unified orchestration system (global singleton for FFI modules)
pub fn initialize_unified_orchestration_system() -> Arc<OrchestrationSystem> {
    static GLOBAL_SYSTEM: std::sync::OnceLock<Arc<OrchestrationSystem>> = std::sync::OnceLock::new();
    
    GLOBAL_SYSTEM.get_or_init(|| {
        info!("🎯 Creating global unified orchestration system");
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        rt.block_on(async {
            OrchestrationSystem::new().await.expect("Failed to initialize orchestration system")
        })
    }).clone()
}

/// Execute async code synchronously (for FFI modules)
pub fn execute_async<F, R>(future: F) -> R
where
    F: std::future::Future<Output = R>,
{
    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");  
    rt.block_on(future)
}
