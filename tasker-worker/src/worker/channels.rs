//! # TAS-133: Semantic NewType Channel Wrappers for Worker
//!
//! This module provides strongly-typed channel wrappers that prevent accidental misuse
//! of channels at compile time. Each channel type encodes its purpose in the type system.
//!
//! ## Benefits
//!
//! - **Type safety**: Can't pass `WorkerNotificationSender` where `WorkerCommandSender` is expected
//! - **Self-documenting**: Function signatures clearly show which channel type is needed
//! - **Zero runtime cost**: NewTypes compile away entirely
//! - **IDE support**: Autocomplete shows only valid operations
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::worker::channels::{ChannelFactory, WorkerCommandSender, WorkerNotificationReceiver};
//!
//! // Create channels using the factory
//! let (cmd_tx, cmd_rx) = ChannelFactory::worker_command_channel(1000);
//! let (notif_tx, notif_rx) = ChannelFactory::worker_notification_channel(500);
//!
//! // Use the typed senders/receivers
//! cmd_tx.send(WorkerCommand::ExecuteStep { ... }).await?;
//! ```

use tokio::sync::mpsc;

use super::command_processor::WorkerCommand;
use super::worker_queues::events::WorkerNotification;

// ============================================================================
// Worker Notification Channel Types
// ============================================================================

/// Strongly-typed sender for worker notifications.
///
/// Used by queue listeners to send notifications to the event system.
/// Wraps `mpsc::Sender<WorkerNotification>` with semantic meaning.
#[derive(Debug, Clone)]
pub struct WorkerNotificationSender(pub(crate) mpsc::Sender<WorkerNotification>);

/// Strongly-typed receiver for worker notifications.
///
/// Used by the event system to receive notifications from queue listeners.
/// Wraps `mpsc::Receiver<WorkerNotification>` with semantic meaning.
#[derive(Debug)]
pub struct WorkerNotificationReceiver(pub(crate) mpsc::Receiver<WorkerNotification>);

impl WorkerNotificationSender {
    /// Send a notification through the channel.
    pub async fn send(
        &self,
        notification: WorkerNotification,
    ) -> Result<(), mpsc::error::SendError<WorkerNotification>> {
        self.0.send(notification).await
    }

    /// Try to send a notification without waiting.
    pub fn try_send(
        &self,
        notification: WorkerNotification,
    ) -> Result<(), mpsc::error::TrySendError<WorkerNotification>> {
        self.0.try_send(notification)
    }

    /// Check if the channel is closed.
    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }

    /// Get the channel capacity.
    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }

    /// Get the maximum capacity of the channel.
    pub fn max_capacity(&self) -> usize {
        self.0.max_capacity()
    }
}

impl WorkerNotificationReceiver {
    /// Receive the next notification from the channel.
    pub async fn recv(&mut self) -> Option<WorkerNotification> {
        self.0.recv().await
    }

    /// Try to receive a notification without waiting.
    pub fn try_recv(&mut self) -> Result<WorkerNotification, mpsc::error::TryRecvError> {
        self.0.try_recv()
    }

    /// Close the receiver, preventing further sends.
    pub fn close(&mut self) {
        self.0.close()
    }
}

// ============================================================================
// Worker Command Channel Types
// ============================================================================

/// Strongly-typed sender for worker commands.
///
/// Used by the event system and fallback poller to send commands to the command processor.
/// Wraps `mpsc::Sender<WorkerCommand>` with semantic meaning.
#[derive(Debug, Clone)]
pub struct WorkerCommandSender(pub(crate) mpsc::Sender<WorkerCommand>);

/// Strongly-typed receiver for worker commands.
///
/// Used by the command processor to receive commands from event systems.
/// Wraps `mpsc::Receiver<WorkerCommand>` with semantic meaning.
#[derive(Debug)]
pub struct WorkerCommandReceiver(pub(crate) mpsc::Receiver<WorkerCommand>);

impl WorkerCommandSender {
    /// Send a command through the channel.
    pub async fn send(
        &self,
        command: WorkerCommand,
    ) -> Result<(), mpsc::error::SendError<WorkerCommand>> {
        self.0.send(command).await
    }

    /// Try to send a command without waiting.
    pub fn try_send(
        &self,
        command: WorkerCommand,
    ) -> Result<(), mpsc::error::TrySendError<WorkerCommand>> {
        self.0.try_send(command)
    }

    /// Check if the channel is closed.
    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }

    /// Get the channel capacity.
    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }

    /// Get the maximum capacity of the channel.
    pub fn max_capacity(&self) -> usize {
        self.0.max_capacity()
    }

    /// Get the inner sender for interop with code that needs the raw type.
    ///
    /// This is provided for gradual migration - prefer using the typed wrapper methods.
    pub fn inner(&self) -> &mpsc::Sender<WorkerCommand> {
        &self.0
    }
}

impl WorkerCommandReceiver {
    /// Receive the next command from the channel.
    pub async fn recv(&mut self) -> Option<WorkerCommand> {
        self.0.recv().await
    }

    /// Try to receive a command without waiting.
    pub fn try_recv(&mut self) -> Result<WorkerCommand, mpsc::error::TryRecvError> {
        self.0.try_recv()
    }

    /// Close the receiver, preventing further sends.
    pub fn close(&mut self) {
        self.0.close()
    }
}

// ============================================================================
// Channel Factory
// ============================================================================

/// Factory for creating strongly-typed channel pairs.
///
/// Provides consistent channel creation with semantic NewType wrappers.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChannelFactory;

impl ChannelFactory {
    /// Create a worker notification channel pair.
    ///
    /// Used for communication between queue listeners and the event system.
    pub fn worker_notification_channel(
        buffer_size: usize,
    ) -> (WorkerNotificationSender, WorkerNotificationReceiver) {
        let (tx, rx) = mpsc::channel(buffer_size);
        (WorkerNotificationSender(tx), WorkerNotificationReceiver(rx))
    }

    /// Create a worker command channel pair.
    ///
    /// Used for communication between event systems and the command processor.
    pub fn worker_command_channel(
        buffer_size: usize,
    ) -> (WorkerCommandSender, WorkerCommandReceiver) {
        let (tx, rx) = mpsc::channel(buffer_size);
        (WorkerCommandSender(tx), WorkerCommandReceiver(rx))
    }
}

// ============================================================================
// Conversion traits for gradual migration
// ============================================================================

impl From<mpsc::Sender<WorkerNotification>> for WorkerNotificationSender {
    fn from(sender: mpsc::Sender<WorkerNotification>) -> Self {
        WorkerNotificationSender(sender)
    }
}

impl From<mpsc::Sender<WorkerCommand>> for WorkerCommandSender {
    fn from(sender: mpsc::Sender<WorkerCommand>) -> Self {
        WorkerCommandSender(sender)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worker::worker_queues::events::{WorkerHealthStatus, WorkerHealthUpdate};

    #[tokio::test]
    async fn test_notification_channel_send_recv() {
        let (tx, mut rx) = ChannelFactory::worker_notification_channel(10);

        // Send a health notification
        let health_update = WorkerHealthUpdate {
            worker_id: "test-worker".to_string(),
            status: WorkerHealthStatus::Healthy,
            supported_namespaces: vec!["default".to_string()],
            last_activity: None,
            details: Some("test notification".to_string()),
        };
        tx.send(WorkerNotification::Health(health_update))
            .await
            .unwrap();

        // Receive it
        let notification = rx.recv().await.unwrap();
        assert!(matches!(notification, WorkerNotification::Health(_)));
    }

    #[tokio::test]
    async fn test_channel_capacity() {
        let (tx, _rx) = ChannelFactory::worker_notification_channel(100);
        assert_eq!(tx.max_capacity(), 100);
    }

    #[test]
    fn test_sender_clone() {
        let (tx, _rx) = ChannelFactory::worker_notification_channel(10);
        let _tx2 = tx.clone(); // Should compile - senders are clonable
    }
}
