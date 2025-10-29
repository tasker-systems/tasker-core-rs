//! Simplified task readiness fallback system
//!
//! This module manages a simple fallback poller that periodically runs
//! TaskClaimStepEnqueuer::process_batch() to catch any ready tasks that
//! may have been missed by the primary pgmq notification system.

use std::sync::Arc;
use std::time::Duration;
use tracing::info;
use uuid::Uuid;

use crate::orchestration::task_readiness::fallback_poller::{FallbackPoller, FallbackPollerConfig};
use tasker_shared::event_system::DeploymentMode;
use tasker_shared::{SystemContext, TaskerResult};

/// Simplified task readiness system that only manages fallback polling
#[derive(Debug)]
pub struct TaskReadinessEventSystem {
    context: Arc<SystemContext>,
    config: Option<FallbackPollerConfig>,
    fallback_poller: Option<FallbackPoller>,
}

impl TaskReadinessEventSystem {
    /// Create a new task readiness system
    pub fn new(context: Arc<SystemContext>) -> Self {
        Self {
            config: None,
            context,
            fallback_poller: None,
        }
    }

    pub fn set_config(&mut self, config: FallbackPollerConfig) {
        self.config = Some(config);
    }

    pub fn set_fallback_poller(&mut self, poller: FallbackPoller) {
        self.fallback_poller = Some(poller);
    }

    /// Start the fallback poller with the given configuration
    pub async fn start(&mut self) -> TaskerResult<()> {
        let config = {
            if self.config.is_some() {
                self.config.take().unwrap()
            } else {
                FallbackPollerConfig {
                    enabled: self
                        .context
                        .tasker_config
                        .event_systems
                        .task_readiness
                        .deployment_mode
                        .has_polling(),
                    polling_interval: Duration::from_secs(
                        self.context
                            .tasker_config
                            .event_systems
                            .task_readiness
                            .timing
                            .fallback_polling_interval_seconds,
                    ),
                }
            }
        };

        info!(
            system_id = %self.context.processor_uuid(),
            enabled = config.enabled,
            interval_seconds = config.polling_interval.as_secs(),
            "Starting task readiness fallback system"
        );

        if !config.enabled {
            info!("Task readiness fallback polling is disabled");
            return Ok(());
        }

        if let Some(poller) = self.fallback_poller.as_mut() {
            poller.start().await?;
        } else {
            let mut poller = FallbackPoller::new(config, self.context.clone()).await?;
            poller.start().await?;
            self.fallback_poller = Some(poller);
        }

        info!(
            system_id = %self.processor_uuid(),
            "Task readiness fallback system started successfully"
        );

        Ok(())
    }

    /// Stop the fallback poller
    pub async fn stop(&mut self) -> TaskerResult<()> {
        info!(
            system_id = %self.processor_uuid(),
            "Stopping task readiness fallback system"
        );

        if let Some(mut poller) = self.fallback_poller.take() {
            poller.stop().await?;
        }

        Ok(())
    }

    /// Check if the system is running
    pub fn is_running(&self) -> bool {
        self.fallback_poller
            .as_ref()
            .map(|p| p.is_running())
            .unwrap_or(false)
    }

    /// Get the system ID
    pub fn processor_uuid(&self) -> Uuid {
        self.context.processor_uuid()
    }

    pub fn deployment_mode(&self) -> DeploymentMode {
        if self.config.is_some() {
            DeploymentMode::PollingOnly
        } else {
            DeploymentMode::Disabled
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_task_readiness_system_creation(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let system = TaskReadinessEventSystem::new(context);

        assert!(!system.is_running());
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
    async fn test_task_readiness_system_disabled(
        pool: sqlx::PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let context = Arc::new(SystemContext::with_pool(pool).await?);
        let mut system = TaskReadinessEventSystem::new(context);

        let config = FallbackPollerConfig {
            enabled: false,
            polling_interval: Duration::from_secs(30),
        };

        system.set_config(config);

        system.start().await?;
        assert!(!system.is_running());

        Ok(())
    }
}
