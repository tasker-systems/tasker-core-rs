# TAS-51: Bounded MPSC Channel Migration

**Status**: In Progress
**Priority**: High
**Created**: 2025-10-11
**Branch**: `jcoletaylor/tas-51-bounded-mpsc-channels`

## Executive Summary

Comprehensive migration from inconsistent unbounded/bounded MPSC channel usage to a fully bounded, configuration-driven approach with proper backpressure handling. This ticket addresses production stability risks from unbounded memory growth and eliminates configuration inconsistencies.

## Problem Statement

### Current State Issues

1. **Unbounded Channels (3 critical sites)**: Risk of unbounded memory growth under load
2. **Configuration Disconnect**: TOML configuration exists but isn't used (6 sites)
3. **Hard-Coded Capacities**: Mix of 100, 1000 values with no rationale
4. **No Backpressure Strategy**: Missing overflow handling policies
5. **No Documentation**: Future developers lack guidelines

### Impact

- **Production Risk**: OOM under high database notification load
- **Operational Pain**: Can't tune channel sizes without code changes
- **Inconsistency**: Test/dev/prod use same hard-coded values
- **Technical Debt**: Wasted configuration infrastructure

## Detailed Analysis

### 1. Unbounded Channels (⚠️ HIGHEST RISK)

#### 1.1 PGMQ Notification Listener
**Location**: `pgmq-notify/src/listener.rs:66`
```rust
let (event_sender, event_receiver) = mpsc::unbounded_channel();
```

**Purpose**: Distribute PostgreSQL LISTEN/NOTIFY events
**Risk Level**: 🔴 **HIGH**
**Scenario**: Database notification burst (e.g., 10k tasks enqueued) → unbounded channel growth → OOM

**Why Unbounded Was Chosen**: Likely to avoid dropping database notifications
**Problem**: No backpressure, no monitoring, no overflow handling

#### 1.2 Event Publisher
**Location**: `tasker-shared/src/events/publisher.rs:134`
```rust
let (queue_sender, queue_receiver) = mpsc::unbounded_channel();
```

**Purpose**: Internal event publishing queue
**Risk Level**: 🟡 **MEDIUM**
**Scenario**: High event volume (step completions, state transitions) → memory buildup

**Why Unbounded**: Unclear - may be legacy decision
**Problem**: Event storms can exhaust memory

#### 1.3 Ruby FFI Event Handler
**Location**: `workers/ruby/ext/tasker_core/src/event_handler.rs:33`
```rust
let (event_sender, event_receiver) = mpsc::unbounded_channel();
```

**Purpose**: Rust → Ruby event communication across FFI boundary
**Risk Level**: 🟡 **MEDIUM**
**Scenario**: Ruby handler slowness → Rust-side event accumulation → memory growth

**Why Unbounded**: FFI boundary - possibly to avoid blocking Rust side
**Problem**: No backpressure signal to Ruby, no overflow detection

### 2. Bounded Channels - Hard-Coded (NOT using configuration)

#### 2.1 Orchestration Event System (2 channels)
**Location**: `tasker-orchestration/src/orchestration/event_systems/orchestration_event_system.rs`
- Line 361: `mpsc::channel(100)`
- Line 407: `mpsc::channel(100)`

**Configuration Available**: `event_systems.toml` has `buffer_size = 1000`
**Problem**: Ignoring configuration, using 10x smaller capacity
**Environment Overrides Ignored**:
- Production: 5000 (task_readiness)
- Development: 500
- Test: 100

#### 2.2 Worker Event System
**Location**: `tasker-worker/src/worker/event_systems/worker_event_system.rs:208`
```rust
mpsc::channel::<WorkerNotification>(1000)
```

**Configuration Available**: `event_systems.toml` has `buffer_size = 1000`
**Problem**: Matches config by coincidence, not design

#### 2.3 Worker Event Subscriber (2 channels)
**Location**: `tasker-worker/src/worker/event_subscriber.rs`
- Line 152: `mpsc::channel(1000)` - Completion channel
- Line 382: `mpsc::channel(1000)` - Result channel

**Configuration Available**: YES
**Problem**: No configuration integration

### 3. Bounded Channels - Partially Configurable

#### 3.1 Orchestration Command Processor ✅ (Architecture Ready)
**Location**: `tasker-orchestration/src/orchestration/command_processor.rs:231`
```rust
let (command_tx, command_rx) = mpsc::channel(buffer_size);
```

**Status**: ⚠️ Takes parameter but caller passes hard-coded value
**Caller**: `core.rs:86` passes `1000`
**Configuration Available**: YES - multiple TOML files
**Environment Overrides Available**:
- Test: 100
- Development: 500
- Production: 5000

**Fix Needed**: Connect caller to configuration

#### 3.2 Worker Command Processor ✅ (Architecture Ready)
**Location**: `tasker-worker/src/worker/command_processor.rs:185`
```rust
let (command_sender, command_receiver) = mpsc::channel(command_buffer_size);
```

**Status**: ⚠️ Configurable interface, not wired to config
**Fix Needed**: Add TOML configuration integration

### 4. Configuration Infrastructure (EXISTS but UNUSED)

#### Current TOML Configuration

**Base Configuration** (`config/tasker/base/`):
```toml
# event_systems.toml
[orchestration_event_system]
buffer_size = 1000
broadcast_buffer_size = 1000

# task_readiness.toml
[task_readiness_event_system]
buffer_size = 1000
```

**Environment-Specific Overrides**:

**Production** (`config/tasker/environments/production/`):
```toml
# task_readiness.toml
buffer_size = 5000

# worker.toml
broadcast_buffer_size = 2000
```

**Development**:
```toml
buffer_size = 500
broadcast_buffer_size = 500
```

**Test**:
```toml
buffer_size = 100
broadcast_buffer_size = 500
```

**Problem**: Configuration exists but code ignores it!

## Solution Design

### Architecture Principles

1. **All Channels Bounded**: Zero unbounded channels in production code
2. **Configuration-Driven**: All capacities from TOML with environment overrides
3. **Documented Strategy**: Clear rationale for capacity choices
4. **Backpressure Handling**: Explicit overflow policies
5. **Observability**: Metrics for channel saturation

### Phase 1: Configuration Schema Enhancement

#### 1.1 Create New MPSC Channel Configuration File

Create unified MPSC channel configuration covering ALL 22 channel creation sites:

```toml
# config/tasker/base/mpsc_channels.toml

# ============================================================================
# ORCHESTRATION CHANNELS
# ============================================================================

[orchestration.command_processor]
# Command processor channel (TAS-40 pattern)
command_buffer_size = 1000

[orchestration.event_systems]
# Event system notification channels
event_channel_buffer_size = 1000

[orchestration.event_listeners]
# PGMQ notification listener (currently unbounded!)
pgmq_event_buffer_size = 10000  # Large for notification bursts

# ============================================================================
# TASK READINESS CHANNELS
# ============================================================================

[task_readiness.event_channel]
# Task readiness event channel (MIGRATED from event_systems.toml)
buffer_size = 1000
send_timeout_ms = 1000

# ============================================================================
# WORKER CHANNELS
# ============================================================================

[worker.command_processor]
# Worker command processor channel
command_buffer_size = 1000

[worker.event_systems]
# Worker event system notification channels
event_channel_buffer_size = 1000

[worker.event_subscribers]
# Step completion and result channels
completion_buffer_size = 1000
result_buffer_size = 1000

[worker.in_process_events]
# In-process event broadcast (MIGRATED from event_systems.toml)
broadcast_buffer_size = 1000

[worker.event_listeners]
# PGMQ notification listener
pgmq_event_buffer_size = 1000

# ============================================================================
# SHARED/CROSS-CUTTING CHANNELS
# ============================================================================

[shared.event_publisher]
# Internal event publishing queue (currently unbounded!)
event_queue_buffer_size = 5000

[shared.ffi]
# Ruby FFI event communication (currently unbounded!)
ruby_event_buffer_size = 1000

# ============================================================================
# OVERFLOW POLICIES
# ============================================================================

[overflow_policy]
# Backpressure behavior when channels full
log_warning_threshold = 0.8  # Log when 80% full
drop_policy = "block"         # "block", "drop_oldest", "drop_newest"

[overflow_policy.metrics]
enabled = true
saturation_check_interval_seconds = 10
```

#### 1.2 Environment-Specific Overrides

**Production** (`config/tasker/environments/production/mpsc_channels.toml`):
```toml
[orchestration.command_processor]
command_buffer_size = 5000

[orchestration.event_listeners]
pgmq_event_buffer_size = 50000  # Handle high load bursts

[task_readiness.event_channel]
buffer_size = 5000

[worker.command_processor]
command_buffer_size = 2000

[worker.in_process_events]
broadcast_buffer_size = 2000

[shared.event_publisher]
event_queue_buffer_size = 10000
```

**Development** (`config/tasker/environments/development/mpsc_channels.toml`):
```toml
[task_readiness.event_channel]
buffer_size = 500

[worker.in_process_events]
broadcast_buffer_size = 500
```

**Test** (`config/tasker/environments/test/mpsc_channels.toml`):
```toml
[orchestration.command_processor]
command_buffer_size = 100

[task_readiness.event_channel]
buffer_size = 100

[worker.in_process_events]
broadcast_buffer_size = 500
```

#### 1.3 Configuration Migration from event_systems.toml

**IMPORTANT**: The following fields must be REMOVED from `event_systems.toml` to avoid duplication:

**From Base Config** (`config/tasker/base/event_systems.toml`):
```toml
# REMOVE these lines:
[event_systems.task_readiness.metadata.event_channel]
buffer_size = 1000              # ← MIGRATE to mpsc_channels.toml
send_timeout_ms = 1000          # ← MIGRATE to mpsc_channels.toml

[event_systems.worker.metadata.in_process_events]
broadcast_buffer_size = 1000    # ← MIGRATE to mpsc_channels.toml
```

**From Test Environment** (`config/tasker/environments/test/event_systems.toml`):
```toml
# REMOVE this section:
[event_systems.worker.metadata.in_process_events]
broadcast_buffer_size = 500     # ← MIGRATE to mpsc_channels.toml
```

**From Development Environment** (`config/tasker/environments/development/event_systems.toml`):
```toml
# REMOVE this section:
[event_systems.worker.metadata.in_process_events]
broadcast_buffer_size = 500     # ← MIGRATE to mpsc_channels.toml
```

**Rationale for Migration**:
- Event systems configuration should focus on deployment modes, timing, health monitoring
- MPSC channel buffer sizing is a lower-level concern deserving its own configuration file
- Single source of truth for ALL channel configurations (not just event system channels)
- Easier discoverability and tuning when all channel configs are together

### Phase 2: Rust Configuration Types

#### 2.1 Create New Configuration Module

```rust
// tasker-shared/src/config/mpsc_channels.rs

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Root MPSC channels configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MpscChannelsConfig {
    pub orchestration: OrchestrationChannelsConfig,
    pub task_readiness: TaskReadinessChannelsConfig,
    pub worker: WorkerChannelsConfig,
    pub shared: SharedChannelsConfig,
    pub overflow_policy: OverflowPolicyConfig,
}

/// Orchestration subsystem channels
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OrchestrationChannelsConfig {
    pub command_processor: OrchestrationCommandProcessorConfig,
    pub event_systems: OrchestrationEventSystemsConfig,
    pub event_listeners: OrchestrationEventListenersConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OrchestrationCommandProcessorConfig {
    pub command_buffer_size: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OrchestrationEventSystemsConfig {
    pub event_channel_buffer_size: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OrchestrationEventListenersConfig {
    pub pgmq_event_buffer_size: usize,
}

/// Task readiness subsystem channels
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskReadinessChannelsConfig {
    pub event_channel: TaskReadinessEventChannelConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TaskReadinessEventChannelConfig {
    pub buffer_size: usize,
    pub send_timeout_ms: u64,
}

impl TaskReadinessEventChannelConfig {
    pub fn send_timeout(&self) -> Duration {
        Duration::from_millis(self.send_timeout_ms)
    }
}

/// Worker subsystem channels
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkerChannelsConfig {
    pub command_processor: WorkerCommandProcessorConfig,
    pub event_systems: WorkerEventSystemsConfig,
    pub event_subscribers: WorkerEventSubscribersConfig,
    pub in_process_events: WorkerInProcessEventsConfig,
    pub event_listeners: WorkerEventListenersConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkerCommandProcessorConfig {
    pub command_buffer_size: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkerEventSystemsConfig {
    pub event_channel_buffer_size: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkerEventSubscribersConfig {
    pub completion_buffer_size: usize,
    pub result_buffer_size: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkerInProcessEventsConfig {
    pub broadcast_buffer_size: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkerEventListenersConfig {
    pub pgmq_event_buffer_size: usize,
}

/// Shared/cross-cutting channels
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SharedChannelsConfig {
    pub event_publisher: SharedEventPublisherConfig,
    pub ffi: SharedFfiConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SharedEventPublisherConfig {
    pub event_queue_buffer_size: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SharedFfiConfig {
    pub ruby_event_buffer_size: usize,
}

/// Overflow policy configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OverflowPolicyConfig {
    /// Threshold for logging warnings (0.0-1.0)
    pub log_warning_threshold: f64,

    /// Drop policy when channel is full
    #[serde(default = "default_drop_policy")]
    pub drop_policy: DropPolicy,

    /// Metrics configuration
    pub metrics: OverflowMetricsConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OverflowMetricsConfig {
    pub enabled: bool,
    pub saturation_check_interval_seconds: u64,
}

impl OverflowMetricsConfig {
    pub fn saturation_check_interval(&self) -> Duration {
        Duration::from_secs(self.saturation_check_interval_seconds)
    }
}

/// Drop policy for channel overflow
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DropPolicy {
    /// Block sender until space available (default)
    Block,
    /// Drop oldest message when full
    DropOldest,
    /// Drop newest message when full
    DropNewest,
}

fn default_drop_policy() -> DropPolicy {
    DropPolicy::Block
}

// Default implementations
impl Default for MpscChannelsConfig {
    fn default() -> Self {
        Self {
            orchestration: OrchestrationChannelsConfig::default(),
            task_readiness: TaskReadinessChannelsConfig::default(),
            worker: WorkerChannelsConfig::default(),
            shared: SharedChannelsConfig::default(),
            overflow_policy: OverflowPolicyConfig::default(),
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = MpscChannelsConfig::default();
        assert_eq!(
            config.orchestration.command_processor.command_buffer_size,
            1000
        );
        assert_eq!(config.overflow_policy.drop_policy, DropPolicy::Block);
    }

    #[test]
    fn test_send_timeout_conversion() {
        let config = TaskReadinessEventChannelConfig {
            buffer_size: 1000,
            send_timeout_ms: 500,
        };
        assert_eq!(config.send_timeout(), Duration::from_millis(500));
    }
}
```

#### 2.2 Integrate with TaskerConfig

```rust
// tasker-shared/src/config/tasker.rs

pub struct TaskerConfig {
    pub database: DatabaseConfig,
    pub telemetry: TelemetryConfig,
    pub task_templates: TaskTemplatesConfig,
    pub system: SystemConfig,
    pub backoff: BackoffConfig,
    pub execution: ExecutionConfig,
    pub queues: QueuesConfig,
    pub orchestration: OrchestrationConfig,
    pub circuit_breakers: CircuitBreakerConfig,
    pub task_readiness: TaskReadinessConfig,
    pub event_systems: EventSystemsConfig,
    pub worker: Option<WorkerConfig>,

    // NEW: MPSC channel configuration
    pub mpsc_channels: MpscChannelsConfig,  // ← Add this field
}
```

#### 2.3 Update Configuration Loader

```rust
// tasker-shared/src/config/loader.rs

impl ConfigLoader {
    pub fn load(environment: &str) -> TaskerResult<TaskerConfig> {
        // ... existing config loading ...

        // Load MPSC channels configuration
        let mpsc_channels = Self::load_component::<MpscChannelsConfig>(
            environment,
            "mpsc_channels"
        )?;

        Ok(TaskerConfig {
            database,
            telemetry,
            // ... other fields ...
            mpsc_channels,  // ← Add to struct initialization
        })
    }
}
```

### Phase 3: Migrate Unbounded → Bounded

#### 3.1 PGMQ Notification Listener (Orchestration)

**Location**: `pgmq-notify/src/listener.rs:66`

**Current**:
```rust
let (event_sender, event_receiver) = mpsc::unbounded_channel();
```

**Migrated**:
```rust
let buffer_size = config.mpsc_channels
    .orchestration.event_listeners.pgmq_event_buffer_size;
let (event_sender, event_receiver) = mpsc::channel(buffer_size);

// Add monitoring
if event_sender.capacity() > 0 {
    let usage = 1.0 - (event_sender.capacity() as f64 / buffer_size as f64);
    if usage > config.mpsc_channels.overflow_policy.log_warning_threshold {
        warn!(
            usage_percent = usage * 100.0,
            "PGMQ notification channel approaching capacity"
        );
    }
}
```

**Overflow Strategy**:
- Monitor channel saturation
- Log warnings at 80% capacity
- Consider notification coalescing for burst scenarios
- Add metrics: `mpsc_channel_saturation{channel="pgmq_notifications"}`

#### 3.2 Event Publisher (Shared)

**Location**: `tasker-shared/src/events/publisher.rs:134`

**Current**:
```rust
let (queue_sender, queue_receiver) = mpsc::unbounded_channel();
```

**Migrated**:
```rust
let buffer_size = config.mpsc_channels
    .shared.event_publisher.event_queue_buffer_size;
let (queue_sender, queue_receiver) = mpsc::channel(buffer_size);

// Implement event dropping policy for overflow
match queue_sender.try_send(event) {
    Ok(_) => {
        metrics::increment_counter!("events_published_total");
    }
    Err(mpsc::error::TrySendError::Full(event)) => {
        warn!(
            event_type = ?event,
            "Event channel full, dropping event (consider increasing buffer_size)"
        );
        metrics::increment_counter!("events_dropped_total");
    }
    Err(mpsc::error::TrySendError::Closed(_)) => {
        error!("Event channel closed unexpectedly");
    }
}
```

**Overflow Strategy**:
- Non-blocking send (`try_send`)
- Drop events when full with metrics
- Priority queue for critical events (future enhancement)

#### 3.3 Ruby FFI Event Handler

**Location**: `workers/ruby/ext/tasker_core/src/event_handler.rs:33`

**Current**:
```rust
let (event_sender, event_receiver) = mpsc::unbounded_channel();
```

**Migrated**:
```rust
let buffer_size = config.mpsc_channels
    .shared.ffi.ruby_event_buffer_size;
let (event_sender, event_receiver) = mpsc::channel(buffer_size);

// Implement backpressure signaling
match event_sender.try_send(event) {
    Ok(_) => Ok(()),
    Err(mpsc::error::TrySendError::Full(_)) => {
        error!(
            "Ruby FFI event channel full - Ruby handlers not keeping up! \
             Increase buffer_size or optimize Ruby handlers"
        );
        Err(TaskerError::WorkerError(
            "FFI event channel saturated - backpressure applied".to_string()
        ))
    }
    Err(mpsc::error::TrySendError::Closed(_)) => {
        Err(TaskerError::WorkerError("FFI channel closed".to_string()))
    }
}
```

**Overflow Strategy**:
- Return error to Rust when full (backpressure signal)
- Ruby side must handle errors gracefully
- Add FFI-level backpressure metrics

### Phase 4: Connect Hard-Coded Sites to Configuration

#### 4.1 Orchestration Event System
**File**: `orchestration_event_system.rs:361, 407`

**Before**:
```rust
let (event_sender, mut event_receiver) = mpsc::channel(100);
```

**After**:
```rust
let buffer_size = self.context.tasker_config.mpsc_channels
    .orchestration.event_systems.event_channel_buffer_size;
let (event_sender, mut event_receiver) = mpsc::channel(buffer_size);
```

#### 4.2 Worker Event System
**File**: `worker_event_system.rs:208`

**Before**:
```rust
mpsc::channel::<WorkerNotification>(1000)
```

**After**:
```rust
let buffer_size = self.context.tasker_config.mpsc_channels
    .worker.event_systems.event_channel_buffer_size;
mpsc::channel::<WorkerNotification>(buffer_size)
```

#### 4.3 Worker Event Subscriber
**File**: `event_subscriber.rs:152, 382`

**Before**:
```rust
let (completion_sender, completion_receiver) = mpsc::channel(1000);
let (result_sender, result_receiver) = mpsc::channel(1000);
```

**After**:
```rust
let completion_buffer = self.context.tasker_config.mpsc_channels
    .worker.event_subscribers.completion_buffer_size;
let (completion_sender, completion_receiver) = mpsc::channel(completion_buffer);

let result_buffer = self.context.tasker_config.mpsc_channels
    .worker.event_subscribers.result_buffer_size;
let (result_sender, result_receiver) = mpsc::channel(result_buffer);
```

#### 4.4 Orchestration Core
**File**: `orchestration/core.rs:86`

**Before**:
```rust
let (mut processor, command_sender) = OrchestrationProcessor::new(
    context.clone(),
    actors,
    context.message_client(),
    1000, // Command buffer size - HARD-CODED
);
```

**After**:
```rust
let command_buffer_size = context.tasker_config.mpsc_channels
    .orchestration.command_processor.command_buffer_size;

let (mut processor, command_sender) = OrchestrationProcessor::new(
    context.clone(),
    actors,
    context.message_client(),
    command_buffer_size,
);
```

#### 4.5 Worker Command Processor
**File**: `worker/command_processor.rs:185`

Already takes parameter, just need to wire config at call site:
```rust
let command_buffer_size = context.tasker_config.mpsc_channels
    .worker.command_processor.command_buffer_size;

let (mut processor, command_sender) = WorkerCommandProcessor::new(
    context.clone(),
    command_buffer_size,
);
```

### Phase 5: Observability & Monitoring

#### Metrics to Add

```rust
// Channel saturation metrics
metrics::gauge!(
    "mpsc_channel_usage_percent",
    usage_percent,
    "channel" => channel_name,
    "component" => component_name
);

// Overflow events
metrics::increment_counter!(
    "mpsc_channel_full_events_total",
    "channel" => channel_name,
    "action" => "blocked" | "dropped"
);

// Throughput
metrics::increment_counter!(
    "mpsc_messages_sent_total",
    "channel" => channel_name
);
metrics::increment_counter!(
    "mpsc_messages_received_total",
    "channel" => channel_name
);
```

#### Health Check Integration

```rust
pub async fn check_channel_health(&self) -> ChannelHealthStatus {
    let channels = vec![
        ("orchestration_command", self.command_sender.capacity()),
        ("worker_event", self.event_sender.capacity()),
        // ... other channels
    ];

    let warnings = channels.iter()
        .filter(|(_, capacity)| {
            let usage = 1.0 - (*capacity as f64 / buffer_size as f64);
            usage > 0.8
        })
        .collect();

    if warnings.is_empty() {
        ChannelHealthStatus::Healthy
    } else {
        ChannelHealthStatus::Degraded { warnings }
    }
}
```

### Phase 6: Documentation

Create comprehensive documentation:

#### 6.1 Architecture Decision Record

**File**: `docs/architecture-decisions/TAS-51-bounded-mpsc-channels.md`

Content:
- Decision rationale: Why bounded channels everywhere
- Capacity sizing guidelines
- Backpressure strategies
- When to use block vs drop policies
- Monitoring and alerting recommendations

#### 6.2 Operational Runbook

**File**: `docs/operations/mpsc-channel-tuning.md`

Content:
- How to diagnose channel saturation
- How to adjust buffer sizes
- Environment-specific recommendations
- Common issues and solutions

#### 6.3 Developer Guidelines

**File**: `docs/development/mpsc-channel-guidelines.md`

Content:
- When to create new channels
- How to choose buffer sizes
- Configuration integration requirements
- Testing strategies

## Implementation Plan

### Phase 1: Configuration Schema (3-4 hours)
- [ ] Create `config/tasker/base/mpsc_channels.toml` with all 22 channel configurations
- [ ] Create environment-specific overrides:
  - [ ] `config/tasker/environments/production/mpsc_channels.toml`
  - [ ] `config/tasker/environments/development/mpsc_channels.toml`
  - [ ] `config/tasker/environments/test/mpsc_channels.toml`
- [ ] **MIGRATION**: Remove duplicate config from event_systems.toml:
  - [ ] Remove `event_systems.task_readiness.metadata.event_channel` from base config
  - [ ] Remove `event_systems.worker.metadata.in_process_events.broadcast_buffer_size` from base config
  - [ ] Remove corresponding sections from test/dev environment overrides
- [ ] **UPDATE event_systems Rust types**: Remove migrated fields:
  - [ ] Remove `buffer_size` and `send_timeout_ms` from `EventChannelConfig` struct
  - [ ] Remove `broadcast_buffer_size` from `InProcessEventConfig` struct
- [ ] Add Rust configuration types in `tasker-shared/src/config/mpsc_channels.rs`
- [ ] Integrate `MpscChannelsConfig` with `TaskerConfig`
- [ ] Update `ConfigLoader` to load mpsc_channels component
- [ ] Add configuration validation tests
- [ ] Update config-validator binary to validate mpsc_channels

### Phase 2: Unbounded → Bounded Migration (6-8 hours)
- [ ] Migrate PGMQ notification listener
- [ ] Migrate event publisher
- [ ] Migrate Ruby FFI event handler
- [ ] Add overflow handling for each
- [ ] Add monitoring and metrics
- [ ] Test under load

### Phase 3: Connect Hard-Coded Sites (3-4 hours)
- [ ] Update orchestration event system (2 sites)
- [ ] Update worker event system
- [ ] Update worker event subscriber (2 sites)
- [ ] Update orchestration core caller
- [ ] Update worker processor caller

### Phase 4: Observability (2-3 hours)
- [ ] Add channel saturation metrics
- [ ] Add overflow event counters
- [ ] Integrate with health checks
- [ ] Add Grafana dashboard recommendations

### Phase 5: Testing (4-5 hours)
- [ ] Unit tests for configuration loading
- [ ] Integration tests with various buffer sizes
- [ ] Load tests to verify backpressure
- [ ] FFI integration tests with bounded channels
- [ ] Test environment-specific overrides

### Phase 6: Documentation (2-3 hours)
- [ ] Architecture decision record
- [ ] Operational runbook
- [ ] Developer guidelines
- [ ] Update CLAUDE.md with channel strategy

## Success Criteria

✅ **Zero Unbounded Channels**: No `unbounded_channel()` calls in production code
✅ **100% Configurable**: All channel capacities from TOML configuration
✅ **Environment Overrides**: Test/dev/prod use appropriate buffer sizes
✅ **Backpressure Handling**: Explicit overflow policies implemented
✅ **Observability**: Metrics for channel saturation and overflows
✅ **Documentation**: Complete guidelines for future developers
✅ **Tests Passing**: All existing tests still pass
✅ **Load Testing**: System stable under 10x normal load

## Risk Assessment

### High Risk Items

1. **PGMQ Notification Listener** 🔴
   - **Risk**: Notification loss during migration
   - **Mitigation**: Gradual rollout, extensive load testing
   - **Rollback**: Keep unbounded option via feature flag initially

2. **Ruby FFI Channel** 🟡
   - **Risk**: Breaking Ruby integration
   - **Mitigation**: FFI integration tests, Ruby-side error handling
   - **Rollback**: Ruby handlers must handle backpressure errors

### Medium Risk Items

3. **Event Publisher** 🟡
   - **Risk**: Event loss
   - **Mitigation**: Non-critical events only, metrics for drops
   - **Rollback**: Increase buffer size in config

### Low Risk Items

4. **Configuration Integration** 🟢
   - **Risk**: Configuration loading errors
   - **Mitigation**: Validation tests, sensible defaults
   - **Rollback**: Easy config revert

## Testing Strategy

### Unit Tests
```rust
#[test]
fn test_mpsc_channel_config_loading() {
    let config = TaskerConfig::load_from_env("test").unwrap();
    assert_eq!(config.mpsc_channels.command_channels.orchestration_buffer_size, 100);
}

#[test]
fn test_environment_overrides() {
    let prod = TaskerConfig::load_from_env("production").unwrap();
    let dev = TaskerConfig::load_from_env("development").unwrap();
    assert!(prod.mpsc_channels.event_channels.notification_buffer_size
        > dev.mpsc_channels.event_channels.notification_buffer_size);
}
```

### Integration Tests
```rust
#[tokio::test]
async fn test_bounded_channel_backpressure() {
    // Create small buffer
    let config = test_config_with_buffer_size(10);
    let (tx, mut rx) = mpsc::channel(config.buffer_size);

    // Fill the channel
    for i in 0..10 {
        tx.send(i).await.unwrap();
    }

    // Next send should apply backpressure
    let send_result = tokio::time::timeout(
        Duration::from_millis(100),
        tx.send(11)
    ).await;

    assert!(send_result.is_err(), "Should block on full channel");
}
```

### Load Tests
```bash
# Simulate 10k notification burst
cargo test --test load_test -- test_pgmq_notification_burst --ignored

# Verify no OOM with bounded channels
cargo test --test load_test -- test_memory_stability --ignored
```

## Rollout Plan

### Stage 1: Configuration Infrastructure (Week 1)
- Add configuration schema
- Integrate with SystemContext
- No behavior changes yet

### Stage 2: Bounded Migration - Low Risk Sites (Week 2)
- Event subscriber channels
- Event system channels
- Monitor for issues

### Stage 3: Bounded Migration - High Risk Sites (Week 3)
- PGMQ notification listener (with feature flag)
- Event publisher
- Ruby FFI handler
- Extensive load testing

### Stage 4: Documentation & Cleanup (Week 4)
- Complete documentation
- Remove feature flags
- Finalize monitoring

## Open Questions

1. **Notification Coalescing**: Should we coalesce duplicate PGMQ notifications?
   - **Decision**: Not in TAS-51 scope, future optimization

2. **Priority Queues**: Should critical events bypass overflow drops?
   - **Decision**: Not in TAS-51 scope, evaluate based on metrics

3. **Dynamic Buffer Sizing**: Should buffers grow/shrink at runtime?
   - **Decision**: No - use configuration for simplicity

4. **Graceful Degradation**: How should system behave when all channels saturated?
   - **Decision**: Block senders, log warnings, alert operators

## Related Tickets

- **TAS-40**: Command pattern implementation (uses bounded channels)
- **TAS-46**: Actor pattern (command channels configured here)
- **TAS-43**: Event-driven systems (notification channels affected)

## References

- Tokio MPSC documentation: https://docs.rs/tokio/latest/tokio/sync/mpsc/
- Backpressure strategies: https://ryhl.io/blog/async-what-is-blocking/
- Configuration management: `docs/crate-architecture.md`

---

**Estimated Total Effort**: 19-26 hours
**Target Completion**: 1-2 weeks
**Risk Level**: Medium (mitigated by phased rollout)
