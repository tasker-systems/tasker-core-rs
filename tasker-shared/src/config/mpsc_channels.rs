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
//! - `task_readiness.event_channel` (buffer_size, send_timeout_ms)
//! - `worker.in_process_events.broadcast_buffer_size`
//!
//! ### Key principles:
//! - All channels are bounded to prevent unbounded memory growth
//! - Configuration-driven sizing with environment-specific overrides
//! - Single source of truth for all MPSC channel capacities
//! - Explicit overflow policies and backpressure handling

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Root MPSC channels configuration
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MpscChannelsConfig {
    /// Orchestration subsystem channels
    pub orchestration: OrchestrationChannelsConfig,
    /// Task readiness subsystem channels
    pub task_readiness: TaskReadinessChannelsConfig,
    /// Worker subsystem channels
    pub worker: WorkerChannelsConfig,
    /// Shared/cross-cutting channels
    pub shared: SharedChannelsConfig,
    /// Overflow policy configuration
    pub overflow_policy: OverflowPolicyConfig,
}

// ============================================================================
// ORCHESTRATION CHANNELS
// ============================================================================

/// Orchestration subsystem channels configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OrchestrationChannelsConfig {
    /// Command processor channel configuration
    pub command_processor: OrchestrationCommandProcessorConfig,
    /// Event systems channel configuration
    pub event_systems: OrchestrationEventSystemsConfig,
    /// Event listeners channel configuration
    pub event_listeners: OrchestrationEventListenersConfig,
}

/// Orchestration command processor configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OrchestrationCommandProcessorConfig {
    /// Command channel buffer size
    /// Handles: InitializeTask, ProcessStepResult, FinalizeTask commands
    pub command_buffer_size: usize,
}

/// Orchestration event systems configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OrchestrationEventSystemsConfig {
    /// Event channel buffer size
    /// Handles: PGMQ message ready events, internal coordination events
    pub event_channel_buffer_size: usize,
}

/// Orchestration event listeners configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OrchestrationEventListenersConfig {
    /// PGMQ event buffer size
    /// Handles: PostgreSQL LISTEN/NOTIFY events for orchestration_* queues
    /// Previously unbounded! Now bounded to prevent OOM during notification bursts
    pub pgmq_event_buffer_size: usize,
}

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
// WORKER CHANNELS
// ============================================================================

/// Worker subsystem channels configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkerChannelsConfig {
    /// Command processor channel configuration
    pub command_processor: WorkerCommandProcessorConfig,
    /// Event systems channel configuration
    pub event_systems: WorkerEventSystemsConfig,
    /// Event subscribers channel configuration
    pub event_subscribers: WorkerEventSubscribersConfig,
    /// In-process events channel configuration
    pub in_process_events: WorkerInProcessEventsConfig,
    /// Event listeners channel configuration
    pub event_listeners: WorkerEventListenersConfig,
}

/// Worker command processor configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkerCommandProcessorConfig {
    /// Command channel buffer size
    /// Handles: ExecuteStep, SendStepResult, ProcessStepCompletion commands
    pub command_buffer_size: usize,
}

/// Worker event systems configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkerEventSystemsConfig {
    /// Event channel buffer size
    /// Handles: PGMQ message ready events for namespace queues
    pub event_channel_buffer_size: usize,
}

/// Worker event subscribers configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkerEventSubscribersConfig {
    /// Completion channel buffer size
    /// Handles: Step execution completion notifications
    pub completion_buffer_size: usize,
    /// Result channel buffer size
    /// Handles: Step result processing
    pub result_buffer_size: usize,
}

/// Worker in-process events configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkerInProcessEventsConfig {
    /// Broadcast channel buffer size
    /// MIGRATED from event_systems.toml
    /// Handles: Rust → Ruby event broadcasts across FFI boundary
    pub broadcast_buffer_size: usize,
}

/// Worker event listeners configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkerEventListenersConfig {
    /// PGMQ event buffer size
    /// Handles: PostgreSQL LISTEN/NOTIFY events for {namespace}_queue
    pub pgmq_event_buffer_size: usize,
}

// ============================================================================
// SHARED/CROSS-CUTTING CHANNELS
// ============================================================================

/// Shared/cross-cutting channels configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SharedChannelsConfig {
    /// Event publisher configuration
    pub event_publisher: SharedEventPublisherConfig,
    /// FFI configuration
    pub ffi: SharedFfiConfig,
}

/// Shared event publisher configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SharedEventPublisherConfig {
    /// Event queue buffer size
    /// Previously unbounded! Now bounded to prevent event storm OOM
    /// Handles: System-wide event publishing (step completions, state transitions)
    pub event_queue_buffer_size: usize,
}

/// Shared FFI configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SharedFfiConfig {
    /// Ruby event buffer size
    /// Previously unbounded! Now bounded to provide backpressure
    /// Handles: Rust → Ruby event communication across FFI boundary
    pub ruby_event_buffer_size: usize,
}

// ============================================================================
// OVERFLOW POLICIES
// ============================================================================

/// Overflow policy configuration for channel backpressure
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OverflowPolicyConfig {
    /// Threshold for logging warnings (0.0-1.0)
    /// Log warning when channel usage exceeds this threshold
    pub log_warning_threshold: f64,

    /// Drop policy when channel is full
    #[serde(default = "default_drop_policy")]
    pub drop_policy: DropPolicy,

    /// Metrics configuration
    pub metrics: OverflowMetricsConfig,
}

/// Overflow metrics configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OverflowMetricsConfig {
    /// Whether to enable overflow metrics collection
    pub enabled: bool,
    /// Interval for checking channel saturation in seconds
    pub saturation_check_interval_seconds: u64,
}

impl OverflowMetricsConfig {
    /// Get saturation check interval as Duration
    pub fn saturation_check_interval(&self) -> Duration {
        Duration::from_secs(self.saturation_check_interval_seconds)
    }
}

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

fn default_drop_policy() -> DropPolicy {
    DropPolicy::Block
}

// ============================================================================
// DEFAULT IMPLEMENTATIONS
// ============================================================================

impl Default for OrchestrationChannelsConfig {
    fn default() -> Self {
        Self {
            command_processor: OrchestrationCommandProcessorConfig {
                command_buffer_size: 1000,
            },
            event_systems: OrchestrationEventSystemsConfig {
                event_channel_buffer_size: 1000,
            },
            event_listeners: OrchestrationEventListenersConfig {
                pgmq_event_buffer_size: 10000,
            },
        }
    }
}

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

impl Default for WorkerChannelsConfig {
    fn default() -> Self {
        Self {
            command_processor: WorkerCommandProcessorConfig {
                command_buffer_size: 1000,
            },
            event_systems: WorkerEventSystemsConfig {
                event_channel_buffer_size: 1000,
            },
            event_subscribers: WorkerEventSubscribersConfig {
                completion_buffer_size: 1000,
                result_buffer_size: 1000,
            },
            in_process_events: WorkerInProcessEventsConfig {
                broadcast_buffer_size: 1000,
            },
            event_listeners: WorkerEventListenersConfig {
                pgmq_event_buffer_size: 1000,
            },
        }
    }
}

impl Default for SharedChannelsConfig {
    fn default() -> Self {
        Self {
            event_publisher: SharedEventPublisherConfig {
                event_queue_buffer_size: 5000,
            },
            ffi: SharedFfiConfig {
                ruby_event_buffer_size: 1000,
            },
        }
    }
}

impl Default for OverflowPolicyConfig {
    fn default() -> Self {
        Self {
            log_warning_threshold: 0.8,
            drop_policy: DropPolicy::Block,
            metrics: OverflowMetricsConfig {
                enabled: true,
                saturation_check_interval_seconds: 10,
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

        // Test orchestration defaults
        assert_eq!(
            config.orchestration.command_processor.command_buffer_size,
            1000
        );
        assert_eq!(
            config.orchestration.event_listeners.pgmq_event_buffer_size,
            10000
        );

        // Test task readiness defaults
        assert_eq!(config.task_readiness.event_channel.buffer_size, 1000);
        assert_eq!(config.task_readiness.event_channel.send_timeout_ms, 1000);

        // Test worker defaults
        assert_eq!(config.worker.command_processor.command_buffer_size, 1000);
        assert_eq!(config.worker.in_process_events.broadcast_buffer_size, 1000);

        // Test shared defaults
        assert_eq!(config.shared.event_publisher.event_queue_buffer_size, 5000);
        assert_eq!(config.shared.ffi.ruby_event_buffer_size, 1000);

        // Test overflow policy defaults
        assert_eq!(config.overflow_policy.log_warning_threshold, 0.8);
        assert_eq!(config.overflow_policy.drop_policy, DropPolicy::Block);
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
        let config = OverflowMetricsConfig {
            enabled: true,
            saturation_check_interval_seconds: 30,
        };
        assert_eq!(config.saturation_check_interval(), Duration::from_secs(30));
    }

    #[test]
    fn test_drop_policy_serialization() {
        // Test default
        let policy = default_drop_policy();
        assert_eq!(policy, DropPolicy::Block);

        // Test equality
        assert_eq!(DropPolicy::Block, DropPolicy::Block);
        assert_ne!(DropPolicy::Block, DropPolicy::DropOldest);
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

        // Verify key values match
        assert_eq!(
            config.orchestration.command_processor.command_buffer_size,
            deserialized
                .orchestration
                .command_processor
                .command_buffer_size
        );
        assert_eq!(
            config.overflow_policy.drop_policy,
            deserialized.overflow_policy.drop_policy
        );
    }
}
