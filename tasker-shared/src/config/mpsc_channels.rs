//! # MPSC Channels Configuration
//!
//! Unified configuration for all Tokio MPSC channel buffer sizes across the system.
//!
//! ## Architecture Decision: TAS-51 - Bounded MPSC Channel Migration
//!
//! This module provides configuration for all MPSC channels in the tasker-core system,
//! replacing previous unbounded channels and hard-coded buffer sizes with a unified,
//! configuration-driven approach.
//!
//! ### Configuration migrated from event_systems.toml (TAS-51):
//! - `task_readiness.event_channel` (buffer_size, send_timeout_ms) - **REMOVED (zero runtime usage)**
//! - `worker.in_process_events.broadcast_buffer_size`
//!
//! ### Key principles:
//! - All channels are bounded to prevent unbounded memory growth
//! - Configuration-driven sizing with environment-specific overrides
//! - Single source of truth for all MPSC channel capacities
//! - Explicit overflow policies and backpressure handling

use serde::{Deserialize, Serialize};
use std::time::Duration;

// Import MPSC channel config structs from V2
pub use crate::config::tasker::{
    CommandProcessorChannels, EventListenerChannels, EventPublisherChannels, EventSystemChannels,
    FfiChannels, OrchestrationMpscChannelsConfig, OverflowMetricsConfig, OverflowPolicyConfig,
    SharedMpscChannelsConfig, WorkerCommandProcessorChannels, WorkerEventListenerChannels,
    WorkerEventSubscriberChannels, WorkerEventSystemChannels, WorkerInProcessEventChannels,
    WorkerMpscChannelsConfig,
};

// Type aliases for backward compatibility (legacy names → V2 names)
pub type OrchestrationChannelsConfig = OrchestrationMpscChannelsConfig;
pub type OrchestrationCommandProcessorConfig = CommandProcessorChannels;
pub type OrchestrationEventSystemsConfig = EventSystemChannels;
pub type OrchestrationEventListenersConfig = EventListenerChannels;

pub type WorkerChannelsConfig = WorkerMpscChannelsConfig;
pub type WorkerCommandProcessorConfig = WorkerCommandProcessorChannels;
pub type WorkerEventSystemsConfig = WorkerEventSystemChannels;
pub type WorkerEventSubscribersConfig = WorkerEventSubscriberChannels;
pub type WorkerInProcessEventsConfig = WorkerInProcessEventChannels;
pub type WorkerEventListenersConfig = WorkerEventListenerChannels;

pub type SharedChannelsConfig = SharedMpscChannelsConfig;
pub type SharedEventPublisherConfig = EventPublisherChannels;
pub type SharedFfiConfig = FfiChannels;

/// Root MPSC channels configuration
///
/// This is an adapter over V2's context-specific channel configs. While V2 embeds
/// channel configs into SharedConfig, OrchestrationConfig, and WorkerConfig, this
/// provides a unified root configuration for backward compatibility.
///
/// **Pattern**: Adapter aggregating V2 context-specific configs
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MpscChannelsConfig {
    /// Orchestration subsystem channels (from V2 OrchestrationMpscChannelsConfig)
    pub orchestration: OrchestrationChannelsConfig,
    /// Task readiness subsystem channels (DEPRECATED: zero runtime usage, kept for compatibility)
    pub task_readiness: TaskReadinessChannelsConfig,
    /// Worker subsystem channels (from V2 WorkerMpscChannelsConfig)
    pub worker: WorkerChannelsConfig,
    /// Shared/cross-cutting channels (from V2 SharedMpscChannelsConfig)
    pub shared: SharedChannelsConfig,
    /// Overflow policy configuration (from V2)
    pub overflow_policy: OverflowPolicyConfig,
}

// ============================================================================
// ORCHESTRATION, WORKER, SHARED CHANNELS - Now imported from tasker.rs
// ============================================================================
// All channel configuration structs are now imported from tasker.rs
// See type aliases at the top of this file for backward compatibility mapping

// ============================================================================
// TASK READINESS CHANNELS
// ============================================================================

/// Task readiness subsystem channels configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskReadinessChannelsConfig {
    /// Event channel configuration
    /// MIGRATED from event_systems.toml
    pub event_channel: TaskReadinessEventChannelConfig,
}

/// Task readiness event channel configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskReadinessEventChannelConfig {
    /// Channel buffer size
    /// Handles: Task readiness notifications and state change events
    pub buffer_size: usize,
    /// Send timeout in milliseconds
    pub send_timeout_ms: u64,
}

impl TaskReadinessEventChannelConfig {
    /// Get send timeout as Duration
    pub fn send_timeout(&self) -> Duration {
        Duration::from_millis(self.send_timeout_ms)
    }
}

// ============================================================================
// OVERFLOW POLICIES - Keep DropPolicy enum for type safety
// ============================================================================
// Note: V2's OverflowPolicyConfig uses String for drop_policy, but we keep
// the DropPolicy enum for type safety and provide conversion methods

/// Drop policy for channel overflow
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DropPolicy {
    /// Block sender until space available (default)
    /// Provides natural backpressure
    Block,
    /// Drop oldest message when full
    /// Use for time-sensitive data where latest is most important
    DropOldest,
    /// Drop newest message when full
    /// Use for batch processing where preserving history matters
    DropNewest,
}

// ============================================================================
// HELPER METHODS
// ============================================================================

impl OverflowMetricsConfig {
    /// Get saturation check interval as Duration
    ///
    /// Note: V2 uses u32 for saturation_check_interval_seconds, cast to u64 for Duration
    pub fn saturation_check_interval(&self) -> Duration {
        Duration::from_secs(self.saturation_check_interval_seconds as u64)
    }
}

// ============================================================================
// DEFAULT IMPLEMENTATIONS
// ============================================================================
// Note: Default implementations for V2-imported types come from V2's impl_builder_default!
// Only need Default for adapter types and deprecated types

impl Default for TaskReadinessChannelsConfig {
    fn default() -> Self {
        Self {
            event_channel: TaskReadinessEventChannelConfig {
                buffer_size: 1000,
                send_timeout_ms: 1000,
            },
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_creation() {
        let config = MpscChannelsConfig::default();

        // Test orchestration defaults (V2 types use u32)
        assert_eq!(
            config.orchestration.command_processor.command_buffer_size,
            5000 // V2 default
        );
        assert_eq!(
            config.orchestration.event_listeners.pgmq_event_buffer_size,
            5000 // V2 default
        );

        // TAS-61: task_readiness deprecated (zero runtime usage)
        assert_eq!(config.task_readiness.event_channel.buffer_size, 1000);

        // Test worker defaults (V2 types use u32)
        assert_eq!(config.worker.command_processor.command_buffer_size, 2000); // V2 default
        assert_eq!(config.worker.in_process_events.broadcast_buffer_size, 1000);

        // Test shared defaults (V2 types use u32)
        assert_eq!(config.shared.event_publisher.event_queue_buffer_size, 5000);
        assert_eq!(config.shared.ffi.ruby_event_buffer_size, 1000);

        // Test overflow policy defaults (V2 OverflowPolicyConfig)
        assert_eq!(config.overflow_policy.log_warning_threshold, 0.8);
        assert!(config.overflow_policy.metrics.enabled);
    }

    #[test]
    fn test_send_timeout_conversion() {
        let config = TaskReadinessEventChannelConfig {
            buffer_size: 1000,
            send_timeout_ms: 500,
        };
        assert_eq!(config.send_timeout(), Duration::from_millis(500));
    }

    #[test]
    fn test_saturation_check_interval_conversion() {
        // V2 uses u32 for saturation_check_interval_seconds
        let config = OverflowMetricsConfig {
            enabled: true,
            saturation_check_interval_seconds: 30, // u32 in V2
        };
        assert_eq!(config.saturation_check_interval(), Duration::from_secs(30));
    }

    #[test]
    fn test_drop_policy_equality() {
        // Test equality - DropPolicy enum kept for type safety (V2 uses String)
        assert_eq!(DropPolicy::Block, DropPolicy::Block);
        assert_ne!(DropPolicy::Block, DropPolicy::DropOldest);
        assert_ne!(DropPolicy::DropOldest, DropPolicy::DropNewest);
    }

    #[test]
    fn test_type_alias_compatibility() {
        // Verify type aliases work correctly
        let orchestration: OrchestrationChannelsConfig = OrchestrationMpscChannelsConfig::default();
        let worker: WorkerChannelsConfig = WorkerMpscChannelsConfig::default();
        let shared: SharedChannelsConfig = SharedMpscChannelsConfig::default();

        // Verify V2 types use u32
        assert_eq!(orchestration.command_processor.command_buffer_size, 5000);
        assert_eq!(worker.command_processor.command_buffer_size, 2000);
        assert_eq!(shared.event_publisher.event_queue_buffer_size, 5000);
    }

    #[test]
    fn test_serde_round_trip() {
        let config = MpscChannelsConfig::default();

        // Serialize to TOML
        let serialized = toml::to_string(&config).expect("Failed to serialize");
        assert!(!serialized.is_empty());

        // Deserialize back
        let deserialized: MpscChannelsConfig =
            toml::from_str(&serialized).expect("Failed to deserialize");

        // Verify key values match (using V2 types with u32)
        assert_eq!(
            config.orchestration.command_processor.command_buffer_size,
            deserialized
                .orchestration
                .command_processor
                .command_buffer_size
        );
        assert_eq!(
            config.overflow_policy.log_warning_threshold,
            deserialized.overflow_policy.log_warning_threshold
        );
        // V2's OverflowPolicyConfig.drop_policy is String, not enum
        assert_eq!(
            config.overflow_policy.drop_policy,
            deserialized.overflow_policy.drop_policy
        );
    }
}
