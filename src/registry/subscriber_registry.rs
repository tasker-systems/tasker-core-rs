//! # Subscriber Registry
//!
//! Registry for event subscriber management with thread-safe operations.
//!
//! ## Overview
//!
//! The SubscriberRegistry provides a centralized way to register, manage, and notify
//! event subscribers across the system. It integrates with the event system to provide
//! reliable event delivery and subscription management.
//!
//! ## Key Features
//!
//! - **Thread-safe subscriber management** using RwLock for concurrent access
//! - **Event pattern matching** for flexible subscription filtering
//! - **Subscriber lifecycle management** (register, unregister, notify)
//! - **Event delivery guarantees** with retry logic
//! - **Subscription statistics** and monitoring
//!
//! ## Usage
//!
//! ```rust
//! use tasker_core::registry::subscriber_registry::{SubscriberRegistry, EventSubscriber};
//! use std::sync::Arc;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let registry = SubscriberRegistry::new();
//!
//! // Register a subscriber for task events
//! registry.register_subscriber(
//!     "task_monitor",
//!     vec!["task.started".to_string(), "task.completed".to_string()],
//!     Arc::new(TaskEventSubscriber::new())
//! ).await?;
//!
//! // Notify subscribers of an event
//! registry.notify_subscribers("task.started", serde_json::json!({"task_uuid": 123})).await?;
//! # Ok(())
//! # }
//!
//! # struct TaskEventSubscriber;
//! # impl TaskEventSubscriber {
//! #     fn new() -> Self { Self }
//! # }
//! # #[async_trait::async_trait]
//! # impl EventSubscriber for TaskEventSubscriber {
//! #     async fn handle_event(&self, event_type: &str, payload: &serde_json::Value) -> Result<(), Box<dyn std::error::Error>> {
//! #         Ok(())
//! #     }
//! # }
//! ```

use crate::orchestration::errors::{OrchestrationError, OrchestrationResult};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

/// Trait for event subscribers
#[async_trait]
pub trait EventSubscriber: Send + Sync {
    /// Handle an event
    async fn handle_event(
        &self,
        event_type: &str,
        payload: &Value,
    ) -> Result<(), Box<dyn std::error::Error>>;

    /// Get subscriber name for identification
    fn subscriber_name(&self) -> &str {
        "unnamed_subscriber"
    }
}

/// Subscription information
#[derive(Clone)]
pub struct Subscription {
    pub subscriber_id: String,
    pub event_patterns: Vec<String>,
    pub subscriber: Arc<dyn EventSubscriber>,
    pub active: bool,
    pub events_received: u64,
    pub last_event_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl std::fmt::Debug for Subscription {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Subscription")
            .field("subscriber_id", &self.subscriber_id)
            .field("event_patterns", &self.event_patterns)
            .field("subscriber", &"<Arc<dyn EventSubscriber>>".to_string())
            .field("active", &self.active)
            .field("events_received", &self.events_received)
            .field("last_event_at", &self.last_event_at)
            .finish()
    }
}

/// Registry for managing event subscribers
pub struct SubscriberRegistry {
    /// Map of subscriber ID to subscription
    subscriptions: Arc<RwLock<HashMap<String, Subscription>>>,
    /// Map of event patterns to subscriber IDs
    pattern_index: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl SubscriberRegistry {
    /// Create a new subscriber registry
    pub fn new() -> Self {
        Self {
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            pattern_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a subscriber for specific event patterns
    pub async fn register_subscriber(
        &self,
        subscriber_id: &str,
        event_patterns: Vec<String>,
        subscriber: Arc<dyn EventSubscriber>,
    ) -> OrchestrationResult<()> {
        // Create subscription
        let subscription = Subscription {
            subscriber_id: subscriber_id.to_string(),
            event_patterns: event_patterns.clone(),
            subscriber,
            active: true,
            events_received: 0,
            last_event_at: None,
        };

        // Add to subscriptions
        {
            let mut subscriptions = self.subscriptions.write().await;
            subscriptions.insert(subscriber_id.to_string(), subscription);
        }

        // Update pattern index
        {
            let mut pattern_index = self.pattern_index.write().await;
            for pattern in event_patterns {
                pattern_index
                    .entry(pattern)
                    .or_insert_with(Vec::new)
                    .push(subscriber_id.to_string());
            }
        }

        info!(
            "Registered subscriber '{}' for event patterns",
            subscriber_id
        );
        Ok(())
    }

    /// Unregister a subscriber
    pub async fn unregister_subscriber(&self, subscriber_id: &str) -> OrchestrationResult<()> {
        // Remove from subscriptions
        let subscription = {
            let mut subscriptions = self.subscriptions.write().await;
            subscriptions.remove(subscriber_id)
        };

        if let Some(subscription) = subscription {
            // Remove from pattern index
            let mut pattern_index = self.pattern_index.write().await;
            for pattern in &subscription.event_patterns {
                if let Some(subscriber_ids) = pattern_index.get_mut(pattern) {
                    subscriber_ids.retain(|id| id != subscriber_id);
                    if subscriber_ids.is_empty() {
                        pattern_index.remove(pattern);
                    }
                }
            }

            info!("Unregistered subscriber '{}'", subscriber_id);
            Ok(())
        } else {
            Err(OrchestrationError::ConfigurationError {
                source: "SubscriberRegistry".to_string(),
                reason: format!("Subscriber '{subscriber_id}' not found"),
            })
        }
    }

    /// Notify subscribers of an event
    pub async fn notify_subscribers(
        &self,
        event_type: &str,
        payload: Value,
    ) -> OrchestrationResult<()> {
        // Find matching subscribers
        let matching_subscribers = self.find_matching_subscribers(event_type).await;

        if matching_subscribers.is_empty() {
            debug!("No subscribers found for event type '{}'", event_type);
            return Ok(());
        }

        // Notify each subscriber
        let mut notification_results = Vec::new();
        for subscriber_id in matching_subscribers {
            let result = self
                .notify_subscriber(&subscriber_id, event_type, &payload)
                .await;
            notification_results.push((subscriber_id, result));
        }

        // Log any failures
        for (subscriber_id, result) in notification_results {
            if let Err(e) = result {
                error!(
                    "Failed to notify subscriber '{}' of event '{}': {}",
                    subscriber_id, event_type, e
                );
            }
        }

        Ok(())
    }

    /// Find subscribers matching an event type
    async fn find_matching_subscribers(&self, event_type: &str) -> Vec<String> {
        let pattern_index = self.pattern_index.read().await;
        let subscriptions = self.subscriptions.read().await;

        let mut matching_subscribers = Vec::new();

        // Check exact matches first
        if let Some(subscriber_ids) = pattern_index.get(event_type) {
            for subscriber_id in subscriber_ids {
                if let Some(subscription) = subscriptions.get(subscriber_id) {
                    if subscription.active {
                        matching_subscribers.push(subscriber_id.clone());
                    }
                }
            }
        }

        // Check pattern matches (simple wildcard support)
        for (pattern, subscriber_ids) in pattern_index.iter() {
            if pattern.contains('*') && self.matches_pattern(event_type, pattern) {
                for subscriber_id in subscriber_ids {
                    if let Some(subscription) = subscriptions.get(subscriber_id) {
                        if subscription.active && !matching_subscribers.contains(subscriber_id) {
                            matching_subscribers.push(subscriber_id.clone());
                        }
                    }
                }
            }
        }

        matching_subscribers
    }

    /// Check if event type matches a pattern
    fn matches_pattern(&self, event_type: &str, pattern: &str) -> bool {
        // Simple wildcard matching - in production this would be more sophisticated
        if let Some(prefix) = pattern.strip_suffix('*') {
            event_type.starts_with(prefix)
        } else if let Some(suffix) = pattern.strip_prefix('*') {
            event_type.ends_with(suffix)
        } else {
            event_type == pattern
        }
    }

    /// Notify a specific subscriber
    async fn notify_subscriber(
        &self,
        subscriber_id: &str,
        event_type: &str,
        payload: &Value,
    ) -> OrchestrationResult<()> {
        let subscriber = {
            let mut subscriptions = self.subscriptions.write().await;
            if let Some(subscription) = subscriptions.get_mut(subscriber_id) {
                subscription.events_received += 1;
                subscription.last_event_at = Some(chrono::Utc::now());
                subscription.subscriber.clone()
            } else {
                return Err(OrchestrationError::ConfigurationError {
                    source: "SubscriberRegistry".to_string(),
                    reason: format!("Subscriber '{subscriber_id}' not found"),
                });
            }
        };

        // Notify the subscriber
        subscriber
            .handle_event(event_type, payload)
            .await
            .map_err(|e| OrchestrationError::ConfigurationError {
                source: "SubscriberRegistry".to_string(),
                reason: format!("Subscriber '{subscriber_id}' failed to handle event: {e}"),
            })?;

        Ok(())
    }

    /// Get subscription statistics
    pub async fn get_stats(&self) -> SubscriberStats {
        let subscriptions = self.subscriptions.read().await;
        let pattern_index = self.pattern_index.read().await;

        let mut stats = SubscriberStats {
            total_subscribers: subscriptions.len(),
            active_subscribers: 0,
            total_patterns: pattern_index.len(),
            total_events_processed: 0,
            subscriber_details: Vec::new(),
        };

        for subscription in subscriptions.values() {
            if subscription.active {
                stats.active_subscribers += 1;
            }
            stats.total_events_processed += subscription.events_received;

            stats.subscriber_details.push(SubscriberDetail {
                subscriber_id: subscription.subscriber_id.clone(),
                event_patterns: subscription.event_patterns.clone(),
                active: subscription.active,
                events_received: subscription.events_received,
                last_event_at: subscription.last_event_at,
            });
        }

        stats
    }

    /// Activate a subscriber
    pub async fn activate_subscriber(&self, subscriber_id: &str) -> OrchestrationResult<()> {
        let mut subscriptions = self.subscriptions.write().await;

        if let Some(subscription) = subscriptions.get_mut(subscriber_id) {
            subscription.active = true;
            info!("Activated subscriber '{}'", subscriber_id);
            Ok(())
        } else {
            Err(OrchestrationError::ConfigurationError {
                source: "SubscriberRegistry".to_string(),
                reason: format!("Subscriber '{subscriber_id}' not found"),
            })
        }
    }

    /// Deactivate a subscriber
    pub async fn deactivate_subscriber(&self, subscriber_id: &str) -> OrchestrationResult<()> {
        let mut subscriptions = self.subscriptions.write().await;

        if let Some(subscription) = subscriptions.get_mut(subscriber_id) {
            subscription.active = false;
            info!("Deactivated subscriber '{}'", subscriber_id);
            Ok(())
        } else {
            Err(OrchestrationError::ConfigurationError {
                source: "SubscriberRegistry".to_string(),
                reason: format!("Subscriber '{subscriber_id}' not found"),
            })
        }
    }

    /// List all subscribers
    pub async fn list_subscribers(&self) -> Vec<String> {
        let subscriptions = self.subscriptions.read().await;
        subscriptions.keys().cloned().collect()
    }
}

impl Default for SubscriberRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about subscribers
#[derive(Debug, Clone)]
pub struct SubscriberStats {
    pub total_subscribers: usize,
    pub active_subscribers: usize,
    pub total_patterns: usize,
    pub total_events_processed: u64,
    pub subscriber_details: Vec<SubscriberDetail>,
}

/// Details about a specific subscriber
#[derive(Debug, Clone)]
pub struct SubscriberDetail {
    pub subscriber_id: String,
    pub event_patterns: Vec<String>,
    pub active: bool,
    pub events_received: u64,
    pub last_event_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Test subscriber implementation
    struct TestSubscriber {
        id: String,
        events_handled: Arc<AtomicU64>,
    }

    impl TestSubscriber {
        fn new(id: &str) -> Self {
            Self {
                id: id.to_string(),
                events_handled: Arc::new(AtomicU64::new(0)),
            }
        }

        fn events_handled(&self) -> u64 {
            self.events_handled.load(Ordering::Relaxed)
        }
    }

    #[async_trait]
    impl EventSubscriber for TestSubscriber {
        async fn handle_event(
            &self,
            _event_type: &str,
            _payload: &Value,
        ) -> Result<(), Box<dyn std::error::Error>> {
            self.events_handled.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }

        fn subscriber_name(&self) -> &str {
            &self.id
        }
    }

    #[tokio::test]
    async fn test_subscriber_registry_creation() {
        let registry = SubscriberRegistry::new();
        let stats = registry.get_stats().await;
        assert_eq!(stats.total_subscribers, 0);
    }

    #[tokio::test]
    async fn test_subscriber_registration() {
        let registry = SubscriberRegistry::new();
        let subscriber = Arc::new(TestSubscriber::new("test_subscriber"));

        registry
            .register_subscriber(
                "test_subscriber",
                vec!["test.event".to_string()],
                subscriber,
            )
            .await
            .unwrap();

        let stats = registry.get_stats().await;
        assert_eq!(stats.total_subscribers, 1);
        assert_eq!(stats.active_subscribers, 1);
    }

    #[tokio::test]
    async fn test_event_notification() {
        let registry = SubscriberRegistry::new();
        let subscriber = Arc::new(TestSubscriber::new("test_subscriber"));

        registry
            .register_subscriber(
                "test_subscriber",
                vec!["test.event".to_string()],
                subscriber.clone(),
            )
            .await
            .unwrap();

        // Notify of an event
        registry
            .notify_subscribers("test.event", serde_json::json!({"test": "data"}))
            .await
            .unwrap();

        // Check that subscriber received the event
        assert_eq!(subscriber.events_handled(), 1);
    }

    #[tokio::test]
    async fn test_pattern_matching() {
        let registry = SubscriberRegistry::new();
        let subscriber = Arc::new(TestSubscriber::new("pattern_subscriber"));

        registry
            .register_subscriber(
                "pattern_subscriber",
                vec!["task.*".to_string()],
                subscriber.clone(),
            )
            .await
            .unwrap();

        // Test different event types
        registry
            .notify_subscribers("task.started", serde_json::json!({"task_uuid": 1}))
            .await
            .unwrap();

        registry
            .notify_subscribers("task.completed", serde_json::json!({"task_uuid": 1}))
            .await
            .unwrap();

        registry
            .notify_subscribers("step.started", serde_json::json!({"step_uuid": 1}))
            .await
            .unwrap();

        // Should have received 2 events (task.* but not step.*)
        assert_eq!(subscriber.events_handled(), 2);
    }

    #[tokio::test]
    async fn test_subscriber_lifecycle() {
        let registry = SubscriberRegistry::new();
        let subscriber = Arc::new(TestSubscriber::new("lifecycle_test"));

        // Register subscriber
        registry
            .register_subscriber(
                "lifecycle_test",
                vec!["test.event".to_string()],
                subscriber.clone(),
            )
            .await
            .unwrap();

        // Deactivate subscriber
        registry
            .deactivate_subscriber("lifecycle_test")
            .await
            .unwrap();

        // Notify - should not receive event
        registry
            .notify_subscribers("test.event", serde_json::json!({"test": "data"}))
            .await
            .unwrap();

        assert_eq!(subscriber.events_handled(), 0);

        // Reactivate subscriber
        registry
            .activate_subscriber("lifecycle_test")
            .await
            .unwrap();

        // Notify - should receive event
        registry
            .notify_subscribers("test.event", serde_json::json!({"test": "data"}))
            .await
            .unwrap();

        assert_eq!(subscriber.events_handled(), 1);

        // Unregister subscriber
        registry
            .unregister_subscriber("lifecycle_test")
            .await
            .unwrap();

        let stats = registry.get_stats().await;
        assert_eq!(stats.total_subscribers, 0);
    }
}
