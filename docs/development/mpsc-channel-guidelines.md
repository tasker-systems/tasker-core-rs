# MPSC Channel Development Guidelines

**Last Updated**: 2025-10-14
**Audience**: Engineers working on tasker-core
**Related**: TAS-51, [ADR: Bounded MPSC Channels](../decisions/TAS-51-bounded-mpsc-channels.md)

## Overview

This guide provides development guidelines for creating, configuring, and using MPSC channels in the tasker-core system. Following these guidelines ensures consistent, safe, and observable channel usage.

## Golden Rules

1. **ALL channels MUST be bounded** - `unbounded_channel()` is forbidden
2. **ALL channels MUST be configured via TOML** - No hard-coded buffer sizes
3. **ALL channels MUST have monitoring** - Use ChannelMonitor for observability
4. **ALL channels MUST handle backpressure** - Document overflow behavior
5. **ALL configurations MUST have environment overrides** - Test with small buffers

## Creating a New Channel

### Step 1: Add TOML Configuration

**Location**: `config/tasker/base/mpsc_channels.toml`

```toml
[mpsc_channels.your_subsystem.your_component]
# Brief description of what this channel does
your_channel_buffer_size = 1000  # Base capacity

# Optional: Related configuration
your_channel_timeout_ms = 1000
```

**Naming Convention**:
- Subsystem: `orchestration`, `task_readiness`, `worker`, `shared`
- Component: Descriptive name (snake_case)
- Field: `<purpose>_buffer_size` or `<purpose>_channel_buffer_size`

**Example**:
```toml
[mpsc_channels.worker.step_processor]
# Processes step execution requests from orchestration
execution_request_buffer_size = 1000
```

### Step 2: Add Environment Overrides

**Test** (`config/tasker/environments/test/mpsc_channels.toml`):
```toml
[mpsc_channels.worker.step_processor]
execution_request_buffer_size = 100  # Small to expose backpressure
```

**Production** (`config/tasker/environments/production/mpsc_channels.toml`):
```toml
[mpsc_channels.worker.step_processor]
execution_request_buffer_size = 5000  # Large for production load
```

**Critical**: Use full `[mpsc_channels.*]` prefix in environment files!

### Step 3: Add Rust Configuration Type

**Location**: `tasker-shared/src/config/mpsc_channels.rs`

```rust
// Add to appropriate subsystem config struct
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkerStepProcessorConfig {
    pub execution_request_buffer_size: usize,
    // Optional: Related config
    pub execution_request_timeout_ms: u64,
}

impl WorkerStepProcessorConfig {
    pub fn execution_request_timeout(&self) -> Duration {
        Duration::from_millis(self.execution_request_timeout_ms)
    }
}

// Add to parent struct
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkerChannelsConfig {
    // ... existing fields
    pub step_processor: WorkerStepProcessorConfig,
}

// Add to Default implementation
impl Default for WorkerChannelsConfig {
    fn default() -> Self {
        Self {
            // ... existing fields
            step_processor: WorkerStepProcessorConfig {
                execution_request_buffer_size: 1000,
            },
        }
    }
}
```

### Step 4: Create Channel with Configuration

```rust
use tokio::sync::mpsc;
use crate::config::MpscChannelsConfig;

pub struct YourComponent {
    sender: mpsc::Sender<YourMessage>,
    config: Arc<MpscChannelsConfig>,
}

impl YourComponent {
    pub fn new(config: Arc<MpscChannelsConfig>) -> Self {
        // Get buffer size from configuration
        let buffer_size = config.worker.step_processor.execution_request_buffer_size;

        // Create bounded channel
        let (sender, receiver) = mpsc::channel(buffer_size);

        // Spawn receiver task
        tokio::spawn(Self::process_messages(receiver));

        Self { sender, config }
    }

    async fn process_messages(mut receiver: mpsc::Receiver<YourMessage>) {
        while let Some(message) = receiver.recv().await {
            // Process message
        }
    }
}
```

### Step 5: Add Monitoring

```rust
use crate::monitoring::ChannelMonitor;

pub struct YourComponent {
    sender: mpsc::Sender<YourMessage>,
    monitor: Arc<ChannelMonitor>,
}

impl YourComponent {
    pub fn new(config: Arc<MpscChannelsConfig>) -> Self {
        let buffer_size = config.worker.step_processor.execution_request_buffer_size;
        let (sender, receiver) = mpsc::channel(buffer_size);

        // Initialize monitoring
        let monitor = ChannelMonitor::new(
            "step_execution_requests",  // Channel name for metrics
            "worker",                    // Component name
            buffer_size,
        );

        // Clone for receiver task
        let monitor_clone = monitor.clone();
        tokio::spawn(Self::process_messages(receiver, monitor_clone));

        Self { sender, monitor }
    }

    async fn send_message(&self, msg: YourMessage) -> Result<(), SendError> {
        // Send with monitoring
        self.sender.send(msg).await?;

        // Update metrics
        self.monitor.record_send();

        // Check saturation
        if self.monitor.is_saturated(0.8) {
            warn!(
                channel = "step_execution_requests",
                usage_percent = self.monitor.usage_percent(),
                "Channel approaching capacity"
            );
        }

        Ok(())
    }

    async fn process_messages(
        mut receiver: mpsc::Receiver<YourMessage>,
        monitor: Arc<ChannelMonitor>,
    ) {
        while let Some(message) = receiver.recv().await {
            monitor.record_receive();
            // Process message
        }
    }
}
```

### Step 6: Implement Backpressure Handling

Choose appropriate strategy based on use case:

#### Strategy 1: Block (Default)

**When to use**: Cannot drop messages, must maintain ordering

```rust
// Blocks until space available - natural backpressure
async fn send_critical(&self, msg: Message) -> Result<()> {
    self.sender.send(msg).await?;  // Blocks if full
    Ok(())
}
```

#### Strategy 2: Try Send with Drop

**When to use**: Non-critical messages, can tolerate loss

```rust
async fn send_best_effort(&self, msg: Message) -> Result<()> {
    match self.sender.try_send(msg) {
        Ok(_) => {
            metrics::increment_counter!("messages_sent_total");
            Ok(())
        }
        Err(mpsc::error::TrySendError::Full(msg)) => {
            warn!("Channel full, dropping message");
            metrics::increment_counter!("messages_dropped_total");
            Err(Error::ChannelFull)
        }
        Err(mpsc::error::TrySendError::Closed(_)) => {
            Err(Error::ChannelClosed)
        }
    }
}
```

#### Strategy 3: Try Send with Timeout

**When to use**: Balance between blocking and dropping

```rust
use tokio::time::{timeout, Duration};

async fn send_with_timeout(&self, msg: Message) -> Result<()> {
    let send_timeout = Duration::from_secs(5);

    match timeout(send_timeout, self.sender.send(msg)).await {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(send_error)) => Err(Error::Send(send_error)),
        Err(_) => {
            warn!("Send timeout - channel saturated");
            metrics::increment_counter!("send_timeouts_total");
            Err(Error::SendTimeout)
        }
    }
}
```

#### Strategy 4: Try Send with Retry

**When to use**: Important messages, temporary saturation

```rust
async fn send_with_retry(&self, msg: Message, max_retries: usize) -> Result<()> {
    let mut attempts = 0;

    loop {
        match self.sender.try_send(msg.clone()) {
            Ok(_) => return Ok(()),
            Err(mpsc::error::TrySendError::Full(_)) if attempts < max_retries => {
                attempts += 1;
                warn!(attempts, "Channel full, retrying");
                tokio::time::sleep(Duration::from_millis(10 * attempts as u64)).await;
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                return Err(Error::MaxRetriesExceeded);
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                return Err(Error::ChannelClosed);
            }
        }
    }
}
```

### Step 7: Add Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_channel_configuration() {
        let config = MpscChannelsConfig::default();
        let component = YourComponent::new(Arc::new(config));

        // Verify buffer size used
        assert_eq!(component.sender.capacity(), 1000);
    }

    #[tokio::test]
    async fn test_backpressure_handling() {
        // Small buffer to test backpressure
        let mut config = MpscChannelsConfig::default();
        config.worker.step_processor.execution_request_buffer_size = 10;

        let component = YourComponent::new(Arc::new(config));

        // Fill channel
        for i in 0..10 {
            component.send_message(Message { id: i }).await.unwrap();
        }

        // Next send should timeout (if using timeout strategy)
        let result = tokio::time::timeout(
            Duration::from_millis(100),
            component.send_message(Message { id: 11 })
        ).await;

        assert!(result.is_err(), "Should timeout on full channel");
    }

    #[tokio::test]
    async fn test_environment_overrides() {
        // Test configuration loads correctly
        let test_config = TaskerConfig::load_from_env("test").unwrap();
        let prod_config = TaskerConfig::load_from_env("production").unwrap();

        assert!(
            prod_config.mpsc_channels.worker.step_processor.execution_request_buffer_size
            > test_config.mpsc_channels.worker.step_processor.execution_request_buffer_size,
            "Production buffer should be larger than test"
        );
    }
}
```

## Choosing Buffer Sizes

### Initial Sizing Guidelines

| Use Case | Base Size | Test Size | Production Size |
|----------|-----------|-----------|-----------------|
| Command channels | 1000 | 100 | 2000-5000 |
| Event notification | 1000 | 100 | 1000-5000 |
| Burst handling (PGMQ) | 10000 | 10000 | 50000 |
| FFI boundary | 1000 | 500 | 2000 |
| Internal events | 5000 | 5000 | 10000 |

### Sizing Formula

```
buffer_size = (peak_message_rate_per_sec * avg_processing_time_sec) * safety_factor

Where:
- peak_message_rate_per_sec: Expected maximum throughput
- avg_processing_time_sec: Average consumer processing time
- safety_factor: 2-5x for burst handling
```

### Example Calculation

```rust
// Command processor sizing
const PEAK_COMMANDS_PER_SEC: usize = 100;
const AVG_PROCESSING_TIME_MS: usize = 50;  // 50ms average
const SAFETY_FACTOR: usize = 3;

const BUFFER_SIZE: usize = (PEAK_COMMANDS_PER_SEC * AVG_PROCESSING_TIME_MS / 1000)
                          * SAFETY_FACTOR;
// Result: (100 * 50 / 1000) * 3 = 15 minimum
// Use 1000 for additional burst capacity
```

## Common Patterns

### Pattern 1: One-Way Command Channel

```rust
pub struct CommandProcessor {
    command_tx: mpsc::Sender<Command>,
}

impl CommandProcessor {
    pub fn new(config: Arc<MpscChannelsConfig>) -> Self {
        let buffer_size = config.orchestration.command_processor.command_buffer_size;
        let (tx, rx) = mpsc::channel(buffer_size);

        tokio::spawn(Self::process_commands(rx));

        Self { command_tx: tx }
    }

    pub async fn send_command(&self, cmd: Command) -> Result<()> {
        self.command_tx.send(cmd).await
            .map_err(|_| Error::CommandChannelClosed)
    }
}
```

### Pattern 2: Request-Response Channel

```rust
pub struct RequestProcessor {
    request_tx: mpsc::Sender<(Request, oneshot::Sender<Response>)>,
}

impl RequestProcessor {
    pub async fn send_request(&self, req: Request) -> Result<Response> {
        let (response_tx, response_rx) = oneshot::channel();

        self.request_tx.send((req, response_tx)).await
            .map_err(|_| Error::RequestChannelClosed)?;

        response_rx.await
            .map_err(|_| Error::ResponseChannelClosed)
    }
}
```

### Pattern 3: Broadcast Channel

```rust
use tokio::sync::broadcast;

pub struct EventBroadcaster {
    tx: broadcast::Sender<Event>,
}

impl EventBroadcaster {
    pub fn new(config: Arc<MpscChannelsConfig>) -> Self {
        let buffer_size = config.shared.event_publisher.event_queue_buffer_size;
        let (tx, _rx) = broadcast::channel(buffer_size);

        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }

    pub fn publish(&self, event: Event) -> Result<usize> {
        self.tx.send(event)
            .map_err(|_| Error::NoBroadcastReceivers)
    }
}
```

## Anti-Patterns to Avoid

### ❌ Anti-Pattern 1: Unbounded Channels

```rust
// NEVER DO THIS
let (tx, rx) = mpsc::unbounded_channel();
```

**Why**: Risk of unbounded memory growth under load
**Instead**: Use bounded channel with appropriate buffer size

### ❌ Anti-Pattern 2: Hard-Coded Buffer Sizes

```rust
// NEVER DO THIS
let (tx, rx) = mpsc::channel(1000);  // Hard-coded!
```

**Why**: Cannot tune without code changes
**Instead**: Load from configuration

### ❌ Anti-Pattern 3: No Backpressure Handling

```rust
// NEVER DO THIS
async fn send(&self, msg: Message) {
    self.tx.send(msg).await.unwrap();  // Panic on error!
}
```

**Why**: Doesn't handle channel closed or saturation
**Instead**: Handle errors explicitly, log warnings

### ❌ Anti-Pattern 4: No Monitoring

```rust
// NEVER DO THIS - Creates channel without monitoring
let (tx, rx) = mpsc::channel(buffer_size);
// ... no metrics, no saturation checks
```

**Why**: Cannot diagnose production issues
**Instead**: Use ChannelMonitor for observability

### ❌ Anti-Pattern 5: Wrong Prefix in Environment Files

```toml
# NEVER DO THIS
[worker.step_processor]  # Missing mpsc_channels prefix!
execution_request_buffer_size = 100
```

**Why**: Creates conflicting top-level key, overrides correct config
**Instead**: Always use full `[mpsc_channels.*]` prefix

## Testing Guidelines

### Unit Tests - MUST Have

1. ✅ Configuration loading test
2. ✅ Backpressure behavior test
3. ✅ Environment override test
4. ✅ Error handling test

### Integration Tests - SHOULD Have

1. ✅ End-to-end message flow
2. ✅ Load test with small buffers
3. ✅ Saturation recovery test

### Test Configuration

Use test environment with small buffers to expose issues:

```toml
# config/tasker/environments/test/mpsc_channels.toml
[mpsc_channels.your_subsystem.your_component]
your_channel_buffer_size = 100  # Small to expose backpressure
```

## Code Review Checklist

When reviewing MPSC channel changes, verify:

- [ ] Channel is bounded (no `unbounded_channel()`)
- [ ] Buffer size comes from configuration (no hard-coded values)
- [ ] Configuration added to `mpsc_channels.toml` with comment
- [ ] Environment overrides use correct `[mpsc_channels.*]` prefix
- [ ] Rust config type added with proper struct
- [ ] Default implementation provided
- [ ] Monitoring integrated (ChannelMonitor)
- [ ] Backpressure strategy documented and implemented
- [ ] Tests cover backpressure behavior
- [ ] Error handling doesn't panic
- [ ] Documentation updated if needed

## Migration Checklist

When migrating existing unbounded/hard-coded channels:

- [ ] Add TOML configuration (base + environments)
- [ ] Add Rust config type
- [ ] Update channel creation to use config
- [ ] Add monitoring
- [ ] Implement backpressure handling
- [ ] Add tests
- [ ] Deploy to dev/staging first
- [ ] Monitor metrics for 24-48 hours
- [ ] Document any tuning needed

## Resources

- **ADR**: [Bounded MPSC Channels](../decisions/TAS-51-bounded-mpsc-channels.md)
- **Operations**: [MPSC Channel Tuning Runbook](../operations/mpsc-channel-tuning.md)
- **Specification**: [TAS-51](../ticket-specs/TAS-51.md)
- **Tokio Docs**: https://docs.rs/tokio/latest/tokio/sync/mpsc/

## Questions?

- **Slack**: `#platform-engineering`
- **Code Review**: Tag `@platform-team`
- **Design Review**: Required for new channel types
