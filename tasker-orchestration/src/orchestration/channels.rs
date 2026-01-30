//! # TAS-133: Semantic NewType Channel Wrappers for Orchestration
//!
//! This module provides strongly-typed channel wrappers that prevent accidental misuse
//! of channels at compile time. Each channel type encodes its purpose in the type system.
//!
//! ## Benefits
//!
//! - **Type safety**: Can't pass `OrchestrationNotificationSender` where `OrchestrationCommandSender` is expected
//! - **Self-documenting**: Function signatures clearly show which channel type is needed
//! - **Zero runtime cost**: NewTypes compile away entirely
//! - **IDE support**: Autocomplete shows only valid operations
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::orchestration::channels::{ChannelFactory, OrchestrationCommandSender};
//!
//! // Create channels using the factory
//! let (cmd_tx, cmd_rx) = ChannelFactory::orchestration_command_channel(5000);
//! let (notif_tx, notif_rx) = ChannelFactory::orchestration_notification_channel(10000);
//!
//! // Use the typed senders/receivers
//! cmd_tx.send(OrchestrationCommand::InitializeTask { ... }).await?;
//! ```

use tokio::sync::mpsc;

use super::commands::OrchestrationCommand;
use super::orchestration_queues::listener::OrchestrationNotification;

// ============================================================================
// Orchestration Notification Channel Types
// ============================================================================

/// Strongly-typed sender for orchestration notifications.
///
/// Used by queue listeners to send notifications to the event system.
/// Wraps `mpsc::Sender<OrchestrationNotification>` with semantic meaning.
#[derive(Debug, Clone)]
pub struct OrchestrationNotificationSender(pub(crate) mpsc::Sender<OrchestrationNotification>);

/// Strongly-typed receiver for orchestration notifications.
///
/// Used by the event system to receive notifications from queue listeners.
/// Wraps `mpsc::Receiver<OrchestrationNotification>` with semantic meaning.
#[derive(Debug)]
pub struct OrchestrationNotificationReceiver(pub(crate) mpsc::Receiver<OrchestrationNotification>);

impl OrchestrationNotificationSender {
    /// Send a notification through the channel.
    pub async fn send(
        &self,
        notification: OrchestrationNotification,
    ) -> Result<(), mpsc::error::SendError<OrchestrationNotification>> {
        self.0.send(notification).await
    }

    /// Try to send a notification without waiting.
    pub fn try_send(
        &self,
        notification: OrchestrationNotification,
    ) -> Result<(), mpsc::error::TrySendError<OrchestrationNotification>> {
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

impl OrchestrationNotificationReceiver {
    /// Receive the next notification from the channel.
    pub async fn recv(&mut self) -> Option<OrchestrationNotification> {
        self.0.recv().await
    }

    /// Try to receive a notification without waiting.
    pub fn try_recv(&mut self) -> Result<OrchestrationNotification, mpsc::error::TryRecvError> {
        self.0.try_recv()
    }

    /// Close the receiver, preventing further sends.
    pub fn close(&mut self) {
        self.0.close()
    }
}

// ============================================================================
// Orchestration Command Channel Types
// ============================================================================

/// Strongly-typed sender for orchestration commands.
///
/// Used by the event system and fallback poller to send commands to the command processor.
/// Wraps `mpsc::Sender<OrchestrationCommand>` with semantic meaning.
#[derive(Debug, Clone)]
pub struct OrchestrationCommandSender(pub(crate) mpsc::Sender<OrchestrationCommand>);

/// Strongly-typed receiver for orchestration commands.
///
/// Used by the command processor to receive commands from event systems.
/// Wraps `mpsc::Receiver<OrchestrationCommand>` with semantic meaning.
#[derive(Debug)]
pub struct OrchestrationCommandReceiver(pub(crate) mpsc::Receiver<OrchestrationCommand>);

impl OrchestrationCommandSender {
    /// Send a command through the channel.
    pub async fn send(
        &self,
        command: OrchestrationCommand,
    ) -> Result<(), mpsc::error::SendError<OrchestrationCommand>> {
        self.0.send(command).await
    }

    /// Try to send a command without waiting.
    #[expect(
        clippy::result_large_err,
        reason = "TrySendError includes the command for recovery; boxing would complicate error handling"
    )]
    pub fn try_send(
        &self,
        command: OrchestrationCommand,
    ) -> Result<(), mpsc::error::TrySendError<OrchestrationCommand>> {
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
    pub fn inner(&self) -> &mpsc::Sender<OrchestrationCommand> {
        &self.0
    }
}

impl OrchestrationCommandReceiver {
    /// Receive the next command from the channel.
    pub async fn recv(&mut self) -> Option<OrchestrationCommand> {
        self.0.recv().await
    }

    /// Try to receive a command without waiting.
    pub fn try_recv(&mut self) -> Result<OrchestrationCommand, mpsc::error::TryRecvError> {
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
    /// Create an orchestration notification channel pair.
    ///
    /// Used for communication between queue listeners and the event system.
    pub fn orchestration_notification_channel(
        buffer_size: usize,
    ) -> (
        OrchestrationNotificationSender,
        OrchestrationNotificationReceiver,
    ) {
        let (tx, rx) = mpsc::channel(buffer_size);
        (
            OrchestrationNotificationSender(tx),
            OrchestrationNotificationReceiver(rx),
        )
    }

    /// Create an orchestration command channel pair.
    ///
    /// Used for communication between event systems and the command processor.
    pub fn orchestration_command_channel(
        buffer_size: usize,
    ) -> (OrchestrationCommandSender, OrchestrationCommandReceiver) {
        let (tx, rx) = mpsc::channel(buffer_size);
        (
            OrchestrationCommandSender(tx),
            OrchestrationCommandReceiver(rx),
        )
    }
}

// ============================================================================
// Conversion traits for gradual migration
// ============================================================================

impl From<mpsc::Sender<OrchestrationNotification>> for OrchestrationNotificationSender {
    fn from(sender: mpsc::Sender<OrchestrationNotification>) -> Self {
        OrchestrationNotificationSender(sender)
    }
}

impl From<mpsc::Sender<OrchestrationCommand>> for OrchestrationCommandSender {
    fn from(sender: mpsc::Sender<OrchestrationCommand>) -> Self {
        OrchestrationCommandSender(sender)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Notification Channel ---

    #[tokio::test]
    async fn test_notification_channel_send_recv() {
        let (tx, mut rx) = ChannelFactory::orchestration_notification_channel(10);

        // Send a notification
        tx.send(OrchestrationNotification::ConnectionError(
            "test".to_string(),
        ))
        .await
        .unwrap();

        // Receive it
        let notification = rx.recv().await.unwrap();
        assert!(matches!(
            notification,
            OrchestrationNotification::ConnectionError(_)
        ));
    }

    #[tokio::test]
    async fn test_notification_channel_capacity() {
        let (tx, _rx) = ChannelFactory::orchestration_notification_channel(100);
        assert_eq!(tx.max_capacity(), 100);
    }

    #[test]
    fn test_notification_sender_clone() {
        let (tx, _rx) = ChannelFactory::orchestration_notification_channel(10);
        let _tx2 = tx.clone(); // Senders are clonable
    }

    #[test]
    fn test_notification_sender_is_closed_when_receiver_dropped() {
        let (tx, rx) = ChannelFactory::orchestration_notification_channel(10);
        assert!(!tx.is_closed());
        drop(rx);
        assert!(tx.is_closed());
    }

    #[tokio::test]
    async fn test_notification_receiver_close() {
        let (tx, mut rx) = ChannelFactory::orchestration_notification_channel(10);
        rx.close();
        // After close, sender should fail
        let result = tx
            .send(OrchestrationNotification::ConnectionError(
                "test".to_string(),
            ))
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_notification_try_recv_empty() {
        let (_tx, mut rx) = ChannelFactory::orchestration_notification_channel(10);
        assert!(rx.try_recv().is_err());
    }

    // --- Command Channel ---

    #[tokio::test]
    async fn test_command_channel_capacity() {
        let (tx, _rx) = ChannelFactory::orchestration_command_channel(500);
        assert_eq!(tx.max_capacity(), 500);
    }

    #[test]
    fn test_command_sender_clone() {
        let (tx, _rx) = ChannelFactory::orchestration_command_channel(10);
        let _tx2 = tx.clone();
    }

    #[test]
    fn test_command_sender_is_closed_when_receiver_dropped() {
        let (tx, rx) = ChannelFactory::orchestration_command_channel(10);
        assert!(!tx.is_closed());
        drop(rx);
        assert!(tx.is_closed());
    }

    #[test]
    fn test_command_sender_inner_access() {
        let (tx, _rx) = ChannelFactory::orchestration_command_channel(10);
        let inner = tx.inner();
        assert_eq!(inner.max_capacity(), 10);
    }

    #[test]
    fn test_command_try_recv_empty() {
        let (_tx, mut rx) = ChannelFactory::orchestration_command_channel(10);
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn test_command_receiver_close() {
        let (_tx, mut rx) = ChannelFactory::orchestration_command_channel(10);
        rx.close();
        // After close, recv returns None
        let result = rx.recv().await;
        assert!(result.is_none());
    }

    // --- From conversions ---

    #[test]
    fn test_notification_sender_from_raw() {
        let (raw_tx, _rx) = mpsc::channel::<OrchestrationNotification>(10);
        let typed_tx: OrchestrationNotificationSender = raw_tx.into();
        assert_eq!(typed_tx.max_capacity(), 10);
    }

    #[test]
    fn test_command_sender_from_raw() {
        let (raw_tx, _rx) = mpsc::channel::<OrchestrationCommand>(20);
        let typed_tx: OrchestrationCommandSender = raw_tx.into();
        assert_eq!(typed_tx.max_capacity(), 20);
    }

    // --- ChannelFactory ---

    #[test]
    fn test_channel_factory_default() {
        let _factory = ChannelFactory::default();
        let _factory2 = ChannelFactory;
    }

    #[test]
    fn test_channel_factory_debug() {
        let factory = ChannelFactory;
        let debug = format!("{:?}", factory);
        assert!(debug.contains("ChannelFactory"));
    }
}
