# TAS-65: Domain Event Publication Architecture

**Status**: In Progress - TAS-69 Command Pattern Integration
**Created**: 2025-11-25
**Updated**: 2025-11-28
**Architecture Decision**: Post-Execution Publisher Callbacks with Command Pattern

## Implementation Status

### Completed Phases

**Phase 1: YAML Schema and Rust Types** ✅
- EventDeclaration types with PublicationCondition and EventDeliveryMode
- EventPublicationValidator with template-time validation
- Task template module restructuring

**Phase 2: Generic Event Publisher** ✅
- GenericEventPublisher with schema validation and condition checking
- DomainEventPayload structure with full execution context
- Integration with DomainEventPublisher (PGMQ-based)

**Phase 3: StepEventPublisher Trait and Registry** ✅
- `StepEventPublisher` trait for custom publishers
- `GenericStepEventPublisher` implementation
- `StepEventPublisherRegistry` for publisher lookup

**Phase 4: Dual-Path Event Delivery** ✅
- `InProcessEventBus` for fast in-memory events
- `EventRouter` for durable (PGMQ) vs fast (in-process) routing
- FFI bridge for Ruby to poll fast events
- Ruby `InProcessDomainEventPoller` class

**Phase 5: Extended Publication Conditions and Test Infrastructure** ✅
- Extended `PublicationCondition` with `RetryableFailure` and `PermanentFailure`
- Added `Broadcast` delivery mode (fast + durable)
- `DurableEventCapture` for PGMQ event inspection in tests
- `FastEventCapture` for in-memory event capture in integration tests
- Example event subscribers (`EventMetricsCollector`, logging subscribers)
- Failure path integration tests (5 test scenarios)
- See: `docs/ticket-specs/TAS-65/domain-event-subscribers.md`

**Phase 8: Worker Bootstrap Integration** ✅
- `InProcessEventBus` created with configurable buffer size
- `EventRouter` wired with `DomainEventPublisher` and `InProcessEventBus`
- `DomainEventSystem` created and spawned in `WorkerCore::start()`
- `processor.set_domain_event_handle(handle)` called during initialization
- Graceful shutdown with event draining in `WorkerCore::stop()`
- `in_process_bus` exposed publicly for Rust subscriber registration

### Remaining Phases (TAS-69)

The following phases complete the domain event architecture by wiring publishers into the workflow step lifecycle using the command pattern.

## Executive Summary

Domain events are published through **post-execution publisher callbacks** rather than in-handler method calls. Events are dispatched using a **command pattern with fire-and-forget semantics** - the workflow step lifecycle never waits for domain event publishing to complete.

### Core Principles

1. **Domain events describe completed state changes** - "payment was processed", not "processing payment"
2. **Publishing never blocks workflow steps** - fire-and-forget via `try_send()`
3. **Publishers are per-step, not per-event** - step receives full context to decide what events to publish
4. **Loud failures at initialization** - unregistered publishers fail at task init, not runtime
5. **Silent failures at runtime** - publishing errors are logged but never fail steps

## Architecture Overview

```
Step Handler executes
        ↓
State machine transition (command_processor.rs)
        ↓
Result sent to orchestration
        ↓
Domain Event Command dispatched (try_send - non-blocking)
        ↓ (async, fire-and-forget)
DomainEventCommandProcessor (background task)
        ↓
StepEventPublisherRegistry.get_or_default(step_name)
        ↓
StepEventPublisher.publish(ctx)
        ↓
EventRouter.route_event()
        ↓
    +--------+--------+
    |                 |
    v                 v
 Durable            Fast
    |                 |
    v                 v
  PGMQ        InProcessEventBus
                    ↓
             Ruby FFI Polling
                    ↓
             Ruby Subscribers
             (BaseSubscriber pattern)
```

## Implementation Phases (TAS-69)

### Phase 1: Remove `publish_event` from Ruby `StepHandler::Base`

**Goal**: Enforce post-execution publisher callback pattern.

**File**: `workers/ruby/lib/tasker_core/step_handler/base.rb`

**Changes**:
- Remove `publish_event` method
- Remove `extract_event_metadata` helper
- Update class documentation to reference publisher callbacks

### Phase 2: Create DomainEventSystem with Command Pattern

**Goal**: Create an event system for domain events with true fire-and-forget semantics.

**New Files**:
- `tasker-worker/src/worker/event_systems/domain_event_system.rs`
- `tasker-worker/src/worker/domain_event_commands.rs`

**Pattern**: Simplified fire-and-forget (NO oneshot response channels):
```rust
pub enum DomainEventCommand {
    /// Publish events for a completed step (fire-and-forget)
    PublishStepEvents(StepEventContext),

    /// Get statistics (uses oneshot since caller needs response)
    GetStatistics(oneshot::Sender<DomainEventSystemStatistics>),

    /// Graceful shutdown - drain fast events with timeout
    Shutdown {
        timeout_ms: u64,
        done: oneshot::Sender<ShutdownResult>,
    },
}
```

**Key Design Decision**: Use `try_send()` for fire-and-forget - no response channel for `PublishStepEvents`. This avoids the confusing pattern of creating a oneshot and immediately discarding it.

### Phase 3: Wire Domain Event Publishing into command_processor.rs

**Goal**: After step result is sent to orchestration, dispatch domain event command with zero blocking.

**File**: `tasker-worker/src/worker/command_processor.rs`

**Key Principle**: The command_processor should NOT know any specifics about domain event publishing. It simply dispatches to DomainEventSystem and moves on.

```rust
/// Dispatch domain events for completed step (fire-and-forget)
///
/// This is intentionally non-blocking. We don't want the orchestration
/// lifecycle to wait for domain event publishing, which may involve
/// arbitrary developer-space business logic in custom publishers.
fn dispatch_domain_events(
    &self,
    task_sequence_step: &TaskSequenceStep,
    step_result: &StepExecutionResult,
) {
    let Some(ref sender) = self.domain_event_sender else {
        return; // Domain events not configured
    };

    let ctx = StepEventContext::new(
        task_sequence_step.clone(),
        step_result.clone(),
    );

    // try_send is non-blocking - if channel is full, log and move on
    if let Err(e) = sender.try_send(DomainEventCommand::PublishStepEvents(ctx)) {
        warn!(
            step_uuid = %step_result.step_uuid,
            error = %e,
            "Domain event channel full or closed (fire-and-forget)"
        );
        metrics::counter!("domain_events_dropped_total", "reason" => "channel_full").increment(1);
    }
}
```

### Phase 4: Strategy Pattern for Publisher Selection

**Goal**: Define mechanism for selecting custom vs default domain event publishers.

**Key Clarification**: Publishers are **PER-STEP only**, not per-event. The step is the single articulation point for declaring a custom publisher.

**YAML Configuration**:
```yaml
steps:
  - name: process_payment
    # Publisher is at STEP level, not event level
    publisher: PaymentEventPublisher  # Custom publisher for this step
    publishes_events:
      - name: payment.processed
        delivery_mode: durable
      - name: payment.metrics
        delivery_mode: fast
      # Both events handled by PaymentEventPublisher
```

**LOUD FAILURE Requirement**: If YAML declares a `publisher` that isn't registered, **fail at task initialization**, not at runtime.

```rust
fn validate_step_publisher(step: &StepDefinition, registry: &StepEventPublisherRegistry) -> TaskerResult<()> {
    if let Some(ref publisher_name) = step.publisher {
        if !registry.contains(publisher_name) {
            return Err(TaskerError::configuration(format!(
                "Step '{}' declares publisher '{}' but it is not registered. \
                 Register it during bootstrap or remove the publisher declaration.",
                step.name, publisher_name
            )));
        }
    }
    Ok(())
}
```

### Phase 5: Rename GenericEventPublisher → DefaultDomainEventPublisher

**Goal**: Better naming to clarify fallback behavior.

**Files to Update**:
- `tasker-shared/src/events/generic_publisher.rs` → Rename struct
- `tasker-shared/src/events/mod.rs` - Update exports

**Note**: `GenericStepEventPublisher` in `step_event_publisher.rs` serves a different purpose (publishes ALL declared events from YAML) - keep that name.

### Phase 6: Ruby Publisher Base Class and Registry

**Goal**: Idiomatic Ruby base class and explicit registry for custom domain event publishers.

**New Files**:
- `workers/ruby/lib/tasker_core/domain_event_publisher/base.rb`
- `workers/ruby/lib/tasker_core/domain_event_publisher/registry.rb`

```ruby
# registry.rb
module TaskerCore
  module DomainEventPublisher
    class Registry
      include Singleton

      def self.register(name, klass)
        instance.register(name, klass)
      end

      def self.get(name)
        instance.get(name)
      end

      def register(name, klass)
        @mutex.synchronize { @publishers[name.to_s] = klass }
      end

      def get(name)
        @mutex.synchronize { @publishers[name.to_s]&.new }
      end
    end
  end
end

# base.rb
module TaskerCore
  module DomainEventPublisher
    class Base
      def publish(context)
        raise NotImplementedError
      end

      protected

      def publish_event(event_name, payload, delivery_mode: :durable)
        TaskerCore::FFI.publish_domain_event(
          event_name: event_name,
          payload: payload,
          metadata: build_metadata(context),
          delivery_mode: delivery_mode.to_s
        )
      rescue StandardError => e
        logger.warn "Failed to publish #{event_name}: #{e.message}"
        nil
      end
    end
  end
end
```

### Phase 7: Ruby Subscriber Base Class (BaseSubscriber Pattern)

**Goal**: Idiomatic Ruby subscriber pattern from legacy Rails engine.

**New File**: `workers/ruby/lib/tasker_core/domain_event_subscriber/base.rb`

```ruby
module TaskerCore
  module DomainEventSubscriber
    class Base
      class << self
        attr_reader :subscribed_patterns

        def subscribe_to(*patterns)
          @subscribed_patterns ||= []
          @subscribed_patterns.concat(patterns.map(&:to_s))
        end

        def register!
          instance = new
          poller = TaskerCore::Worker::InProcessDomainEventPoller.instance

          subscribed_patterns.each do |pattern|
            poller.subscribe(pattern) { |event| instance.handle_event(event) }
          end
        end
      end

      def handle_event(event)
        event_name = event[:event_name] || event['event_name']
        handler_method = method_for_event(event_name)

        if respond_to?(handler_method, true)
          send(handler_method, event)
        else
          handle_default(event)
        end
      rescue StandardError => e
        @logger.warn "Subscriber error for #{event_name}: #{e.message}"
      end

      private

      def method_for_event(event_name)
        "handle_#{event_name.gsub('.', '_').gsub('*', 'wildcard')}"
      end
    end
  end
end
```

**Example Usage**:
```ruby
class SentryErrorSubscriber < TaskerCore::DomainEventSubscriber::Base
  subscribe_to 'payment.failed', 'order.failed', '*.error'

  def handle_payment_failed(event)
    Sentry.capture_message("Payment failed: #{event[:metadata][:task_uuid]}")
  end

  def handle_wildcard_error(event)
    Sentry.capture_exception(build_exception(event))
  end
end

# In bootstrap
SentryErrorSubscriber.register!
```

### Phase 8: Worker Bootstrap Integration

**Goal**: Create and wire DomainEventSystem during worker bootstrap.

**Files**:
- `tasker-worker/src/worker/core.rs`
- `workers/ruby/ext/tasker_core/src/bootstrap.rs`
- `workers/ruby/lib/tasker_core/bootstrap.rb`

**Changes**:
1. Create `DomainEventSystem` with `StepEventPublisherRegistry` and `EventRouter`
2. Store command sender in `WorkerCommandProcessor`
3. Start domain event system in background
4. Ruby bootstrap registers custom publishers and subscribers

### Phase 9: Observability and Metrics

**Goal**: Production-grade observability for domain event publishing.

**OpenTelemetry Counters** (required):
```rust
// Successful publications
metrics::counter!("domain_events_published_total",
    "event_name" => event_name,
    "delivery_mode" => delivery_mode,
    "namespace" => namespace
).increment(1);

// Failed publications
metrics::counter!("domain_events_failed_total",
    "event_name" => event_name,
    "reason" => reason
).increment(1);

// Skipped (condition not met)
metrics::counter!("domain_events_skipped_total",
    "event_name" => event_name,
    "condition" => condition
).increment(1);

// Channel drops (backpressure)
metrics::counter!("domain_events_dropped_total",
    "reason" => "channel_full"
).increment(1);
```

### Phase 10: MPSC Channel Configuration (TAS-51 Compliance)

**Goal**: Configurable channel sizes per TAS-51 guidelines.

**Configuration**:
```toml
[mpsc_channels.domain_events]
command_buffer_size = 1000
backpressure_strategy = "drop"  # fire-and-forget semantics
```

**Environment Overrides**:
- Test: `command_buffer_size = 100` (small for testing backpressure)
- Production: `command_buffer_size = 5000` (large for throughput)

### Phase 11: Test Helpers

**Goal**: Enable integration tests to verify events were published.

**Rust Test Helpers**:
```rust
pub struct DomainEventTestCollector {
    // Subscribe to InProcessEventBus and collect all events
}

impl DomainEventTestCollector {
    pub fn new(bus: &InProcessEventBus) -> Self { ... }
    pub async fn flush(&self, timeout: Duration) -> Result<(), TimeoutError> { ... }
    pub fn events(&self) -> Vec<DomainEvent> { ... }
    pub fn assert_event_published(&self, event_name: &str) { ... }
}
```

**Ruby Test Helpers**:
```ruby
module DomainEventHelpers
  def wait_for_domain_events(timeout: 5.seconds)
    TaskerCore::FFI.flush_domain_events(timeout * 1000)
  end

  def assert_event_published(event_name)
    expect(captured_domain_events.map { |e| e[:event_name] }).to include(event_name)
  end
end
```

### Phase 12: Shutdown Handling

**Goal**: Graceful shutdown that handles in-flight events appropriately.

**Shutdown Behavior**:
- **Durable events**: Already persisted to PGMQ - no action needed
- **Fast events**: Drain from in-process queue with configurable timeout

```rust
pub enum ShutdownResult {
    Drained { events_processed: usize },
    Timeout { events_remaining: usize },
    ChannelClosed,
}
```

**Configuration**:
```toml
[domain_events.shutdown]
drain_timeout_ms = 5000
```

### Phase 13: Integration Tests

**Test Scenarios**:
1. Step completes → domain events published via command pattern
2. Custom publisher registered → uses custom publisher
3. No publisher specified → uses GenericStepEventPublisher
4. Durable events → PGMQ
5. Fast events → InProcessEventBus → Ruby subscribers
6. Publisher failure → logged, step still succeeds
7. Multiple events per step → all published or skipped by condition
8. Unregistered publisher in YAML → fails at task init (loud failure)
9. Channel full → event dropped with metric
10. Graceful shutdown → fast events drained

### Phase 14: Documentation

**Files to Create/Update**:
- `docs/domain-events.md` - User guide for publishers and subscribers
- Code documentation in all new files

## Critical Files to Modify

### Rust (tasker-worker)
1. `src/worker/command_processor.rs` - Add domain event dispatch
2. `src/worker/core.rs` - Initialize DomainEventSystem
3. `src/worker/event_systems/domain_event_system.rs` - **NEW**
4. `src/worker/domain_event_commands.rs` - **NEW**
5. `src/worker/test_helpers/domain_event_test_helpers.rs` - **NEW**

### Rust (tasker-shared)
1. `src/events/generic_publisher.rs` - Rename to DefaultDomainEventPublisher
2. `src/config/mpsc_channels.rs` - Add domain_events channel config

### Rust (workers/ruby/ext)
1. `src/bootstrap.rs` - Wire DomainEventSystem
2. `src/bridge.rs` - Add domain event system handle
3. `src/lib.rs` - Export flush_domain_events FFI

### Ruby
1. `lib/tasker_core/step_handler/base.rb` - Remove publish_event
2. `lib/tasker_core/domain_event_publisher/base.rb` - **NEW**
3. `lib/tasker_core/domain_event_publisher/registry.rb` - **NEW**
4. `lib/tasker_core/domain_event_subscriber/base.rb` - **NEW**
5. `lib/tasker_core/bootstrap.rb` - Register publishers and subscribers

### Configuration
1. `config/tasker/base/mpsc_channels.toml` - Add domain_events section
2. `config/tasker/environments/*/mpsc_channels.toml` - Environment overrides
3. `config/tasker/base/domain_events.toml` - Shutdown config

## Design Decisions

### Q1: DomainEventSystem separate from WorkerEventSystem?

**Decision**: Separate system. Domain events have fire-and-forget semantics, worker queue events must be processed reliably. Mixing them could lead to subtle bugs.

### Q2: How do custom Ruby publishers get registered?

**Decision**: Explicit registry configuration. Publishers are explicitly registered in bootstrap code. More verbose but explicit and testable.

### Q3: Should Ruby subscribers be auto-discovered?

**Decision**: Explicit registration. Each subscriber class calls `ClassName.register!` during bootstrap. More explicit and testable.

### Q4: Per-step or per-event publishers?

**Decision**: Per-step only. The step receives full context (task, step, result) and decides what events to publish. This keeps the YAML schema simple and aligns with the post-execution callback pattern.

## Success Criteria

1. ✅ `publish_event` removed from `StepHandler::Base`
2. ✅ Domain events published after step completion without blocking (`try_send`)
3. ✅ Custom publishers selectable via YAML configuration (per-step)
4. ✅ Ruby subscribers receive fast-path events via polling
5. ✅ All existing tests pass
6. ✅ New integration tests cover full lifecycle
7. ✅ Event publishing failures never fail workflow steps
8. ✅ Unregistered publishers fail loudly at task initialization
9. ✅ OpenTelemetry metrics for published/failed/skipped/dropped events
10. ✅ MPSC channel configuration per TAS-51 guidelines
11. ✅ Test helpers for verifying event publication
12. ✅ Graceful shutdown drains fast events with configurable timeout

## Summary

This architecture provides:

1. **Zero-blocking event dispatch**: `try_send()` ensures workflow steps never wait
2. **Clear separation of concerns**: Handlers contain business logic, publishers handle events
3. **Configuration-as-contract**: YAML declarations enforced with validation
4. **Structural safety**: Events published after persistence, can't fail steps
5. **Production observability**: OpenTelemetry metrics for all event outcomes
6. **Graceful degradation**: Channel backpressure handled with metrics, not failures
7. **Developer ergonomics**: Simple Ruby base classes for publishers and subscribers
