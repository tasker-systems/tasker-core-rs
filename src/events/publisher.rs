use serde_json::Value;
use tokio::sync::broadcast;

/// High-throughput event publisher for lifecycle events
#[derive(Debug, Clone)]
pub struct EventPublisher {
    sender: broadcast::Sender<PublishedEvent>,
}

/// Event that has been published
#[derive(Debug, Clone)]
pub struct PublishedEvent {
    pub name: String,
    pub context: Value,
    pub published_at: chrono::DateTime<chrono::Utc>,
}

impl EventPublisher {
    /// Create a new event publisher with the specified channel capacity
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Publish an event with the given name and context
    pub async fn publish(
        &self,
        event_name: impl Into<String>,
        context: Value,
    ) -> Result<(), PublishError> {
        let event = PublishedEvent {
            name: event_name.into(),
            context,
            published_at: chrono::Utc::now(),
        };

        // For broadcast channels, send() returns an error if there are no subscribers
        // In our case, this is acceptable - we want to publish events even if no one is listening
        match self.sender.send(event) {
            Ok(_) => Ok(()),
            Err(broadcast::error::SendError(_)) => {
                // No subscribers - this is acceptable for event publishing
                Ok(())
            }
        }
    }

    /// Subscribe to events
    pub fn subscribe(&self) -> broadcast::Receiver<PublishedEvent> {
        self.sender.subscribe()
    }

    /// Get the number of active subscribers
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

/// Error types for event publishing
#[derive(Debug, thiserror::Error)]
pub enum PublishError {
    #[error("Event channel is closed")]
    ChannelClosed,
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl Default for EventPublisher {
    fn default() -> Self {
        Self::new(1000) // Default capacity of 1000 events
    }
}
