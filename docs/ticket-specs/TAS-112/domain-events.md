# TAS-112 Phase 7: Domain Events Integration Analysis

**Phase**: 7 of 9  
**Created**: 2025-12-27  
**Status**: Analysis Complete  
**Prerequisites**: Phase 1 (Documentation Analysis), TAS-65 (Domain Events Implementation)

---

## Executive Summary

This analysis examines **domain event publishing integration** - how step handlers publish business events for external system integration and internal subscribers. Domain events differ from system events: they communicate business outcomes (payment processed, order fulfilled) rather than internal workflow coordination.

**Key Findings**:
1. **✅ Ruby Complete** - Full publisher/subscriber pattern with lifecycle hooks, FFI channel integration
2. **⚠️ Python Partial** - Has `BasePublisher`/`BaseSubscriber` abstractions and poller, but no FFI integration examples
3. **❌ TypeScript Missing** - No domain event support (only mentioned in bootstrap comments)
4. **✅ Rust Complete** - Orchestration publishes events post-execution (TAS-65 complete), in-process bus for subscribers

**Architecture Discovery**: Domain events are **not handler features** - they are **orchestration features**! Rust orchestration automatically publishes declared events after step completion. Workers only define **custom publishers** (for payload transformation) and **subscribers** (for fast events).

**Guiding Principle** (Zen of Python): *"Explicit is better than implicit."*

**Project Context**: Pre-alpha greenfield. Breaking changes welcome to achieve correct architecture.

---

## Domain Events Architecture Overview

### Two Distinct Event Types

| Aspect | System Events | Domain Events |
|--------|---------------|---------------|
| **Purpose** | Internal coordination | Business observability |
| **Producers** | Orchestration (Rust only) | Step handlers via declarations |
| **Consumers** | Event processors, command handlers | External systems, analytics, internal subscribers |
| **Delivery** | PGMQ + LISTEN/NOTIFY | Durable (PGMQ) or Fast (in-process) |
| **Semantics** | At-least-once | Fire-and-forget (best effort) |
| **Publishing** | Automatic | Declarative (YAML) with optional custom publishers |

**Critical Distinction**: Domain events are declared in YAML `publishes_events` field. Rust orchestration publishes them **after** successful orchestration notification. Handlers DO NOT call "publish_event()" - they return results that trigger declarative events.

---

## Publishing Architecture

### YAML Event Declaration (Cross-Language)

Events are **declared** in task template YAML at the step level:

```yaml
# config/tasks/payments/credit_card_payment/1.0.0.yaml
steps:
  - name: process_payment
    handler:
      callable: PaymentProcessing::StepHandler::ProcessPaymentHandler
    
    publishes_events:
      - name: payment.processed
        description: "Payment successfully processed"
        condition: success  # success, failure, retryable_failure, permanent_failure, always
        delivery_mode: durable  # durable, fast, or broadcast
        publisher: DomainEvents::Publishers::PaymentEventPublisher  # optional custom publisher
        schema:
          type: object
          required: [transaction_id, amount]
          properties:
            transaction_id: { type: string }
            amount: { type: number }
      
      - name: payment.failed
        description: "Payment processing failed permanently"
        condition: permanent_failure
        delivery_mode: durable
        publisher: DomainEvents::Publishers::PaymentEventPublisher
```

**Publication Conditions**:
- `success`: Publish only when step completes successfully
- `failure`: Publish on any step failure (retryable or permanent)
- `retryable_failure`: Publish only on retryable failures
- `permanent_failure`: Publish only on permanent failures (exhausted retries or non-retryable)
- `always`: Publish regardless of step outcome

**Delivery Modes**:
- `durable`: Published to PGMQ for external consumers (default)
- `fast`: Published to in-process bus for internal subscribers only
- `broadcast`: Published to both durable queue AND in-process bus

### Publishing Flow (Rust Orchestration)

```
┌─────────────────────────────────────────────────────────────────────┐
│ Step Handler (Ruby/Python/TypeScript/Rust)                         │
│ Returns: StepExecutionResult                                        │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────────┐
│ Worker Command Processor (Rust)                                     │
│ 1. Send result to orchestration (PGMQ)                             │
│ 2. IF SUCCESS → Dispatch domain events (fire-and-forget)           │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────────┐
│ Domain Event Command Loop (Rust)                                    │
│ Reads cached TaskSequenceStep + StepExecutionResult                 │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────────┐
│ Event Declaration Processing                                        │
│ - Filter by condition (success/failure/always)                     │
│ - Lookup custom publisher (if specified)                           │
│ - Transform payload (custom publisher or default)                  │
│ - Build EventMetadata (correlation_id, task_uuid, etc.)            │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────────────┐
│ EventRouter (Rust)                                                   │
│ Route based on delivery_mode:                                       │
│ - durable → DomainEventPublisher (PGMQ)                            │
│ - fast → InProcessEventBus (broadcast channel)                     │
│ - broadcast → Both paths concurrently                              │
└──────────────────────────┬──────────────────────────────────────────┘
                           │
                           ▼
         ┌─────────────────┴────────────────┐
         │                                  │
         ▼                                  ▼
┌──────────────────┐              ┌──────────────────┐
│ PGMQ Queue       │              │ InProcessEventBus│
│ (External)       │              │ (Internal)       │
└──────────────────┘              └──────────────────┘
         │                                  │
         ▼                                  ▼
┌──────────────────┐              ┌──────────────────┐
│ External         │              │ Internal         │
│ Consumers        │              │ Subscribers      │
└──────────────────┘              └──────────────────┘
```

**Key Design Decisions** (from `tasker-worker/src/worker/command_processor.rs:512-525`):
1. **Events only after orchestration success**: Domain events are declarative of what HAS happened. If orchestration notification fails, step isn't truly complete.
2. **Fire-and-forget via `try_send`**: Never blocks worker command loop. If channel full, events dropped and logged.
3. **Context caching**: Step execution context cached when step claimed, retrieved for event building after completion.

---

## Custom Publishers (Cross-Language Pattern)

Custom publishers provide **payload transformation** and **conditional publishing** beyond YAML declarations. They are NOT responsible for calling publish APIs - they transform data that Rust orchestration publishes.

### Ruby: BasePublisher

**Location**: `workers/ruby/lib/tasker_core/domain_events/base_publisher.rb`

```ruby
module TaskerCore
  module DomainEvents
    class BasePublisher
      # Required: Publisher name for YAML declaration
      def name
        raise NotImplementedError
      end
      
      # Optional: Transform step result into event payload
      def transform_payload(step_result, event_declaration, step_context = nil)
        step_result[:result] || {}
      end
      
      # Optional: Additional publishing condition beyond YAML
      def should_publish?(step_result, event_declaration, step_context = nil)
        true
      end
      
      # Optional: Add metadata to event
      def additional_metadata(step_result, event_declaration, step_context = nil)
        {}
      end
      
      # Lifecycle hooks
      def before_publish(event_name, payload, metadata)
        logger.debug "Publishing event: #{event_name}"
      end
      
      def after_publish(event_name, payload, metadata)
        logger.debug "Event published: #{event_name}"
      end
      
      def on_publish_error(event_name, error, payload)
        logger.error "Failed to publish event #{event_name}: #{error.message}"
      end
      
      # TAS-96: Cross-language standard API
      def publish(ctx)
        event_name = ctx[:event_name]
        step_result = ctx[:step_result]
        event_declaration = ctx[:event_declaration] || {}
        step_context = ctx[:step_context]
        
        return false unless should_publish?(step_result, event_declaration, step_context)
        
        payload = transform_payload(step_result, event_declaration, step_context)
        metadata = build_metadata(step_result, event_declaration, step_context)
        
        begin
          before_publish(event_name, payload, metadata)
          # Actual publishing handled by Rust orchestration
          after_publish(event_name, payload, metadata)
          true
        rescue StandardError => e
          on_publish_error(event_name, e, payload)
          false
        end
      end
    end
  end
end
```

**Real Example**: `workers/ruby/spec/handlers/examples/domain_events/publishers/payment_event_publisher.rb`

```ruby
class PaymentEventPublisher < TaskerCore::DomainEvents::BasePublisher
  def name
    'DomainEvents::Publishers::PaymentEventPublisher'
  end
  
  def transform_payload(step_result, event_declaration, step_context = nil)
    result = step_result[:result] || {}
    event_name = event_declaration[:name]
    
    if step_result[:success] && event_name&.include?('processed')
      {
        transaction_id: result[:transaction_id],
        amount: result[:amount],
        currency: result[:currency] || 'USD',
        payment_method: result[:payment_method] || 'credit_card',
        processed_at: result[:processed_at] || Time.now.iso8601,
        delivery_mode: 'durable',
        publisher: name
      }
    elsif !step_result[:success] && event_name&.include?('failed')
      metadata = step_result[:metadata] || {}
      {
        error_code: metadata[:error_code],
        error_message: metadata[:error_message],
        failed_at: Time.now.iso8601
      }
    else
      result
    end
  end
  
  def should_publish?(step_result, event_declaration, step_context = nil)
    result = step_result[:result] || {}
    event_name = event_declaration[:name]
    
    # Only publish success events if we have transaction data
    if event_name&.include?('processed')
      return step_result[:success] && result[:transaction_id].present?
    end
    
    # Only publish failure events if we have error info
    if event_name&.include?('failed')
      metadata = step_result[:metadata] || {}
      return !step_result[:success] && metadata[:error_code].present?
    end
    
    true
  end
  
  def additional_metadata(step_result, event_declaration, step_context = nil)
    metadata = step_result[:metadata] || {}
    {
      execution_time_ms: metadata[:execution_time_ms],
      publisher_type: 'custom',
      publisher_name: name,
      payment_provider: metadata[:payment_provider]
    }
  end
end
```

**Registration**:
```ruby
# In bootstrap
TaskerCore::DomainEvents::PublisherRegistry.instance.register(
  DomainEvents::Publishers::PaymentEventPublisher.new
)
```

**Analysis**:
- ✅ **Complete lifecycle hooks**: `before_publish`, `after_publish`, `on_publish_error`
- ✅ **Flexible transformation**: Can customize based on success/failure
- ✅ **Conditional publishing**: Beyond YAML condition
- ✅ **TAS-96 unified API**: `publish(ctx)` matches cross-language pattern

### Python: BasePublisher

**Location**: `workers/python/python/tasker_core/domain_events.py`

```python
from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import Any

@dataclass
class StepEventContext:
    """Context for publishing step events."""
    task_uuid: str
    step_uuid: str
    step_name: str
    namespace: str
    correlation_id: str
    result: dict[str, Any] | None = None
    metadata: dict[str, Any] = field(default_factory=dict)
    
    @classmethod
    def from_step_context(cls, step_context, result=None, metadata=None):
        return cls(
            task_uuid=str(step_context.task_uuid),
            step_uuid=str(step_context.step_uuid),
            step_name=step_context.handler_name,
            namespace=step_context.event.task_sequence_step
                .get("task", {}).get("task", {}).get("namespace", "default"),
            correlation_id=str(step_context.correlation_id),
            result=result,
            metadata=metadata or {},
        )


class BasePublisher(ABC):
    """Abstract base class for step event publishers.
    
    Publishers emit domain events after step completion to external
    systems like Kafka, webhooks, or message queues.
    """
    
    @abstractmethod
    def name(self) -> str:
        """Return the publisher name."""
        ...
    
    @abstractmethod
    def publish(self, ctx: StepEventContext) -> None:
        """Publish an event with the given context.
        
        This method is called after a step completes to publish
        the event to external systems.
        """
        ...
    
    def should_publish(self, ctx: StepEventContext) -> bool:
        """Determine whether to publish for this context.
        
        Override to conditionally publish based on step name,
        namespace, or other context attributes.
        
        Returns:
            True if the event should be published (default: True).
        """
        return True
    
    def transform_payload(self, ctx: StepEventContext) -> dict[str, Any]:
        """Transform the event payload before publishing.
        
        Override to customize payload structure for external system.
        """
        return {
            "task_uuid": ctx.task_uuid,
            "step_uuid": ctx.step_uuid,
            "step_name": ctx.step_name,
            "namespace": ctx.namespace,
            "correlation_id": ctx.correlation_id,
            "result": ctx.result,
            "metadata": ctx.metadata,
        }
```

**Analysis**:
- ✅ **Has StepEventContext**: Matches Ruby's ctx pattern
- ✅ **Has transform_payload**: Payload customization
- ✅ **Has should_publish**: Conditional publishing
- ❌ **No lifecycle hooks**: Missing `before_publish`, `after_publish`, `on_error`
- ❌ **No additional_metadata**: Can't add custom metadata
- ⚠️ **Abstract publish()**: Requires subclasses to implement, but Rust orchestration actually publishes

### TypeScript: Missing

**Status**: ❌ **No domain event support**

**Evidence**:
- No `domain-events.ts` file in `workers/typescript/src/handler/`
- `bootstrap.ts` line 29 mentions "Subscribing to domain events" but no implementation
- No publisher or subscriber abstractions

**Gap Assessment**:
- TypeScript workers cannot define custom publishers
- TypeScript workers cannot subscribe to fast events
- TypeScript steps can still publish via YAML declarations (Rust handles it), but can't customize payloads

### Rust: No Custom Publisher API (Orchestration Only)

**Status**: ✅ **Complete orchestration, no handler-level API**

Rust does NOT have a custom publisher trait for step handlers. This makes sense because:
1. Rust handlers could implement publisher logic inline if needed
2. Most payload transformation can be done in the handler's result
3. Ruby/Python custom publishers exist to bridge FFI gap

**GenericEventPublisher** (`tasker-shared/src/events/generic_publisher.rs`):
```rust
pub struct GenericEventPublisher {
    domain_publisher: Arc<DomainEventPublisher>,
}

impl GenericEventPublisher {
    pub async fn publish(
        &self,
        event_decl: &EventDeclaration,
        task_sequence_step: TaskSequenceStep,
        execution_result: StepExecutionResult,
        business_payload: Value,
        metadata: EventMetadata,
    ) -> Result<Option<Uuid>, GenericPublisherError> {
        // Check condition
        let step_succeeded = execution_result.success;
        if !event_decl.should_publish(step_succeeded) {
            return Ok(None);
        }
        
        // Validate payload against schema
        event_decl.validate_payload(&business_payload)?;
        
        // Create full domain event payload
        let domain_payload = DomainEventPayload {
            task_sequence_step,
            execution_result,
            payload: business_payload,
        };
        
        // Publish via domain publisher
        let event_id = self.domain_publisher
            .publish_event(&event_decl.name, domain_payload, metadata)
            .await?;
        
        Ok(Some(event_id))
    }
}
```

**Analysis**:
- ✅ **Orchestration-level**: Used by command processor, not by handlers
- ✅ **Schema validation**: Validates business payload against JSON schema
- ✅ **Condition checking**: Honors YAML condition field
- ❌ **No custom publisher support**: No trait for Ruby-style custom publishers

**Cross-Language Comparison**:

| Feature | Rust | Ruby | Python | TypeScript |
|---------|------|------|--------|------------|
| **Custom Publisher API** | ❌ (orchestration only) | ✅ `BasePublisher` | ✅ `BasePublisher` | ❌ Missing |
| **Lifecycle Hooks** | N/A | ✅ Full (before/after/error) | ❌ Missing | N/A |
| **Payload Transform** | N/A | ✅ `transform_payload` | ✅ `transform_payload` | N/A |
| **Conditional Publish** | N/A | ✅ `should_publish?` | ✅ `should_publish` | N/A |
| **Additional Metadata** | N/A | ✅ `additional_metadata` | ❌ Missing | N/A |
| **Registry** | ✅ (built into orchestration) | ✅ `PublisherRegistry` | ⚠️ (implied, not shown) | N/A |

---

## Subscriber Patterns (Fast/In-Process Events Only)

Subscribers handle **fast (in-process) domain events** for real-time internal processing. They DO NOT handle durable events (those go to external consumers via PGMQ).

**Key Insight**: Subscribers are for internal Tasker use only - metrics, logging, secondary actions that aren't part of the Task → WorkflowStep DAG hierarchy.

### Rust: EventHandler + InProcessEventBus

**Location**: `workers/rust/src/event_subscribers/`

```rust
use std::sync::Arc;
use tasker_shared::events::registry::EventHandler;
use tracing::info;

/// Create a logging subscriber for all events matching a pattern
pub fn create_logging_subscriber(prefix: &str) -> EventHandler {
    let prefix = prefix.to_string();
    
    Arc::new(move |event| {
        let prefix = prefix.clone();
        
        Box::pin(async move {
            let step_name = event.metadata.step_name.as_deref().unwrap_or("unknown");
            
            info!(
                prefix = %prefix,
                event_name = %event.event_name,
                event_id = %event.event_id,
                task_uuid = %event.metadata.task_uuid,
                step_name = %step_name,
                namespace = %event.metadata.namespace,
                correlation_id = %event.metadata.correlation_id,
                fired_at = %event.metadata.fired_at,
                "Domain event received"
            );
            
            Ok(())
        })
    })
}
```

**Metrics Subscriber** (`workers/rust/src/event_subscribers/metrics_subscriber.rs`):
```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

pub struct EventMetricsCollector {
    events_received: AtomicU64,
    success_events: AtomicU64,
    failure_events: AtomicU64,
}

impl EventMetricsCollector {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            events_received: AtomicU64::new(0),
            success_events: AtomicU64::new(0),
            failure_events: AtomicU64::new(0),
        })
    }
    
    pub fn create_handler(self: &Arc<Self>) -> EventHandler {
        let metrics = Arc::clone(self);
        
        Arc::new(move |event| {
            let metrics = Arc::clone(&metrics);
            
            Box::pin(async move {
                metrics.events_received.fetch_add(1, Ordering::Relaxed);
                
                if event.payload.execution_result.success {
                    metrics.success_events.fetch_add(1, Ordering::Relaxed);
                } else {
                    metrics.failure_events.fetch_add(1, Ordering::Relaxed);
                }
                
                Ok(())
            })
        })
    }
    
    pub fn events_received(&self) -> u64 {
        self.events_received.load(Ordering::Relaxed)
    }
}
```

**Registration**:
```rust
use tasker_worker::worker::in_process_event_bus::InProcessEventBus;

let mut bus = InProcessEventBus::new(config);

// Subscribe to all events
bus.subscribe("*", create_logging_subscriber("[ALL]")).unwrap();

// Subscribe to specific patterns
bus.subscribe("payment.*", create_logging_subscriber("[PAYMENT]")).unwrap();

// Use metrics collector
let metrics = EventMetricsCollector::new();
bus.subscribe("*", metrics.create_handler()).unwrap();
```

**Analysis**:
- ✅ **Functional pattern**: Uses `Arc<dyn Fn(DomainEvent) -> Future>` (EventHandler type)
- ✅ **Pattern matching**: Supports wildcard patterns (`*`, `payment.*`)
- ✅ **Thread-safe**: Uses `AtomicU64` for metrics
- ✅ **Async**: Handlers return `Pin<Box<dyn Future>>`
- ✅ **Error handling**: Returns `Result<(), Error>`

### Ruby: BaseSubscriber + InProcessDomainEventPoller

**Location**: `workers/ruby/lib/tasker_core/domain_events/base_subscriber.rb`

```ruby
module TaskerCore
  module DomainEvents
    class BaseSubscriber
      class << self
        # DSL for declaring event patterns
        def subscribes_to(*patterns)
          @patterns = patterns.flatten
        end
        
        def patterns
          @patterns || ['*']
        end
      end
      
      attr_reader :logger, :active
      
      def initialize
        @logger = TaskerCore::Logger.instance
        @active = false
        @subscriptions = []
      end
      
      # Start listening for events
      def start!
        return if @active
        
        @active = true
        poller = TaskerCore::Worker::InProcessDomainEventPoller.instance
        
        self.class.patterns.each do |pattern|
          poller.subscribe(pattern) do |event|
            handle_event_safely(event)
          end
          @subscriptions << pattern
          logger.info "#{self.class.name}: Subscribed to pattern '#{pattern}'"
        end
      end
      
      # Stop listening
      def stop!
        return unless @active
        
        @active = false
        poller = TaskerCore::Worker::InProcessDomainEventPoller.instance
        @subscriptions.each { |pattern| poller.unsubscribe(pattern) }
        @subscriptions.clear
      end
      
      # Handle a domain event (MUST implement)
      def handle(event)
        raise NotImplementedError
      end
      
      # Lifecycle hooks
      def before_handle(event)
        true  # Return false to skip
      end
      
      def after_handle(event)
        # Default: no-op
      end
      
      def on_handle_error(event, error)
        logger.error "Failed to handle #{event[:event_name]}: #{error.message}"
      end
      
      private
      
      def handle_event_safely(event)
        return unless @active
        return unless before_handle(event)
        
        handle(event)
        after_handle(event)
      rescue StandardError => e
        on_handle_error(event, e)
      end
    end
  end
end
```

**Real Example**: `workers/ruby/spec/handlers/examples/domain_events/subscribers/metrics_subscriber.rb`

```ruby
module DomainEvents
  module Subscribers
    class MetricsSubscriber < TaskerCore::DomainEvents::BaseSubscriber
      subscribes_to '*'
      
      class << self
        attr_accessor :events_received, :success_events, :failure_events,
                      :events_by_namespace, :last_event_at
        
        def reset_counters!
          @mutex = Mutex.new
          @events_received = 0
          @success_events = 0
          @failure_events = 0
          @events_by_namespace = Hash.new(0)
          @last_event_at = nil
        end
        
        def increment(counter)
          @mutex.synchronize { instance_variable_set("@#{counter}", send(counter) + 1) }
        end
        
        def increment_hash(hash_name, key)
          @mutex.synchronize { send(hash_name)[key] += 1 }
        end
      end
      
      reset_counters!
      
      def handle(event)
        event_name = event[:event_name]
        metadata = event[:metadata] || {}
        execution_result = event[:execution_result] || {}
        
        self.class.increment(:events_received)
        
        if execution_result[:success]
          self.class.increment(:success_events)
        else
          self.class.increment(:failure_events)
        end
        
        namespace = metadata[:namespace] || 'unknown'
        self.class.increment_hash(:events_by_namespace, namespace)
        self.class.set(:last_event_at, Time.now)
      end
    end
  end
end
```

**Registration**:
```ruby
# In bootstrap
registry = TaskerCore::DomainEvents::SubscriberRegistry.instance
registry.register(DomainEvents::Subscribers::LoggingSubscriber)
registry.register(DomainEvents::Subscribers::MetricsSubscriber)
registry.start_all!

# Query metrics
puts "Total: #{MetricsSubscriber.events_received}"
puts "By namespace: #{MetricsSubscriber.events_by_namespace}"
```

**Analysis**:
- ✅ **Class-level DSL**: `subscribes_to` pattern declaration
- ✅ **Lifecycle management**: `start!`, `stop!`, `active?`
- ✅ **Lifecycle hooks**: `before_handle`, `after_handle`, `on_handle_error`
- ✅ **Pattern matching**: Via poller's pattern matching
- ✅ **Registry pattern**: Centralized subscriber management
- ✅ **FFI integration**: Poller reads from Rust broadcast channel

### Python: BaseSubscriber + InProcessDomainEventPoller

**Location**: `workers/python/python/tasker_core/domain_events.py`

```python
class BaseSubscriber(ABC):
    """Abstract base class for domain event subscribers.
    
    Subscribers listen for domain events and handle them accordingly.
    They define which event patterns they subscribe to and how to handle
    incoming events.
    """
    
    @classmethod
    @abstractmethod
    def subscribes_to(cls) -> list[str]:
        """Return list of event patterns to subscribe to.
        
        Returns:
            List of event name patterns (e.g., ["order.completed", "payment.*"]).
        """
        ...
    
    @abstractmethod
    def handle(self, event: dict[str, Any]) -> None:
        """Handle the received event.
        
        Args:
            event: The event data dictionary containing event_name, payload, etc.
        """
        ...
    
    def matches(self, event_name: str) -> bool:
        """Check if this subscriber should handle the given event name.
        
        Supports wildcard matching with "*".
        """
        import fnmatch
        
        return any(fnmatch.fnmatch(event_name, pattern) for pattern in self.subscribes_to())


class InProcessDomainEventPoller:
    """Threaded poller for in-process domain events.
    
    The InProcessDomainEventPoller runs in a separate thread and polls for
    domain events from the Rust broadcast channel. Events are emitted to
    registered handlers via callbacks.
    
    Threading Model:
        - Main Thread: Python application, event handlers
        - Polling Thread: Dedicated background thread for event polling
        - Rust Threads: Rust worker runtime (separate from Python)
    
    Performance Characteristics:
        - Poll Interval: 10ms (configurable)
        - Max Latency: ~10ms from event generation to handler execution
        - CPU Usage: Minimal (yields during sleep)
        - Delivery: Best-effort (may miss events if lagged)
    """
    
    DEFAULT_POLL_INTERVAL_MS = 10
    DEFAULT_MAX_EVENTS_PER_POLL = 100
    
    def __init__(
        self,
        polling_interval_ms: int = DEFAULT_POLL_INTERVAL_MS,
        max_events_per_poll: int = DEFAULT_MAX_EVENTS_PER_POLL,
    ) -> None:
        self._polling_interval = polling_interval_ms / 1000.0
        self._max_events_per_poll = max_events_per_poll
        self._running = False
        self._thread: threading.Thread | None = None
        self._poll_count = 0
        self._events_processed = 0
        self._events_lagged = 0
        
        # Callbacks
        self._event_callbacks: list[DomainEventCallback] = []
        self._error_callbacks: list[ErrorCallback] = []
    
    def on_event(self, callback: DomainEventCallback) -> None:
        """Register a callback for domain events."""
        self._event_callbacks.append(callback)
    
    def on_error(self, callback: ErrorCallback) -> None:
        """Register a callback for errors."""
        self._error_callbacks.append(callback)
    
    def start(self) -> None:
        """Start the domain event polling thread."""
        if self._running:
            raise RuntimeError("InProcessDomainEventPoller already running")
        
        self._running = True
        self._thread = threading.Thread(
            target=self._poll_loop,
            name="tasker-domain-event-poller",
            daemon=True,
        )
        self._thread.start()
        
        log_info("InProcessDomainEventPoller started", 
                 {"interval_ms": str(int(self._polling_interval * 1000))})
    
    def stop(self, timeout: float = 5.0) -> None:
        """Stop the domain event polling thread."""
        if not self._running:
            return
        
        log_info("Stopping InProcessDomainEventPoller...")
        self._running = False
        
        if self._thread is not None:
            self._thread.join(timeout=timeout)
            self._thread = None
        
        log_info("InProcessDomainEventPoller stopped",
                 {"events_processed": str(self._events_processed),
                  "events_lagged": str(self._events_lagged)})
    
    @property
    def is_running(self) -> bool:
        """Check if the poller is currently running."""
        return self._running and self._thread is not None and self._thread.is_alive()
    
    @property
    def stats(self) -> dict[str, int]:
        """Get polling statistics."""
        return {
            "poll_count": self._poll_count,
            "events_processed": self._events_processed,
            "events_lagged": self._events_lagged,
        }
    
    def _poll_loop(self) -> None:
        """Main polling loop (runs in separate thread)."""
        log_debug("InProcessDomainEventPoller: Starting poll loop")
        
        while self._running:
            try:
                self._poll_count += 1
                events_this_poll = 0
                
                # Poll for events (up to max_events_per_poll)
                while events_this_poll < self._max_events_per_poll:
                    event_data = _poll_in_process_events()  # FFI call to Rust
                    
                    if event_data is None:
                        break  # No more events
                    
                    self._process_event(event_data)
                    events_this_poll += 1
                    self._events_processed += 1
                
                # Sleep if no events were found
                if events_this_poll == 0:
                    time.sleep(self._polling_interval)
            
            except Exception as e:
                self._emit_error(e)
                time.sleep(self._polling_interval * 10)  # Sleep longer on error
        
        log_debug("InProcessDomainEventPoller: Poll loop terminated")
    
    def _process_event(self, event_data: dict[str, Any]) -> None:
        """Process a polled event through callbacks."""
        try:
            event = InProcessDomainEvent.model_validate(event_data)
            log_debug("InProcessDomainEventPoller: Processing event",
                     {"event_id": str(event.event_id), "event_name": event.event_name})
            
            for callback in self._event_callbacks:
                try:
                    callback(event)
                except Exception as e:
                    log_error(f"InProcessDomainEventPoller: Callback error: {e}",
                             {"event_id": str(event.event_id)})
                    self._emit_error(e)
        
        except Exception as e:
            log_error(f"InProcessDomainEventPoller: Failed to process event: {e}")
            self._emit_error(e)
```

**Analysis**:
- ✅ **Has BaseSubscriber**: Abstract base with `subscribes_to()`, `handle()`
- ✅ **Has InProcessDomainEventPoller**: Threaded poller with FFI integration
- ✅ **Pattern matching**: Via `fnmatch` in `matches()` method
- ✅ **Stats tracking**: Poll count, events processed, lag detection
- ❌ **No lifecycle hooks**: Missing `before_handle`, `after_handle`, `on_error`
- ❌ **No registry pattern**: Subscribers self-register (not centralized)
- ⚠️ **Callback-based**: Uses callback registration instead of class-based subscription

### TypeScript: Missing

**Status**: ❌ **No subscriber support**

**Gap Assessment**:
- TypeScript cannot subscribe to fast events
- No in-process event bus integration
- No poller implementation

**Cross-Language Comparison**:

| Feature | Rust | Ruby | Python | TypeScript |
|---------|------|------|--------|------------|
| **Subscriber API** | ✅ `EventHandler` type | ✅ `BaseSubscriber` | ✅ `BaseSubscriber` | ❌ Missing |
| **Pattern Matching** | ✅ Wildcard support | ✅ DSL `subscribes_to` | ✅ `subscribes_to()` | N/A |
| **Lifecycle Hooks** | ✅ (via Result) | ✅ Full (before/after/error) | ❌ Missing | N/A |
| **Poller/Registry** | ✅ `InProcessEventBus` | ✅ `InProcessDomainEventPoller` + Registry | ✅ `InProcessDomainEventPoller` | ❌ Missing |
| **FFI Integration** | ✅ (native) | ✅ broadcast channel | ✅ `poll_in_process_events` FFI | N/A |
| **Thread-Safe** | ✅ `Arc`, `AtomicU64` | ✅ `Mutex` | ✅ Threading | N/A |
| **Start/Stop** | ✅ `subscribe`/`unsubscribe` | ✅ `start!`/`stop!` | ✅ `start()`/`stop()` | N/A |

---

## Functional Gaps Summary

### Rust (Complete Orchestration)
1. ✅ **GenericEventPublisher** - Full schema validation and condition checking
2. ✅ **DomainEventPublisher** - PGMQ publishing with correlation IDs
3. ✅ **InProcessEventBus** - Fast event delivery to internal subscribers
4. ✅ **EventRouter** - Dual-path routing (durable vs fast vs broadcast)
5. ✅ **Post-execution publishing** - TAS-65 complete (events after orchestration notification)
6. ❌ **No custom publisher trait** - Not needed (handlers own transformation)

### Ruby (Most Complete Worker Support)
1. ✅ **BasePublisher** - Full lifecycle hooks, payload transformation, conditional publishing
2. ✅ **BaseSubscriber** - Pattern-based subscription with lifecycle management
3. ✅ **PublisherRegistry** - Centralized publisher management
4. ✅ **SubscriberRegistry** - Centralized subscriber management
5. ✅ **InProcessDomainEventPoller** - FFI integration with Rust broadcast channel
6. ✅ **Lifecycle hooks** - Before/after/error for both publishers and subscribers

### Python (Good Core, Missing Hooks)
1. ✅ **BasePublisher** - Has `publish()`, `transform_payload()`, `should_publish()`
2. ✅ **BaseSubscriber** - Has `subscribes_to()`, `handle()`, `matches()`
3. ✅ **InProcessDomainEventPoller** - Threaded poller with FFI integration
4. ✅ **StepEventContext** - Matches Ruby's context pattern
5. ❌ **No lifecycle hooks** - Missing before/after/error callbacks for both publishers and subscribers
6. ❌ **No additional_metadata** - Can't add custom metadata to events
7. ⚠️ **No registry** - Subscribers self-register via callbacks (less structured)

### TypeScript (Completely Missing)
1. ❌ **No BasePublisher** - Cannot define custom publishers
2. ❌ **No BaseSubscriber** - Cannot subscribe to fast events
3. ❌ **No InProcessEventPoller** - No FFI integration
4. ❌ **No domain event support** - Completely absent
5. ⚠️ **Can still publish via YAML** - Rust orchestration handles it, but can't customize payloads

---

## Recommendations Summary

### Critical Changes (Implement Now)

#### 1. Python Lifecycle Hooks

**Add to BasePublisher**:
```python
class BasePublisher(ABC):
    # ... existing methods ...
    
    def before_publish(self, event_name: str, payload: dict, metadata: dict) -> bool:
        """Hook called before publishing.
        
        Override for pre-publish validation or filtering.
        Return False to skip publishing.
        """
        return True
    
    def after_publish(self, event_name: str, payload: dict, metadata: dict) -> None:
        """Hook called after successful publishing."""
        pass
    
    def on_publish_error(self, event_name: str, error: Exception, payload: dict) -> None:
        """Hook called if publishing fails."""
        log_error(f"Failed to publish {event_name}: {error}")
    
    def additional_metadata(self, ctx: StepEventContext) -> dict[str, Any]:
        """Add custom metadata to the event."""
        return {}
```

**Add to BaseSubscriber**:
```python
class BaseSubscriber(ABC):
    # ... existing methods ...
    
    def before_handle(self, event: dict[str, Any]) -> bool:
        """Hook called before handling an event.
        
        Return False to skip handling this event.
        """
        return True
    
    def after_handle(self, event: dict[str, Any]) -> None:
        """Hook called after successful handling."""
        pass
    
    def on_handle_error(self, event: dict[str, Any], error: Exception) -> None:
        """Hook called if handling fails."""
        log_error(f"Failed to handle {event.get('event_name')}: {error}")
```

#### 2. TypeScript Domain Events Complete Implementation

**Create `workers/typescript/src/handler/domain-events.ts`**:
```typescript
export interface StepEventContext {
  taskUuid: string;
  stepUuid: string;
  stepName: string;
  namespace: string;
  correlationId: string;
  result?: Record<string, unknown>;
  metadata: Record<string, unknown>;
}

export abstract class BasePublisher {
  abstract name(): string;
  
  abstract publish(ctx: StepEventContext): Promise<void>;
  
  shouldPublish(ctx: StepEventContext): boolean {
    return true;
  }
  
  transformPayload(ctx: StepEventContext): Record<string, unknown> {
    return {
      taskUuid: ctx.taskUuid,
      stepUuid: ctx.stepUuid,
      stepName: ctx.stepName,
      namespace: ctx.namespace,
      correlationId: ctx.correlationId,
      result: ctx.result,
      metadata: ctx.metadata,
    };
  }
  
  beforePublish(eventName: string, payload: Record<string, unknown>, metadata: Record<string, unknown>): boolean {
    return true;
  }
  
  afterPublish(eventName: string, payload: Record<string, unknown>, metadata: Record<string, unknown>): void {
    // Default: no-op
  }
  
  onPublishError(eventName: string, error: Error, payload: Record<string, unknown>): void {
    console.error(`Failed to publish ${eventName}:`, error);
  }
  
  additionalMetadata(ctx: StepEventContext): Record<string, unknown> {
    return {};
  }
}

export interface DomainEvent {
  eventId: string;
  eventName: string;
  payload: Record<string, unknown>;
  metadata: Record<string, unknown>;
  executionResult: Record<string, unknown>;
}

export abstract class BaseSubscriber {
  static subscribesTo(): string[] {
    return ['*'];
  }
  
  abstract handle(event: DomainEvent): Promise<void> | void;
  
  matches(eventName: string): boolean {
    const patterns = (this.constructor as typeof BaseSubscriber).subscribesTo();
    return patterns.some(pattern => {
      const regex = new RegExp('^' + pattern.replace('*', '.*') + '$');
      return regex.test(eventName);
    });
  }
  
  beforeHandle(event: DomainEvent): boolean {
    return true;
  }
  
  afterHandle(event: DomainEvent): void {
    // Default: no-op
  }
  
  onHandleError(event: DomainEvent, error: Error): void {
    console.error(`Failed to handle ${event.eventName}:`, error);
  }
}

export class InProcessDomainEventPoller {
  // Implementation similar to Python with FFI integration
  private running = false;
  private eventCallbacks: Array<(event: DomainEvent) => void> = [];
  private errorCallbacks: Array<(error: Error) => void> = [];
  
  onEvent(callback: (event: DomainEvent) => void): void {
    this.eventCallbacks.push(callback);
  }
  
  onError(callback: (error: Error) => void): void {
    this.errorCallbacks.push(callback);
  }
  
  start(): void {
    // TODO: Implement polling via FFI
  }
  
  stop(): void {
    // TODO: Implement graceful shutdown
  }
  
  get isRunning(): boolean {
    return this.running;
  }
}
```

#### 3. Document Publishing Architecture

**Create `docs/domain-events-publishing-flow.md`**:
- Explain that handlers DON'T call publish APIs directly
- Show YAML declaration → Rust orchestration flow
- Document custom publisher use cases (payload transformation only)
- Clarify when to use durable vs fast vs broadcast
- Show FFI integration for subscribers

**Update `docs/worker-crates/*.md`**:
- Add domain events section to each language doc
- Show custom publisher examples
- Show subscriber examples
- Explain FFI integration

#### 4. Standardize Event Structure

**Ensure consistent event payload across languages**:
```json
{
  "event_id": "UUID v7",
  "event_name": "payment.processed",
  "event_version": "1.0",
  "payload": {
    "task_sequence_step": { ... },
    "execution_result": { ... },
    "payload": { ... }  // Business-specific event data
  },
  "metadata": {
    "task_uuid": "UUID",
    "step_uuid": "UUID",
    "step_name": "process_payment",
    "namespace": "payments",
    "correlation_id": "UUID",
    "fired_at": "ISO8601",
    "fired_by": "handler_name"
  }
}
```

### Important Clarifications

#### 1. Handlers Don't Publish Events Directly

**Wrong Understanding**:
```ruby
# ❌ Handlers don't call this
publish_domain_event("payment.processed", payload)
```

**Correct Understanding**:
```yaml
# ✅ Events declared in YAML
publishes_events:
  - name: payment.processed
    condition: success
    delivery_mode: durable
    publisher: PaymentEventPublisher  # Optional custom transformation
```

```ruby
# ✅ Handler just returns result
def call(context)
  # Process payment
  result = process_payment(context.inputs)
  
  # Return result - Rust orchestration publishes events
  success(
    transaction_id: result.id,
    amount: result.amount,
    currency: result.currency
  )
end
```

**Custom Publisher Role**:
```ruby
# ✅ Custom publisher transforms payload (called by Rust orchestration)
class PaymentEventPublisher < TaskerCore::DomainEvents::BasePublisher
  def transform_payload(step_result, event_declaration, step_context = nil)
    result = step_result[:result]
    
    {
      transaction_id: result[:transaction_id],
      amount: result[:amount],
      currency: result[:currency] || 'USD',
      payment_method: result[:payment_method] || 'credit_card',
      processed_at: result[:processed_at] || Time.now.iso8601
    }
  end
end
```

#### 2. Subscribers Are for Internal Use Only

**Fast Events** (in-process):
- Internal Tasker subscribers (metrics, logging, secondary actions)
- NOT part of the Task → WorkflowStep DAG
- Best-effort delivery (may be lost)
- Sub-millisecond latency

**Durable Events** (PGMQ):
- External consumers (analytics, audit, integration)
- Persisted for reliability
- External systems poll PGMQ queues
- No internal Tasker subscribers

**Wrong**:
```ruby
# ❌ Don't use subscribers for critical business logic
class ProcessRefundSubscriber < BaseSubscriber
  subscribes_to 'payment.processed'
  
  def handle(event)
    # This is a critical business step - should be in the DAG!
    issue_refund(event[:payload])
  end
end
```

**Correct**:
```yaml
# ✅ Critical business steps go in the DAG
steps:
  - name: process_payment
    handler:
      callable: ProcessPaymentHandler
  
  - name: issue_refund
    handler:
      callable: IssueRefundHandler
    dependencies:
      - process_payment  # Explicit dependency
```

```ruby
# ✅ Subscribers for non-critical internal actions
class MetricsSubscriber < BaseSubscriber
  subscribes_to 'payment.*'
  
  def handle(event)
    # Secondary action - metrics collection
    StatsD.increment("payments.#{event[:event_name]}")
  end
end
```

---

## Implementation Checklist

### Python Enhancements
- [ ] Add lifecycle hooks to `BasePublisher`:
  - [ ] `before_publish(event_name, payload, metadata) -> bool`
  - [ ] `after_publish(event_name, payload, metadata) -> None`
  - [ ] `on_publish_error(event_name, error, payload) -> None`
  - [ ] `additional_metadata(ctx) -> dict`
- [ ] Add lifecycle hooks to `BaseSubscriber`:
  - [ ] `before_handle(event) -> bool`
  - [ ] `after_handle(event) -> None`
  - [ ] `on_handle_error(event, error) -> None`
- [ ] Add `PublisherRegistry` class for centralized management
- [ ] Add `SubscriberRegistry` class for centralized management
- [ ] Update examples to show lifecycle hooks

### TypeScript Complete Implementation
- [ ] Create `workers/typescript/src/handler/domain-events.ts`:
  - [ ] `StepEventContext` interface
  - [ ] `BasePublisher` abstract class with full hooks
  - [ ] `BaseSubscriber` abstract class with pattern matching
  - [ ] `DomainEvent` interface
  - [ ] `InProcessDomainEventPoller` class
  - [ ] `PublisherRegistry` class
  - [ ] `SubscriberRegistry` class
- [ ] Implement FFI integration:
  - [ ] Add `poll_in_process_events` to FFI layer
  - [ ] Test broadcast channel communication
- [ ] Create examples:
  - [ ] Payment event publisher example
  - [ ] Logging subscriber example
  - [ ] Metrics subscriber example
- [ ] Update `bootstrap.ts` to actually initialize domain events

### Documentation
- [ ] Create `docs/domain-events-publishing-flow.md`:
  - [ ] Explain YAML declaration model
  - [ ] Show orchestration publishing flow
  - [ ] Clarify custom publisher role (transformation only)
  - [ ] Document when to use durable vs fast vs broadcast
- [ ] Update `docs/worker-crates/rust.md`:
  - [ ] Add subscriber examples
  - [ ] Show EventHandler pattern
  - [ ] Document InProcessEventBus usage
- [ ] Update `docs/worker-crates/ruby.md`:
  - [ ] Add custom publisher examples
  - [ ] Add subscriber examples
  - [ ] Show registry usage
- [ ] Update `docs/worker-crates/python.md`:
  - [ ] Add custom publisher examples
  - [ ] Add subscriber examples
  - [ ] Document poller setup
- [ ] Create `docs/worker-crates/typescript.md`:
  - [ ] Complete domain events section
  - [ ] Show all patterns
- [ ] Update `docs/domain-events.md`:
  - [ ] Add "How Publishing Works" section
  - [ ] Clarify handler vs orchestration roles
  - [ ] Show all four languages

### Testing
- [ ] Python: Test lifecycle hooks
- [ ] TypeScript: Full domain events integration test
- [ ] All languages: Verify event payload structure matches
- [ ] All languages: Test FFI integration with in-process bus

---

## Next Phase

**Phase 8: Conditional Workflows** will analyze how decision point handlers create dynamic workflow branches and how convergence steps aggregate results. Key questions:
- Are DecisionPointOutcome structures identical?
- Do all languages support routing context propagation?
- Is convergence step handling consistent?
- Are helper methods for branch creation aligned?

---

## Metadata

**Document Version**: 1.0  
**Analysis Date**: 2025-12-27  
**Reviewers**: TBD  
**Approval Status**: Draft
