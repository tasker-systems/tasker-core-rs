# Event System Analysis and Roadmap

This document analyzes the Rails engine event system architecture and outlines the roadmap for enhanced Rust-Ruby event integration.

## Current Status

### Phase 2 Accomplishments ✅
- **FFI Event Bridge**: Implemented basic Rust→Ruby event forwarding with callback registration
- **Configuration Management**: Extracted hardcoded values to YAML configuration
- **Event Publishing Core**: Unified event system with dual API (simple + structured)
- **External Callback System**: Registry for cross-language event handlers

### Rails Engine Event System Analysis

#### Architecture Overview
The Rails engine uses a sophisticated event system built on dry-events with the following components:

**Core Infrastructure:**
- **Publisher Singleton**: `Tasker::Events::Publisher` using dry-events with automatic event registration
- **BaseSubscriber Pattern**: Declarative subscription system with automatic method routing
- **Event Constants**: Statically defined events in `Tasker::Constants` (56+ events)
- **Custom Event Registry**: Dynamic registration system for business-specific events
- **Event Catalog**: Complete searchable registry of system + custom events

**Key Design Patterns:**
1. **Singleton Publisher**: Single source of truth for event publishing
2. **Declarative Subscriptions**: `subscribe_to :task_completed, :step_failed` class method pattern
3. **Automatic Method Routing**: Event names map to handler methods (`task.completed` → `handle_task_completed`)
4. **Dual Event Types**: System events (constants) + custom events (dynamic registration)
5. **Safe Payload Access**: Defensive event handling with fallback values

#### Event Categories

**System Events (56+ events):**
- Task Events: `task.completed`, `task.failed`, `task.initialize_requested`
- Step Events: `step.completed`, `step.failed`, `step.execution_requested`
- Workflow Events: `workflow.started`, `workflow.completed`
- Observability Events: Task/Step monitoring and telemetry

**Custom Events (Dynamic):**
- Business-specific: `order.fulfilled`, `payment.processed`, `inventory.restock_needed`
- Auto-discovery: Events defined in handler classes or YAML templates
- Namespace support: `custom.order.processed` format

#### Observability Strategy

**Dual Approach:**
1. **Telemetry (TelemetrySubscriber)**: OpenTelemetry spans for detailed debugging
2. **Metrics (MetricsSubscriber)**: Aggregated data for dashboards/alerts

**TelemetrySubscriber Features:**
- Hierarchical span relationships (task → step spans)
- Automatic duration calculation with fallbacks
- Error status propagation with categorization
- Service-prefixed attributes for multi-service environments
- Sensitive data filtering with configurable parameter filters

**Integration Examples:**
- Slack notifications for critical failures
- PagerDuty alerts for system errors
- StatsD metrics for operational monitoring
- Custom business event workflows

## Gap Analysis: Rust vs Rails Integration

### Current Limitations

1. **One-Way Event Flow**: Rust can forward to Ruby, but Ruby cannot easily publish back to Rust
2. **Payload Structure Mismatch**: Rust uses simple JSON, Rails expects specific payload schemas
3. **Event Type Mapping**: No mapping between Rust event names and Rails constants
4. **Custom Event Registration**: Rust cannot register events in Rails CustomRegistry
5. **BaseSubscriber Incompatibility**: Rust events don't follow Rails subscription patterns

### Missing Features

1. **Bidirectional Event Flow**: Ruby → Rust event publishing capability
2. **Event Type Constants**: Shared constants between Rust and Rails
3. **Payload Standardization**: Compatible payload structures across languages
4. **Custom Event Bridge**: Registration of Rust custom events in Rails registry
5. **Subscription Bridge**: Rust handlers subscribing to Rails events

## Implementation Roadmap

### Phase 3: Enhanced Event Integration (Current)

#### 3.1 FFI Publishing Bridge ⭐ **HIGH PRIORITY**
**Goal**: Enable Rust to publish directly to Rails dry-events Publisher singleton

**Implementation:**
```rust
// New FFI function in handlers.rs
pub fn rust_publish_to_rails(event_name: String, payload: Value) -> Result<Value, Error>
```

**Ruby Integration:**
```ruby
def self.handle_rust_event_publication(event_name, payload)
  Tasker::Events::Publisher.instance.publish(event_name, payload)
end
```

**Benefits:**
- Rust orchestration events appear natively in Rails
- Existing Rails subscribers automatically receive Rust events
- No changes needed to existing Rails event handling code

#### 3.2 Payload Serialization Compatibility ⭐ **HIGH PRIORITY**
**Goal**: Standardize payload structures between Rust and Rails

**Rails Payload Structure:**
```ruby
{
  task_id: "123",
  step_id: "456", 
  timestamp: Time.current,
  task_name: "order_processor",
  step_name: "validate_payment",
  # ... event-specific fields
}
```

**Rust Implementation:**
```rust
pub struct RailsCompatiblePayload {
    task_id: Option<String>,
    step_id: Option<String>, 
    timestamp: DateTime<Utc>,
    task_name: Option<String>,
    step_name: Option<String>,
    // ... additional fields
}
```

#### 3.3 Event Type Mapping 🔶 **MEDIUM PRIORITY**
**Goal**: Map Rust event names to Rails constants for consistency

**Mapping Structure:**
```rust
pub static RAILS_EVENT_MAPPING: &[(&str, &str)] = &[
    ("task.completed", "Tasker::Constants::TaskEvents::COMPLETED"),
    ("step.failed", "Tasker::Constants::StepEvents::FAILED"),
    // ... complete mapping
];
```

**Auto-Resolution:**
```rust
fn resolve_rails_event_constant(rust_event_name: &str) -> Option<&str>
```

#### 3.4 Custom Event Registration Bridge 🔶 **MEDIUM PRIORITY**
**Goal**: Register Rust custom events in Rails CustomRegistry

**FFI Interface:**
```rust
pub fn register_custom_event_in_rails(
    event_name: String,
    description: String, 
    fired_by: Vec<String>
) -> Result<Value, Error>
```

**Rails Integration:**
```ruby
def self.register_rust_custom_event(event_name, description, fired_by)
  Tasker::Events::CustomRegistry.instance.register_event(
    event_name,
    description: description,
    fired_by: fired_by
  )
end
```

### Phase 4: Bidirectional Integration

#### 4.1 Ruby→Rust Event Publishing
**Goal**: Enable Rails to publish events that Rust can handle

#### 4.2 Rust Event Subscription System  
**Goal**: Rust handlers can subscribe to Rails events

#### 4.3 Event Middleware Integration
**Goal**: Leverage dry-events middleware for filtering, routing, batching

### Phase 5: Advanced Features

#### 5.1 Event Pattern Matching
**Goal**: Use dry-events pattern matching for complex event routing

#### 5.2 Event Batching and Buffering
**Goal**: Optimize cross-language event performance

#### 5.3 Event Replay and Recovery
**Goal**: Event sourcing capabilities for workflow recovery

## Implementation Priority

### Phase 3 Sprint Plan (2 weeks)

**Week 1: Core Publishing Bridge**
- Day 1-2: Implement `rust_publish_to_rails` FFI function
- Day 3-4: Create payload serialization compatibility layer
- Day 5: Integration testing with existing Rails subscribers

**Week 2: Event Type Integration**
- Day 1-2: Implement event type mapping system
- Day 3-4: Add custom event registration bridge
- Day 5: Complete integration testing and documentation

### Success Criteria

**Phase 3 Completion:**
1. ✅ Rust can publish events that appear natively in Rails dry-events
2. ✅ Payload structures are compatible between languages
3. ✅ Event type mapping works bidirectionally
4. ✅ Rust custom events register in Rails CustomRegistry
5. ✅ All existing Rails subscribers receive Rust events seamlessly
6. ✅ Integration tests verify end-to-end event flow

**Performance Targets:**
- <1ms FFI overhead per event publication
- >1000 events/sec cross-language throughput
- Zero event loss during FFI transitions

## Technical Considerations

### Error Handling
- Graceful degradation when Rails event system unavailable
- Detailed error reporting for FFI boundary issues
- Automatic retry mechanisms for transient failures

### Thread Safety
- All FFI event operations must be thread-safe
- Rust async event publishing compatible with Ruby threading
- No deadlocks in bidirectional event scenarios

### Memory Management
- Proper Ruby GC integration for event payloads
- Efficient serialization to minimize memory allocation
- Cleanup of event handlers on system shutdown

### Testing Strategy
- Unit tests for each FFI function
- Integration tests for complete event flows
- Performance benchmarks for throughput validation
- Error injection tests for resilience verification

## Migration Strategy

### Backward Compatibility
- All existing Rails event functionality preserved
- No breaking changes to current event handlers
- Optional feature flags for new Rust integration features

### Rollout Plan
1. **Internal Testing**: Verify integration with test events
2. **Shadow Mode**: Publish Rust events alongside existing Rails events
3. **Gradual Migration**: Move specific event types to Rust publishing
4. **Full Integration**: Complete Rust-Rails event unification

## Documentation Requirements

### Developer Guide Updates
- FFI event publishing examples
- Payload structure specifications  
- Event type mapping reference
- Integration testing patterns

### API Documentation
- Complete FFI function reference
- Event payload schemas
- Error handling patterns
- Performance optimization guide

---

**Next Steps**: Begin Phase 3 implementation with FFI publishing bridge as highest priority item.