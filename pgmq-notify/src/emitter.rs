//! Event emitters for PGMQ notifications

use async_trait::async_trait;
use sqlx::PgPool;
use tracing::{debug, error, instrument, warn};

use crate::config::PgmqNotifyConfig;
use crate::error::{PgmqNotifyError, Result};
use crate::events::{
    BatchReadyEvent, MessageReadyEvent, MessageWithPayloadEvent, PgmqNotifyEvent, QueueCreatedEvent,
};

/// Trait for emitting PGMQ notifications
#[async_trait]
pub trait PgmqNotifyEmitter: Send + Sync {
    /// Emit a queue created event
    async fn emit_queue_created(&self, event: QueueCreatedEvent) -> Result<()>;

    /// Emit a message ready event (signal only, for large messages)
    async fn emit_message_ready(&self, event: MessageReadyEvent) -> Result<()>;

    /// Emit a message with payload event (TAS-133, for small messages)
    async fn emit_message_with_payload(&self, event: MessageWithPayloadEvent) -> Result<()>;

    /// Emit a batch ready event
    async fn emit_batch_ready(&self, event: BatchReadyEvent) -> Result<()>;

    /// Emit a generic PGMQ event
    async fn emit_event(&self, event: PgmqNotifyEvent) -> Result<()>;

    /// Check if emitter is healthy and can send notifications
    async fn is_healthy(&self) -> bool;
}

/// Database-backed emitter using `PostgreSQL` NOTIFY
pub struct DbEmitter {
    pool: PgPool,
    config: PgmqNotifyConfig,
}

impl std::fmt::Debug for DbEmitter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DbEmitter")
            .field("config", &self.config)
            .field("pool", &"PgPool")
            .finish()
    }
}

impl DbEmitter {
    /// Create a new database emitter
    pub fn new(pool: PgPool, config: PgmqNotifyConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self { pool, config })
    }

    /// Get the configuration
    #[must_use]
    pub fn config(&self) -> &PgmqNotifyConfig {
        &self.config
    }

    /// Build notification payload with size validation
    fn build_payload(&self, event: &PgmqNotifyEvent) -> Result<String> {
        let payload = if self.config.include_metadata {
            serde_json::to_string(event)?
        } else {
            // Create a minimal payload without metadata
            let minimal_event = match event {
                PgmqNotifyEvent::QueueCreated(e) => {
                    PgmqNotifyEvent::QueueCreated(QueueCreatedEvent {
                        queue_name: e.queue_name.clone(),
                        namespace: e.namespace.clone(),
                        created_at: e.created_at,
                        metadata: std::collections::HashMap::new(),
                    })
                }
                PgmqNotifyEvent::MessageReady(e) => {
                    PgmqNotifyEvent::MessageReady(MessageReadyEvent {
                        msg_id: e.msg_id,
                        queue_name: e.queue_name.clone(),
                        namespace: e.namespace.clone(),
                        ready_at: e.ready_at,
                        metadata: std::collections::HashMap::new(),
                        visibility_timeout_seconds: e.visibility_timeout_seconds,
                    })
                }
                PgmqNotifyEvent::BatchReady(e) => PgmqNotifyEvent::BatchReady(BatchReadyEvent {
                    msg_ids: e.msg_ids.clone(),
                    queue_name: e.queue_name.clone(),
                    namespace: e.namespace.clone(),
                    message_count: e.message_count,
                    ready_at: e.ready_at,
                    metadata: std::collections::HashMap::new(),
                    delay_seconds: e.delay_seconds,
                }),
                // MessageWithPayload doesn't have metadata to strip, pass through as-is
                PgmqNotifyEvent::MessageWithPayload(e) => {
                    PgmqNotifyEvent::MessageWithPayload(e.clone())
                }
            };
            serde_json::to_string(&minimal_event)?
        };

        // Check payload size limit
        if payload.len() > self.config.max_payload_size {
            return Err(PgmqNotifyError::config(format!(
                "Payload size {} exceeds limit {}",
                payload.len(),
                self.config.max_payload_size
            )));
        }

        Ok(payload)
    }

    /// Send notification to `PostgreSQL` channel
    #[instrument(skip(self, payload), fields(channel = %channel))]
    async fn notify_channel(&self, channel: &str, payload: &str) -> Result<()> {
        debug!("Sending notification to channel: {}", channel);

        let sql = format!("NOTIFY {}, $1", channel);
        sqlx::query(&sql)
            .bind(payload)
            .execute(&self.pool)
            .await
            .map_err(|e| {
                error!("Failed to send notification to channel {}: {}", channel, e);
                PgmqNotifyError::Database(e)
            })?;

        debug!("Successfully sent notification to channel: {}", channel);
        Ok(())
    }
}

#[async_trait]
impl PgmqNotifyEmitter for DbEmitter {
    #[instrument(skip(self, event), fields(queue = %event.queue_name, namespace = %event.namespace))]
    async fn emit_queue_created(&self, event: QueueCreatedEvent) -> Result<()> {
        let pgmq_event = PgmqNotifyEvent::QueueCreated(event);
        let payload = self.build_payload(&pgmq_event)?;
        let channel = self.config.queue_created_channel();

        self.notify_channel(&channel, &payload).await
    }

    #[instrument(skip(self, event), fields(msg_id = %event.msg_id, queue = %event.queue_name, namespace = %event.namespace))]
    async fn emit_message_ready(&self, event: MessageReadyEvent) -> Result<()> {
        let namespace = event.namespace.clone();
        let pgmq_event = PgmqNotifyEvent::MessageReady(event);
        let payload = self.build_payload(&pgmq_event)?;

        // Send to both namespace-specific and global channels
        let namespace_channel = self.config.message_ready_channel(&namespace);
        let global_channel = self.config.global_message_ready_channel();

        // Send to namespace-specific channel first
        self.notify_channel(&namespace_channel, &payload).await?;

        // Also send to global channel for listeners monitoring all namespaces
        if namespace_channel != global_channel {
            self.notify_channel(&global_channel, &payload).await?;
        }

        Ok(())
    }

    #[instrument(skip(self, event), fields(msg_count = %event.message_count, queue = %event.queue_name, namespace = %event.namespace))]
    async fn emit_batch_ready(&self, event: BatchReadyEvent) -> Result<()> {
        let namespace = event.namespace.clone();
        let pgmq_event = PgmqNotifyEvent::BatchReady(event);
        let payload = self.build_payload(&pgmq_event)?;

        // Send to both namespace-specific and global channels (same pattern as MessageReady)
        let namespace_channel = self.config.message_ready_channel(&namespace);
        let global_channel = self.config.global_message_ready_channel();

        // Send to namespace-specific channel first
        self.notify_channel(&namespace_channel, &payload).await?;

        // Also send to global channel for listeners monitoring all namespaces
        if namespace_channel != global_channel {
            self.notify_channel(&global_channel, &payload).await?;
        }

        Ok(())
    }

    #[instrument(skip(self, event), fields(msg_id = %event.msg_id, queue = %event.queue_name, namespace = %event.namespace))]
    async fn emit_message_with_payload(&self, event: MessageWithPayloadEvent) -> Result<()> {
        let namespace = event.namespace.clone();
        let pgmq_event = PgmqNotifyEvent::MessageWithPayload(event);
        let payload = self.build_payload(&pgmq_event)?;

        // Send to both namespace-specific and global channels (same pattern as MessageReady)
        let namespace_channel = self.config.message_ready_channel(&namespace);
        let global_channel = self.config.global_message_ready_channel();

        // Send to namespace-specific channel first
        self.notify_channel(&namespace_channel, &payload).await?;

        // Also send to global channel for listeners monitoring all namespaces
        if namespace_channel != global_channel {
            self.notify_channel(&global_channel, &payload).await?;
        }

        Ok(())
    }

    async fn emit_event(&self, event: PgmqNotifyEvent) -> Result<()> {
        match event {
            PgmqNotifyEvent::QueueCreated(e) => self.emit_queue_created(e).await,
            PgmqNotifyEvent::MessageReady(e) => self.emit_message_ready(e).await,
            PgmqNotifyEvent::MessageWithPayload(e) => self.emit_message_with_payload(e).await,
            PgmqNotifyEvent::BatchReady(e) => self.emit_batch_ready(e).await,
        }
    }

    async fn is_healthy(&self) -> bool {
        // Simple health check - try to get a connection from the pool
        match self.pool.acquire().await {
            Ok(_) => true,
            Err(e) => {
                warn!("Database emitter health check failed: {}", e);
                false
            }
        }
    }
}

/// No-operation emitter for testing and disabled scenarios
#[derive(Debug, Clone)]
pub struct NoopEmitter {
    config: PgmqNotifyConfig,
}

impl NoopEmitter {
    /// Create a new no-op emitter
    pub fn new(config: PgmqNotifyConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self { config })
    }

    /// Get the configuration
    #[must_use]
    pub fn config(&self) -> &PgmqNotifyConfig {
        &self.config
    }
}

#[async_trait]
impl PgmqNotifyEmitter for NoopEmitter {
    async fn emit_queue_created(&self, event: QueueCreatedEvent) -> Result<()> {
        debug!(
            "NoopEmitter: Would emit queue created event for {}",
            event.queue_name
        );
        Ok(())
    }

    async fn emit_message_ready(&self, event: MessageReadyEvent) -> Result<()> {
        debug!(
            "NoopEmitter: Would emit message ready event for msg {} in queue {}",
            event.msg_id, event.queue_name
        );
        Ok(())
    }

    async fn emit_batch_ready(&self, event: BatchReadyEvent) -> Result<()> {
        debug!(
            "NoopEmitter: Would emit batch ready event for {} messages in queue {}",
            event.message_count, event.queue_name
        );
        Ok(())
    }

    async fn emit_message_with_payload(&self, event: MessageWithPayloadEvent) -> Result<()> {
        debug!(
            "NoopEmitter: Would emit message with payload event for msg {} in queue {}",
            event.msg_id, event.queue_name
        );
        Ok(())
    }

    async fn emit_event(&self, event: PgmqNotifyEvent) -> Result<()> {
        debug!("NoopEmitter: Would emit event: {:?}", event.event_type());
        Ok(())
    }

    async fn is_healthy(&self) -> bool {
        true // No-op emitter is always healthy
    }
}

/// Factory for creating emitters based on configuration
#[derive(Debug)]
pub struct EmitterFactory;

impl EmitterFactory {
    /// Create a database emitter
    pub fn database(pool: PgPool, config: PgmqNotifyConfig) -> Result<DbEmitter> {
        DbEmitter::new(pool, config)
    }

    /// Create a no-op emitter
    pub fn noop(config: PgmqNotifyConfig) -> Result<NoopEmitter> {
        NoopEmitter::new(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_noop_emitter() {
        let config = PgmqNotifyConfig::default();
        let emitter = NoopEmitter::new(config).unwrap();

        let queue_event = QueueCreatedEvent::new("test_queue", "test");
        let message_event = MessageReadyEvent::new(123, "test_queue", "test");

        // All operations should succeed with no-op
        assert!(emitter.emit_queue_created(queue_event).await.is_ok());
        assert!(emitter.emit_message_ready(message_event).await.is_ok());
        assert!(emitter.is_healthy().await);
    }

    #[test]
    fn test_payload_building() {
        let config = PgmqNotifyConfig::default();
        // Skip the actual database test but test configuration
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_payload_size_limit() {
        let mut large_metadata = HashMap::new();
        for i in 0..1000 {
            large_metadata.insert(format!("key_{}", i), "x".repeat(100));
        }

        let event = QueueCreatedEvent::new("test_queue", "test").with_metadata(large_metadata);

        let pgmq_event = PgmqNotifyEvent::QueueCreated(event);
        let json = serde_json::to_string(&pgmq_event).unwrap();

        // This should exceed the default 7800 byte limit
        assert!(json.len() > 7800);
    }

    #[tokio::test]
    async fn test_emitter_factory() {
        let config = PgmqNotifyConfig::default();

        // Test no-op factory
        let noop = EmitterFactory::noop(config.clone()).unwrap();
        assert!(noop.is_healthy().await);
    }

    #[test]
    fn test_channel_naming() {
        let config = PgmqNotifyConfig::new().with_channels_prefix("test_app");

        assert_eq!(
            config.queue_created_channel(),
            "test_app.pgmq_queue_created"
        );
        assert_eq!(
            config.message_ready_channel("orders"),
            "test_app.pgmq_message_ready.orders"
        );
        assert_eq!(
            config.global_message_ready_channel(),
            "test_app.pgmq_message_ready"
        );
    }
}
