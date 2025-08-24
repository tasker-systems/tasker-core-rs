//! Worker foundation core bootstrap system
//! Mirrors OrchestrationCore patterns for consistency

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

use tasker_shared::{
    config::ConfigManager,
    database::Connection,
    messaging::UnifiedPgmqClient,
    registry::{HandlerRegistry, TaskTemplateRegistry},
};

use crate::{
    config::WorkerConfig,
    error::{Result, WorkerError},
    event_publisher::EventPublisher,
    event_subscriber::EventSubscriber,
    events::InProcessEventSystem,
    worker::{
        coordinator::WorkerLoopCoordinator, health_monitor::WorkerHealthMonitor,
        resource_validator::WorkerResourceValidator,
    },
};

/// Worker foundation core bootstrap system
/// Mirrors OrchestrationCore patterns for consistency
pub struct WorkerCore {
    config_manager: Arc<ConfigManager>,
    database_connection: Arc<Connection>,
    messaging_client: Arc<UnifiedPgmqClient>,
    coordinator: Arc<RwLock<WorkerLoopCoordinator>>,
    task_template_registry: Arc<TaskTemplateRegistry>,
    handler_registry: Arc<HandlerRegistry>,
    event_system: Arc<InProcessEventSystem>,
    event_publisher: Arc<EventPublisher>,
    event_subscriber: Arc<EventSubscriber>,
    health_monitor: Arc<WorkerHealthMonitor>,
    resource_validator: Arc<WorkerResourceValidator>,
}

impl WorkerCore {
    /// Create new WorkerCore instance
    pub async fn new() -> Result<Self> {
        info!("🚀 Initializing WorkerCore...");

        // Load configuration
        let config_manager = Arc::new(ConfigManager::new().await?);
        let worker_config = WorkerConfig::from_config_manager(&config_manager)?;

        // Initialize database connection with worker-specific pool
        let database_connection = Arc::new(
            Connection::new_with_config(config_manager.database_config())
                .await
                .map_err(|e| WorkerError::Database(e))?,
        );

        // Run migrations if needed
        Self::ensure_migrations(&database_connection).await?;

        // Initialize messaging client
        let messaging_client = Arc::new(
            UnifiedPgmqClient::new_with_config(
                database_connection.clone(),
                config_manager.pgmq_config(),
                config_manager.circuit_breaker_config(),
            )
            .await
            .map_err(|e| WorkerError::Messaging(e.to_string()))?,
        );

        // Create queues for configured namespaces
        for namespace in &worker_config.namespaces {
            let queue_name = format!("{}_queue", namespace);
            messaging_client
                .create_queue(&queue_name)
                .await
                .map_err(|e| {
                    WorkerError::Messaging(format!("Failed to create queue {}: {}", queue_name, e))
                })?;
            info!("✅ Created queue: {}", queue_name);
        }

        // Initialize registries
        let task_template_registry = Arc::new(
            TaskTemplateRegistry::new(database_connection.clone())
                .await
                .map_err(|e| WorkerError::Configuration(e.to_string()))?,
        );

        let handler_registry = Arc::new(
            HandlerRegistry::new()
                .await
                .map_err(|e| WorkerError::Configuration(e.to_string()))?,
        );

        // Initialize in-process event system
        let event_system = Arc::new(InProcessEventSystem::new());

        // Initialize event publisher and subscriber
        let event_publisher = Arc::new(EventPublisher::new(event_system.clone()));
        let event_subscriber = Arc::new(EventSubscriber::new(event_system.clone()));

        // Initialize health monitor
        let health_monitor = Arc::new(
            WorkerHealthMonitor::new(&worker_config)
                .await
                .map_err(|e| WorkerError::Other(e))?,
        );

        // Initialize resource validator
        let resource_validator = Arc::new(
            WorkerResourceValidator::new(&worker_config)
                .await
                .map_err(|e| WorkerError::Other(e))?,
        );

        // Initialize coordinator (similar to OrchestrationLoopCoordinator)
        let coordinator = Arc::new(RwLock::new(
            WorkerLoopCoordinator::new(
                config_manager.clone(),
                database_connection.clone(),
                messaging_client.clone(),
                task_template_registry.clone(),
                handler_registry.clone(),
                event_publisher.clone(),
                worker_config.clone(),
            )
            .await?,
        ));

        info!("✅ WorkerCore initialized successfully");

        Ok(Self {
            config_manager,
            database_connection,
            messaging_client,
            coordinator,
            task_template_registry,
            handler_registry,
            event_system,
            event_publisher,
            event_subscriber,
            health_monitor,
            resource_validator,
        })
    }

    /// Create WorkerCore with custom configuration (for testing)
    pub async fn new_with_config(config_manager: Arc<ConfigManager>) -> Result<Self> {
        info!("🚀 Initializing WorkerCore with custom config...");

        let worker_config = WorkerConfig::from_config_manager(&config_manager)?;

        // Initialize database connection
        let database_connection = Arc::new(
            Connection::new_with_config(config_manager.database_config())
                .await
                .map_err(|e| WorkerError::Database(e))?,
        );

        // Run migrations if needed
        Self::ensure_migrations(&database_connection).await?;

        // Initialize messaging client
        let messaging_client = Arc::new(
            UnifiedPgmqClient::new_with_config(
                database_connection.clone(),
                config_manager.pgmq_config(),
                config_manager.circuit_breaker_config(),
            )
            .await
            .map_err(|e| WorkerError::Messaging(e.to_string()))?,
        );

        // Create queues
        for namespace in &worker_config.namespaces {
            let queue_name = format!("{}_queue", namespace);
            messaging_client
                .create_queue(&queue_name)
                .await
                .map_err(|e| {
                    WorkerError::Messaging(format!("Failed to create queue {}: {}", queue_name, e))
                })?;
        }

        // Initialize registries
        let task_template_registry = Arc::new(
            TaskTemplateRegistry::new(database_connection.clone())
                .await
                .map_err(|e| WorkerError::Configuration(e.to_string()))?,
        );

        let handler_registry = Arc::new(
            HandlerRegistry::new()
                .await
                .map_err(|e| WorkerError::Configuration(e.to_string()))?,
        );

        // Initialize event system
        let event_system = Arc::new(InProcessEventSystem::new());
        let event_publisher = Arc::new(EventPublisher::new(event_system.clone()));
        let event_subscriber = Arc::new(EventSubscriber::new(event_system.clone()));

        // Initialize monitors
        let health_monitor = Arc::new(
            WorkerHealthMonitor::new(&worker_config)
                .await
                .map_err(|e| WorkerError::Other(e))?,
        );

        let resource_validator = Arc::new(
            WorkerResourceValidator::new(&worker_config)
                .await
                .map_err(|e| WorkerError::Other(e))?,
        );

        // Initialize coordinator
        let coordinator = Arc::new(RwLock::new(
            WorkerLoopCoordinator::new(
                config_manager.clone(),
                database_connection.clone(),
                messaging_client.clone(),
                task_template_registry.clone(),
                handler_registry.clone(),
                event_publisher.clone(),
                worker_config.clone(),
            )
            .await?,
        ));

        info!("✅ WorkerCore initialized with custom config");

        Ok(Self {
            config_manager,
            database_connection,
            messaging_client,
            coordinator,
            task_template_registry,
            handler_registry,
            event_system,
            event_publisher,
            event_subscriber,
            health_monitor,
            resource_validator,
        })
    }

    /// Start worker system
    pub async fn start(&self) -> Result<()> {
        info!("🔄 Starting WorkerCore...");

        // Validate resources before starting
        self.resource_validator.validate_startup().await?;

        // Start health monitoring
        self.health_monitor.start().await?;

        // Start coordinator (which manages WorkerExecutor pools)
        let mut coordinator = self.coordinator.write().await;
        coordinator.start().await?;

        info!("✅ WorkerCore started successfully");
        Ok(())
    }

    /// Graceful shutdown
    pub async fn shutdown(&self) -> Result<()> {
        info!("🛑 Shutting down WorkerCore...");

        // Signal graceful shutdown to coordinator
        let mut coordinator = self.coordinator.write().await;
        coordinator.transition_to_graceful_shutdown().await?;

        // Stop health monitoring
        self.health_monitor.stop().await?;

        // Wait for coordinator to finish processing
        coordinator.wait_for_shutdown().await?;

        info!("✅ WorkerCore shutdown completed");
        Ok(())
    }

    /// Get event subscriber for registering step handlers
    pub fn event_subscriber(&self) -> Arc<EventSubscriber> {
        self.event_subscriber.clone()
    }

    /// Get event publisher for testing
    pub fn event_publisher(&self) -> Arc<EventPublisher> {
        self.event_publisher.clone()
    }

    /// Get health monitor
    pub fn health_monitor(&self) -> Arc<WorkerHealthMonitor> {
        self.health_monitor.clone()
    }

    /// Get database connection
    pub fn database_connection(&self) -> Arc<Connection> {
        self.database_connection.clone()
    }

    /// Get messaging client
    pub fn messaging_client(&self) -> Arc<UnifiedPgmqClient> {
        self.messaging_client.clone()
    }

    /// Ensure database migrations are applied
    async fn ensure_migrations(connection: &Connection) -> Result<()> {
        // Check if migrations are needed
        match connection.check_migrations_status().await {
            Ok(true) => {
                info!("✅ Database migrations are up to date");
                Ok(())
            }
            Ok(false) => {
                error!("❌ Database migrations are not up to date. Please run migrations first.");
                Err(WorkerError::Configuration(
                    "Database migrations not applied".to_string(),
                ))
            }
            Err(e) => {
                error!("❌ Failed to check migration status: {}", e);
                Err(WorkerError::Database(sqlx::Error::Protocol(format!(
                    "Migration check failed: {}",
                    e
                ))))
            }
        }
    }
}