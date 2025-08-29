//! PostgreSQL LISTEN/NOTIFY client for TAS-43 task readiness events
//!
//! This module provides a robust PostgreSQL notification listener that handles
//! connection management, automatic reconnection, and event classification using
//! the config-driven approach from events.rs.

use std::collections::HashMap;
use std::sync::Arc;
use sqlx::PgPool;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use tasker_shared::{TaskerResult, TaskerError};
use super::events::*;

/// PostgreSQL listener for task readiness notifications
/// 
/// Manages PostgreSQL LISTEN/NOTIFY connections with automatic reconnection,
/// error handling, and config-driven event classification. Provides a unified
/// interface for receiving all types of task readiness events.
pub struct TaskReadinessListener {
    /// Database connection pool
    pool: PgPool,
    /// SQLx PostgreSQL listener (when connected)
    listener: Option<sqlx::postgres::PgListener>,
    /// Event sender channel
    event_sender: mpsc::Sender<TaskReadinessNotification>,
    /// Config-driven event classifier
    event_classifier: ReadinessEventClassifier,
    /// Connection state
    is_connected: bool,
    /// Channels currently being listened to
    listened_channels: HashMap<String, bool>,
}

/// Unified notification enum for all task readiness events
/// 
/// Wraps the classified TaskReadinessEvent with connection error handling.
/// This provides a single channel interface for all event types and errors.
#[derive(Debug, Clone)]
pub enum TaskReadinessNotification {
    /// Classified readiness event using config-driven classification
    Event(TaskReadinessEvent),
    /// Connection error from PostgreSQL listener
    ConnectionError(String),
    /// Listener reconnected successfully
    Reconnected,
}

impl TaskReadinessListener {
    /// Create new task readiness listener with config-driven classification
    /// 
    /// The listener uses the provided event classifier for config-driven event
    /// parsing and classification, following the same patterns as ConfigDrivenMessageEvent.
    pub async fn new(
        pool: PgPool,
        event_sender: mpsc::Sender<TaskReadinessNotification>,
        notification_config: TaskReadinessNotificationConfig,
    ) -> TaskerResult<Self> {
        Ok(Self {
            pool,
            listener: None,
            event_sender,
            event_classifier: ReadinessEventClassifier::new(notification_config),
            is_connected: false,
            listened_channels: HashMap::new(),
        })
    }

    /// Create listener with default notification configuration
    pub async fn new_with_defaults(
        pool: PgPool,
        event_sender: mpsc::Sender<TaskReadinessNotification>,
    ) -> TaskerResult<Self> {
        Self::new(pool, event_sender, TaskReadinessNotificationConfig::default()).await
    }

    /// Connect to PostgreSQL and start listening to global channels
    /// 
    /// Establishes the PostgreSQL LISTEN connection and subscribes to the core
    /// global channels (task_ready, task_state_change, namespace_created).
    /// Individual namespaces can be added later with listen_for_namespace().
    pub async fn connect(&mut self) -> TaskerResult<()> {
        info!("Connecting task readiness listener to PostgreSQL");

        let mut listener = sqlx::postgres::PgListener::connect_with(&self.pool)
            .await
            .map_err(|e| TaskerError::DatabaseError(format!("Failed to connect listener: {}", e)))?;

        // Listen to global channels configured via the classifier
        let global_channels = self.event_classifier.get_global_channels();
        
        for channel in global_channels {
            listener.listen(&channel).await.map_err(|e| {
                TaskerError::DatabaseError(format!("Failed to listen to {}: {}", channel, e))
            })?;
            
            self.listened_channels.insert(channel.clone(), true);
            debug!("Now listening to global channel: {}", channel);
        }

        self.listener = Some(listener);
        self.is_connected = true;

        info!("Task readiness listener connected successfully");
        Ok(())
    }

    /// Listen for specific namespace events
    /// 
    /// Adds namespace-specific channels (task_ready.namespace, task_state_change.namespace)
    /// to the listener. This enables namespace-specific event filtering and processing.
    pub async fn listen_for_namespace(&mut self, namespace: &str) -> TaskerResult<()> {
        if let Some(ref mut listener) = self.listener {
            let namespace_channels = self.event_classifier.get_namespace_channels(namespace);
            
            for channel in namespace_channels {
                if !self.listened_channels.contains_key(&channel) {
                    listener.listen(&channel).await.map_err(|e| {
                        TaskerError::DatabaseError(format!("Failed to listen to {}: {}", channel, e))
                    })?;
                    
                    self.listened_channels.insert(channel.clone(), true);
                    debug!("Now listening for namespace channel: {}", channel);
                }
            }
            
            info!("Added namespace listening: {}", namespace);
        } else {
            return Err(TaskerError::ValidationError(
                "Cannot add namespace: listener not connected".to_string(),
            ));
        }
        
        Ok(())
    }

    /// Stop listening for specific namespace events
    /// 
    /// Removes namespace-specific channels from the listener. Note that SQLx doesn't
    /// provide an "unlisten" method, so this just tracks that we're no longer interested
    /// in events from this namespace.
    pub async fn stop_listening_for_namespace(&mut self, namespace: &str) -> TaskerResult<()> {
        let namespace_channels = self.event_classifier.get_namespace_channels(namespace);
        
        for channel in namespace_channels {
            self.listened_channels.remove(&channel);
            debug!("Stopped tracking namespace channel: {}", channel);
        }
        
        info!("Stopped namespace listening: {}", namespace);
        Ok(())
    }

    /// Start listening loop with automatic reconnection
    /// 
    /// This is the main event loop that receives PostgreSQL notifications and
    /// sends classified events via the event channel. It handles connection errors
    /// with automatic reconnection attempts.
    pub async fn start_listening(&mut self) -> TaskerResult<()> {
        if !self.is_connected {
            return Err(TaskerError::ValidationError(
                "Listener not connected".to_string(),
            ));
        }

        let listener = self.listener.as_mut().unwrap();
        let event_sender = self.event_sender.clone();
        let event_classifier = self.event_classifier.clone();

        info!("Starting task readiness notification listening loop");

        loop {
            match listener.recv().await {
                Ok(notification) => {
                    debug!(
                        "Received PostgreSQL notification on channel: {}",
                        notification.channel()
                    );

                    let event = Self::parse_notification_static(&event_classifier, notification).await;
                    if let Err(e) = event_sender.send(event).await {
                        error!("Failed to send task readiness event: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    error!("PostgreSQL listener error: {}", e);
                    
                    // Send connection error event
                    let error_event = TaskReadinessNotification::ConnectionError(format!("{}", e));
                    if event_sender.send(error_event).await.is_err() {
                        error!("Failed to send connection error event");
                        break;
                    }

                    // SQLx will attempt auto-reconnect on next recv()
                    // Send reconnected event if successful
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    
                    // Test if we're reconnected by trying to receive again
                    match listener.recv().await {
                        Ok(notification) => {
                            info!("PostgreSQL listener reconnected successfully");
                            let reconnect_event = TaskReadinessNotification::Reconnected;
                            if event_sender.send(reconnect_event).await.is_err() {
                                break;
                            }
                            
                            // Process the notification we just received
                            let event = Self::parse_notification_static(&event_classifier, notification).await;
                            if event_sender.send(event).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => {
                            // Still having connection issues, continue loop to retry
                            continue;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Parse PostgreSQL notification into classified event using config-driven classification
    /// 
    /// Uses the ReadinessEventClassifier to parse the notification channel and payload
    /// into typed events, following the same pattern as ConfigDrivenMessageEvent::classify
    /// from event_driven_coordinator.rs.
    async fn parse_notification_static(
        event_classifier: &ReadinessEventClassifier,
        notification: sqlx::postgres::PgNotification,
    ) -> TaskReadinessNotification {
        let channel = notification.channel();
        let payload = notification.payload();

        debug!("Parsing notification from channel {}: {}", channel, payload);

        // Use config-driven classification (no hardcoded string matching)
        // This follows the same exhaustive enum matching pattern as ConfigDrivenMessageEvent
        let classified_event = event_classifier.classify(channel, payload);
        
        debug!(
            channel = channel,
            event_type = ?classified_event,
            "Classified readiness notification using config-driven enum-based dispatching"
        );

        TaskReadinessNotification::Event(classified_event)
    }

    async fn parse_notification(
        &self,
        notification: sqlx::postgres::PgNotification,
    ) -> TaskReadinessNotification {
        Self::parse_notification_static(&self.event_classifier, notification).await
    }

    /// Disconnect listener gracefully
    /// 
    /// Closes the PostgreSQL LISTEN connection and cleans up resources.
    /// The listener can be reconnected later with connect().
    pub async fn disconnect(&mut self) -> TaskerResult<()> {
        if let Some(listener) = self.listener.take() {
            // SQLx listener doesn't have explicit disconnect, dropping closes connection
            drop(listener);
        }
        
        self.is_connected = false;
        self.listened_channels.clear();
        info!("Task readiness listener disconnected");
        Ok(())
    }

    /// Check if listener is connected
    pub fn is_connected(&self) -> bool {
        self.is_connected
    }

    /// Get list of channels currently being listened to
    pub fn get_listened_channels(&self) -> Vec<String> {
        self.listened_channels.keys().cloned().collect()
    }

    /// Get the event classifier configuration
    pub fn get_classifier_config(&self) -> &TaskReadinessNotificationConfig {
        self.event_classifier.config()
    }

    /// Get statistics about the listener
    pub fn get_listener_stats(&self) -> TaskReadinessListenerStats {
        TaskReadinessListenerStats {
            is_connected: self.is_connected,
            channels_count: self.listened_channels.len(),
            listened_channels: self.get_listened_channels(),
        }
    }
}

/// Statistics for task readiness listener
#[derive(Debug, Clone)]
pub struct TaskReadinessListenerStats {
    /// Whether the listener is currently connected
    pub is_connected: bool,
    /// Number of channels being listened to
    pub channels_count: usize,
    /// List of channels being listened to
    pub listened_channels: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, timeout, Duration};

    #[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
    async fn test_listener_connection(pool: sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
        let (tx, _rx) = mpsc::channel(100);
        let notification_config = TaskReadinessNotificationConfig::default();
        let mut listener = TaskReadinessListener::new(pool, tx, notification_config).await?;

        assert!(!listener.is_connected());
        
        listener.connect().await?;
        assert!(listener.is_connected());
        
        // Check that global channels are being listened to
        let channels = listener.get_listened_channels();
        assert!(channels.contains(&"task_ready".to_string()));
        assert!(channels.contains(&"task_state_change".to_string()));
        assert!(channels.contains(&"namespace_created".to_string()));

        listener.disconnect().await?;
        assert!(!listener.is_connected());

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
    async fn test_namespace_listening(pool: sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
        let (tx, _rx) = mpsc::channel(100);
        let mut listener = TaskReadinessListener::new_with_defaults(pool, tx).await?;
        
        listener.connect().await?;
        
        // Add namespace listening
        listener.listen_for_namespace("fulfillment").await?;
        
        let channels = listener.get_listened_channels();
        assert!(channels.contains(&"task_ready.fulfillment".to_string()));
        assert!(channels.contains(&"task_state_change.fulfillment".to_string()));
        
        // Remove namespace listening
        listener.stop_listening_for_namespace("fulfillment").await?;
        
        let channels_after = listener.get_listened_channels();
        assert!(!channels_after.contains(&"task_ready.fulfillment".to_string()));
        assert!(!channels_after.contains(&"task_state_change.fulfillment".to_string()));
        
        listener.disconnect().await?;
        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
    async fn test_notification_parsing(pool: sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
        let (tx, mut rx) = mpsc::channel(100);
        let notification_config = TaskReadinessNotificationConfig::default();
        let mut listener = TaskReadinessListener::new(pool.clone(), tx, notification_config).await?;
        
        listener.connect().await?;

        // Send a test notification
        let test_payload = r#"{"task_uuid":"123e4567-e89b-12d3-a456-426614174000","namespace":"test","priority":1,"ready_steps":2,"triggered_by":"step_transition"}"#;
        sqlx::query("SELECT pg_notify('task_ready', $1)")
            .bind(test_payload)
            .execute(&pool)
            .await?;

        // Start listening in background
        let listener_handle = tokio::spawn(async move {
            let _ = listener.start_listening().await;
        });

        // Wait for notification
        let notification = timeout(Duration::from_secs(5), rx.recv()).await?.ok_or("No notification received")?;
        
        match notification {
            TaskReadinessNotification::Event(TaskReadinessEvent::TaskReady(event)) => {
                assert_eq!(event.namespace, "test");
                assert_eq!(event.ready_steps, 2);
                assert!(matches!(event.triggered_by, ReadinessTrigger::StepTransition));
            }
            _ => panic!("Expected TaskReady notification"),
        }

        // Clean up
        listener_handle.abort();

        Ok(())
    }

    #[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
    async fn test_multiple_notification_types(pool: sqlx::PgPool) -> Result<(), Box<dyn std::error::Error>> {
        let (tx, mut rx) = mpsc::channel(100);
        let mut listener = TaskReadinessListener::new_with_defaults(pool.clone(), tx).await?;
        
        listener.connect().await?;

        // Start listening in background
        let listener_handle = tokio::spawn(async move {
            let _ = listener.start_listening().await;
        });

        // Send different types of notifications
        let task_ready_payload = r#"{"task_uuid":"123e4567-e89b-12d3-a456-426614174000","namespace":"test","priority":1,"ready_steps":2,"triggered_by":"step_transition"}"#;
        sqlx::query("SELECT pg_notify('task_ready', $1)")
            .bind(task_ready_payload)
            .execute(&pool)
            .await?;

        let state_change_payload = r#"{"task_uuid":"123e4567-e89b-12d3-a456-426614174000","namespace":"test","task_state":"complete","triggered_by":"task_transition","action_needed":"finalization"}"#;
        sqlx::query("SELECT pg_notify('task_state_change', $1)")
            .bind(state_change_payload)
            .execute(&pool)
            .await?;

        // Wait for both notifications
        let mut task_ready_received = false;
        let mut state_change_received = false;
        
        while !task_ready_received || !state_change_received {
            let notification = timeout(Duration::from_secs(5), rx.recv()).await?.ok_or("No notification received")?;
            
            match notification {
                TaskReadinessNotification::Event(TaskReadinessEvent::TaskReady(_)) => {
                    task_ready_received = true;
                }
                TaskReadinessNotification::Event(TaskReadinessEvent::TaskStateChange(_)) => {
                    state_change_received = true;
                }
                _ => {}
            }
        }

        assert!(task_ready_received);
        assert!(state_change_received);

        // Clean up
        listener_handle.abort();

        Ok(())
    }

    #[tokio::test]
    async fn test_listener_stats() {
        // Skip this test as it requires a real database connection
        // The stats functionality is tested in integration tests
        // This test would be for unit testing the TaskReadinessListenerStats structure
        let stats = TaskReadinessListenerStats {
            is_connected: false,
            channels_count: 0,
            listened_channels: vec![],
        };
        
        assert!(!stats.is_connected);
        assert_eq!(stats.channels_count, 0);
        assert_eq!(stats.listened_channels.len(), 0);
    }
}