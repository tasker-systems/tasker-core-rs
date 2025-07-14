//! # Global Resource Management
//!
//! Singleton pattern for shared orchestration resources to avoid recreating
//! expensive components (database connections, tokio runtime, event publisher)
//! on every FFI call.

use std::sync::OnceLock;
use tokio::runtime::Runtime;
use tasker_core::events::EventPublisher;
use tasker_core::orchestration::workflow_coordinator::WorkflowCoordinator;
use tasker_core::orchestration::state_manager::StateManager;
use tasker_core::orchestration::task_initializer::TaskInitializer;
use tasker_core::database::sql_functions::SqlFunctionExecutor;
use tasker_core::registry::TaskHandlerRegistry;
use sqlx::PgPool;
use std::sync::Arc;

/// Global orchestration system singleton
static GLOBAL_ORCHESTRATION_SYSTEM: OnceLock<Arc<OrchestrationSystem>> = OnceLock::new();

/// Shared orchestration resources
pub struct OrchestrationSystem {
    pub runtime: Runtime,
    pub database_pool: PgPool,
    pub event_publisher: EventPublisher,
    pub workflow_coordinator: WorkflowCoordinator,
    pub state_manager: StateManager,
    pub task_initializer: TaskInitializer,
    pub task_handler_registry: TaskHandlerRegistry,
}

impl OrchestrationSystem {
    /// Create new orchestration system with all required components
    async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Create tokio runtime for async operations
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;

        // Get database connection
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://tasker:tasker@localhost/tasker_rust_test".to_string());

        let database_pool = PgPool::connect(&database_url).await?;

        // Create core components
        let event_publisher = EventPublisher::new();
        let sql_executor = SqlFunctionExecutor::new(database_pool.clone());
        let state_manager = StateManager::new(sql_executor, event_publisher.clone(), database_pool.clone());
        let task_handler_registry = TaskHandlerRegistry::with_event_publisher(event_publisher.clone());

        // Create task initializer
        let task_initializer = TaskInitializer::with_state_manager(
            database_pool.clone(),
            Default::default(),
            event_publisher.clone()
        );

        // Create workflow coordinator
        let workflow_coordinator = WorkflowCoordinator::new(database_pool.clone());

        Ok(OrchestrationSystem {
            runtime,
            database_pool,
            event_publisher,
            workflow_coordinator,
            state_manager,
            task_initializer,
            task_handler_registry,
        })
    }
}

/// Get or initialize the global orchestration system
pub fn get_global_orchestration_system() -> Arc<OrchestrationSystem> {
    GLOBAL_ORCHESTRATION_SYSTEM.get_or_init(|| {
        // We need to block on async initialization
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create runtime for initialization");

        Arc::new(rt.block_on(async {
            OrchestrationSystem::new().await
                .expect("Failed to initialize orchestration system")
        }))
    }).clone()
}

/// Execute async operation using the global runtime
pub fn execute_async<F, R>(future: F) -> R
where
    F: std::future::Future<Output = R>,
{
    let orchestration = get_global_orchestration_system();
    orchestration.runtime.block_on(future)
}

/// Get the global event publisher
pub fn get_global_event_publisher() -> EventPublisher {
    get_global_orchestration_system().event_publisher.clone()
}

/// Get the global database pool
pub fn get_global_database_pool() -> PgPool {
    get_global_orchestration_system().database_pool.clone()
}

/// Get the global task handler registry
pub fn get_global_task_handler_registry() -> TaskHandlerRegistry {
    get_global_orchestration_system().task_handler_registry.clone()
}
