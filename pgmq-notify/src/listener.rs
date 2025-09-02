//! Event listener for PGMQ notifications using sqlx::PgListener

use async_trait::async_trait;
use futures::StreamExt;
use serde_json;
use sqlx::postgres::PgListener;
use sqlx::PgPool;
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use std::time::SystemTime;
use tokio::sync::mpsc;
use tracing::{debug, error, info, instrument, warn};

use crate::config::PgmqNotifyConfig;
use crate::error::{PgmqNotifyError, Result};
use crate::events::PgmqNotifyEvent;

/// Statistics about the listener
#[derive(Debug, Clone)]
pub struct ListenerStats {
    pub connected: bool,
    pub channels_listening: usize,
    pub events_received: u64,
    pub parse_errors: u64,
    pub connection_errors: u64,
    pub last_event_at: Option<SystemTime>,
    pub last_error_at: Option<SystemTime>,
}

impl Default for ListenerStats {
    fn default() -> Self {
        Self {
            connected: false,
            channels_listening: 0,
            events_received: 0,
            parse_errors: 0,
            connection_errors: 0,
            last_event_at: None,
            last_error_at: None,
        }
    }
}

/// Trait for handling PGMQ notification events
#[async_trait]
pub trait PgmqEventHandler: Send + Sync {
    /// Handle a received PGMQ notification event
    async fn handle_event(&self, event: PgmqNotifyEvent) -> Result<()>;

    /// Handle a notification parsing error
    async fn handle_parse_error(&self, channel: &str, payload: &str, error: PgmqNotifyError) {
        warn!(
            "Failed to parse notification from channel {}: {} - payload: {}",
            channel, error, payload
        );
    }

    /// Handle connection issues
    async fn handle_connection_error(&self, error: PgmqNotifyError) {
        error!("Connection error in PGMQ listener: {}", error);
    }
}

/// PGMQ notification listener using PostgreSQL LISTEN/NOTIFY
pub struct PgmqNotifyListener {
    pool: PgPool,
    config: PgmqNotifyConfig,
    listener: Option<PgListener>,
    listening_channels: Arc<RwLock<HashSet<String>>>,
    stats: Arc<RwLock<ListenerStats>>,
    event_sender: Option<mpsc::UnboundedSender<PgmqNotifyEvent>>,
    event_receiver: Option<mpsc::UnboundedReceiver<PgmqNotifyEvent>>,
}

impl PgmqNotifyListener {
    /// Create a new PGMQ notification listener
    pub async fn new(pool: PgPool, config: PgmqNotifyConfig) -> Result<Self> {
        config.validate()?;

        let (event_sender, event_receiver) = mpsc::unbounded_channel();

        Ok(Self {
            pool,
            config,
            listener: None,
            listening_channels: Arc::new(RwLock::new(HashSet::new())),
            stats: Arc::new(RwLock::new(ListenerStats::default())),
            event_sender: Some(event_sender),
            event_receiver: Some(event_receiver),
        })
    }

    /// Get the configuration
    pub fn config(&self) -> &PgmqNotifyConfig {
        &self.config
    }

    /// Get listener statistics
    pub fn stats(&self) -> ListenerStats {
        self.stats.read().unwrap().clone()
    }

    /// Connect to the database for listening
    #[instrument(skip(self))]
    pub async fn connect(&mut self) -> Result<()> {
        if self.listener.is_some() {
            debug!("Already connected to database");
            return Ok(());
        }

        info!("Connecting PGMQ notification listener to database");

        let listener = PgListener::connect_with(&self.pool).await?;
        self.listener = Some(listener);

        // Update stats
        {
            let mut stats = self.stats.write().unwrap();
            stats.connected = true;
        }

        info!("Successfully connected PGMQ notification listener");
        Ok(())
    }

    /// Disconnect from the database
    #[instrument(skip(self))]
    pub async fn disconnect(&mut self) -> Result<()> {
        if let Some(listener) = self.listener.take() {
            info!("Disconnecting PGMQ notification listener");
            // PgListener will be dropped, closing the connection
            drop(listener);
        }

        // Clear listening channels
        {
            let mut channels = self.listening_channels.write().unwrap();
            channels.clear();
        }

        // Update stats
        {
            let mut stats = self.stats.write().unwrap();
            stats.connected = false;
            stats.channels_listening = 0;
        }

        info!("Disconnected PGMQ notification listener");
        Ok(())
    }

    /// Listen to a specific channel
    #[instrument(skip(self), fields(channel = %channel))]
    pub async fn listen_channel(&mut self, channel: &str) -> Result<()> {
        if self.listener.is_none() {
            return Err(PgmqNotifyError::NotConnected);
        }

        // Check if already listening
        {
            let channels = self.listening_channels.read().unwrap();
            if channels.contains(channel) {
                warn!("Already listening on channel {channel}");
                return Ok(());
            }
        }

        debug!("Starting to listen to channel: {}", channel);

        if let Some(ref mut listener) = self.listener {
            listener.listen(channel).await?;
        }

        // Add to listening channels
        {
            let mut channels = self.listening_channels.write().unwrap();
            channels.insert(channel.to_string());
        }

        // Update stats
        {
            let mut stats = self.stats.write().unwrap();
            stats.channels_listening = self.listening_channels.read().unwrap().len();
        }

        info!("Now listening to channel: {}", channel);
        Ok(())
    }

    /// Stop listening to a specific channel
    #[instrument(skip(self), fields(channel = %channel))]
    pub async fn unlisten_channel(&mut self, channel: &str) -> Result<()> {
        if self.listener.is_none() {
            return Err(PgmqNotifyError::NotConnected);
        }

        debug!("Stopping listening to channel: {}", channel);

        if let Some(ref mut listener) = self.listener {
            listener.unlisten(channel).await?;
        }

        // Remove from listening channels
        {
            let mut channels = self.listening_channels.write().unwrap();
            channels.remove(channel);
        }

        // Update stats
        {
            let mut stats = self.stats.write().unwrap();
            stats.channels_listening = self.listening_channels.read().unwrap().len();
        }

        info!("Stopped listening to channel: {}", channel);
        Ok(())
    }

    /// Listen to queue created events
    pub async fn listen_queue_created(&mut self) -> Result<()> {
        let channel = self.config.queue_created_channel();
        self.listen_channel(&channel).await
    }

    /// Listen to message ready events for a specific namespace
    pub async fn listen_message_ready_for_namespace(&mut self, namespace: &str) -> Result<()> {
        let channel = self.config.message_ready_channel(namespace);
        self.listen_channel(&channel).await
    }

    /// Listen to all message ready events (global)
    pub async fn listen_message_ready_global(&mut self) -> Result<()> {
        let channel = self.config.global_message_ready_channel();
        self.listen_channel(&channel).await
    }

    /// Listen to default namespaces from configuration
    pub async fn listen_default_namespaces(&mut self) -> Result<()> {
        let namespaces: Vec<String> = self.config.default_namespaces.iter().cloned().collect();

        for namespace in namespaces {
            self.listen_message_ready_for_namespace(&namespace).await?;
        }

        Ok(())
    }

    /// Get the next notification event (blocking)
    pub async fn next_event(&mut self) -> Result<Option<PgmqNotifyEvent>> {
        if let Some(ref mut receiver) = self.event_receiver {
            Ok(receiver.recv().await)
        } else {
            Err(PgmqNotifyError::NotConnected)
        }
    }

    /// Start listening loop with an event handler
    #[instrument(skip(self, handler))]
    pub async fn listen_with_handler<H>(&mut self, handler: H) -> Result<()>
    where
        H: PgmqEventHandler + 'static,
    {
        if self.listener.is_none() {
            return Err(PgmqNotifyError::NotConnected);
        }

        let handler = Arc::new(handler);

        info!("Starting PGMQ notification listener loop");

        if let Some(listener) = self.listener.take() {
            let stats = Arc::clone(&self.stats);
            let _listening_channels = Arc::clone(&self.listening_channels);

            // Spawn the listening task
            tokio::spawn(async move {
                let mut stream = listener.into_stream();

                while let Some(notification) = stream.next().await {
                    match notification {
                        Ok(notification) => {
                            debug!(
                                "Received notification from channel: {} with payload: {}",
                                notification.channel(),
                                notification.payload()
                            );

                            // Update stats
                            {
                                let mut stats = stats.write().unwrap();
                                stats.events_received += 1;
                                stats.last_event_at = Some(SystemTime::now());
                            }

                            // Parse the event
                            match serde_json::from_str::<PgmqNotifyEvent>(notification.payload()) {
                                Ok(event) => {
                                    if let Err(e) = handler.handle_event(event).await {
                                        error!("Event handler failed: {}", e);
                                    }
                                }
                                Err(e) => {
                                    let parse_error = PgmqNotifyError::Serialization(e);

                                    // Update stats
                                    {
                                        let mut stats = stats.write().unwrap();
                                        stats.parse_errors += 1;
                                        stats.last_error_at = Some(SystemTime::now());
                                    }

                                    handler
                                        .handle_parse_error(
                                            notification.channel(),
                                            notification.payload(),
                                            parse_error,
                                        )
                                        .await;
                                }
                            }
                        }
                        Err(e) => {
                            let conn_error = PgmqNotifyError::Database(e);

                            // Update stats
                            {
                                let mut stats = stats.write().unwrap();
                                stats.connection_errors += 1;
                                stats.last_error_at = Some(SystemTime::now());
                                stats.connected = false;
                            }

                            handler.handle_connection_error(conn_error).await;

                            // Break the loop on connection error
                            break;
                        }
                    }
                }

                info!("PGMQ notification listener loop ended");
            });
        }

        Ok(())
    }

    /// Start a simple listening loop that queues events
    pub async fn start_listening(&mut self) -> Result<()> {
        if self.listener.is_none() {
            return Err(PgmqNotifyError::NotConnected);
        }

        let event_sender = self.event_sender.take();
        if let (Some(listener), Some(sender)) = (self.listener.take(), event_sender) {
            let stats = Arc::clone(&self.stats);

            info!("Starting PGMQ notification listener with event queue");

            tokio::spawn(async move {
                let mut stream = listener.into_stream();

                while let Some(notification) = stream.next().await {
                    match notification {
                        Ok(notification) => {
                            debug!(
                                "Received notification from channel: {} with payload: {}",
                                notification.channel(),
                                notification.payload()
                            );

                            // Update stats
                            {
                                let mut stats = stats.write().unwrap();
                                stats.events_received += 1;
                                stats.last_event_at = Some(SystemTime::now());
                            }

                            // Parse and queue the event
                            match serde_json::from_str::<PgmqNotifyEvent>(notification.payload()) {
                                Ok(event) => {
                                    if sender.send(event).is_err() {
                                        warn!("Event receiver dropped, stopping listener");
                                        break;
                                    }
                                }
                                Err(e) => {
                                    // Update stats
                                    {
                                        let mut stats = stats.write().unwrap();
                                        stats.parse_errors += 1;
                                        stats.last_error_at = Some(SystemTime::now());
                                    }

                                    warn!(
                                        "Failed to parse notification from channel {}: {} - payload: {}",
                                        notification.channel(),
                                        e,
                                        notification.payload()
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            // Update stats
                            {
                                let mut stats = stats.write().unwrap();
                                stats.connection_errors += 1;
                                stats.last_error_at = Some(SystemTime::now());
                                stats.connected = false;
                            }

                            error!("Connection error in listener: {}", e);
                            break;
                        }
                    }
                }

                info!("PGMQ notification listener stopped");
            });
        }

        Ok(())
    }

    /// Check if the listener is healthy
    pub async fn is_healthy(&self) -> bool {
        let stats = self.stats.read().unwrap();
        stats.connected
    }

    /// Get list of channels currently being listened to
    pub fn listening_channels(&self) -> Vec<String> {
        self.listening_channels
            .read()
            .unwrap()
            .iter()
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PgmqNotifyConfig;
    use crate::events::QueueCreatedEvent;

    // Mock event handler for testing
    struct MockEventHandler {
        events_received: Arc<RwLock<Vec<PgmqNotifyEvent>>>,
    }

    impl MockEventHandler {
        fn new() -> Self {
            Self {
                events_received: Arc::new(RwLock::new(Vec::new())),
            }
        }

        fn received_events(&self) -> Vec<PgmqNotifyEvent> {
            self.events_received.read().unwrap().clone()
        }
    }

    #[async_trait]
    impl PgmqEventHandler for MockEventHandler {
        async fn handle_event(&self, event: PgmqNotifyEvent) -> Result<()> {
            self.events_received.write().unwrap().push(event);
            Ok(())
        }
    }

    #[test]
    fn test_listener_stats() {
        let stats = ListenerStats::default();
        assert!(!stats.connected);
        assert_eq!(stats.channels_listening, 0);
        assert_eq!(stats.events_received, 0);
    }

    #[test]
    fn test_channel_management() {
        let config = PgmqNotifyConfig::default();

        assert_eq!(config.queue_created_channel(), "pgmq_queue_created");
        assert_eq!(
            config.message_ready_channel("orders"),
            "pgmq_message_ready.orders"
        );
        assert_eq!(config.global_message_ready_channel(), "pgmq_message_ready");
    }

    #[test]
    fn test_mock_event_handler() {
        let handler = MockEventHandler::new();
        let _event = PgmqNotifyEvent::QueueCreated(QueueCreatedEvent::new("test_queue", "test"));

        // Test handler setup
        assert_eq!(handler.received_events().len(), 0);
    }

    // Note: Full integration tests require a PostgreSQL connection
    // and would be better placed in an integration test module
}
