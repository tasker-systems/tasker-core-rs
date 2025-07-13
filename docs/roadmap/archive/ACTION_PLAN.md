# Immediate Action Plan - Testing Foundation

Based on the ROADMAP and TODO analysis, here's our prioritized action plan to ensure a solid foundation:

## Week 1: Core Integration Tests 🧪

### Day 1-2: Complex Workflow Tests
```rust
// tests/orchestration/complex_workflow_test.rs
```
- [ ] Test LinearWorkflow pattern with state transitions
- [ ] Test DiamondWorkflow with parallel execution
- [ ] Test error propagation through workflow DAG
- [ ] Test retry behavior with backoff

### Day 3-4: SQL Function Integration Tests
```rust
// tests/sql_functions/workflow_execution_test.rs
```
- [ ] Test step_readiness with complex dependencies
- [ ] Test concurrent step discovery
- [ ] Test state transition atomicity
- [ ] Benchmark SQL function performance

### Day 5: Ruby Binding Basic Tests
```ruby
# bindings/ruby/spec/tasker_core_spec.rb
```
- [ ] Test module loading and initialization
- [ ] Test error hierarchy
- [ ] Test basic handler instantiation
- [ ] Test context serialization

## Week 2: Configuration & Event System 🔧

### Day 1-2: Configuration Extraction
**Priority TODOs to address:**
- `task_handler.rs:397` - Timeout configuration
- `workflow_coordinator.rs:116,125` - Discovery and batch configs
- `handlers.rs:607` - Dependent system resolution

**Actions:**
- [ ] Create `config/tasker-core.yaml` with all settings
- [ ] Implement ConfigurationManager enhancements
- [ ] Add environment variable overrides
- [ ] Document all configuration options

### Day 3-4: Event Publishing Core
**Priority TODOs to address:**
- `step_handler.rs:302,367,386` - Event publishing
- `task_finalizer.rs:715,730,740,750,761` - Event publishing
- `event_publisher.rs:421` - FFI bridge

**Actions:**
- [ ] Complete EventPublisher implementation
- [ ] Add event buffering and batching
- [ ] Create event type registry
- [ ] Test event flow end-to-end

### Day 5: Ruby Event Bridge
```ruby
# bindings/ruby/lib/tasker_core/events.rb
```
- [ ] Implement FFI callback registration (TODO line 173)
- [ ] Bridge to dry-events system
- [ ] Test event propagation Ruby→Rust→Ruby
- [ ] Ensure Rails engine compatibility

## Week 3: Developer Operations & Logging 🛠️

### Day 1-2: CRUD Operations
**New implementations needed:**
- [ ] `Task::reset_attempts()`
- [ ] `Task::cancel_with_reason()`
- [ ] `WorkflowStep::update_result()`
- [ ] `WorkflowStep::mark_resolved()`

### Day 3-4: Structured Logging
**Components to build:**
- [ ] `StructuredLogger` trait
- [ ] `CorrelationId` generator (UUID v7)
- [ ] Thread-local correlation storage
- [ ] JSON formatter with standard fields

### Day 5: Ruby Integration
- [ ] Expose CRUD operations via Magnus
- [ ] Integrate logging with Rails.logger
- [ ] Ensure correlation IDs flow properly
- [ ] Add REST endpoint helpers

## Critical Path Items 🚨

### Immediate Fixes Required:
1. **State Machine Integration** (Multiple TODOs)
   - Client handlers need state transitions
   - Currently missing in-progress/completed transitions

2. **Task Initialization** (`handlers.rs:453`)
   - Create proper TaskInitializer integration
   - Required for Ruby binding to work correctly

3. **Queue Integration** (`task_enqueuer.rs:393`)
   - Implement actual queue system bridge
   - Critical for production use

### Configuration Priorities:
1. **Database Pool Settings**
   - Connection limits
   - Timeout configurations

2. **Retry Policies**
   - Max attempts (hardcoded in multiple places)
   - Backoff strategies

3. **Concurrency Limits**
   - Step batch sizes
   - Parallel execution limits

## Success Metrics 📊

### Week 1:
- [ ] 100% test coverage for complex workflows
- [ ] All SQL functions tested with real data
- [ ] Basic Ruby gem tests passing

### Week 2:
- [ ] Zero hardcoded configuration values
- [ ] Events flowing from Rust to Ruby
- [ ] Configuration fully documented

### Week 3:
- [ ] All CRUD operations exposed
- [ ] Structured logging operational
- [ ] Correlation IDs working end-to-end

## Next Decision Point

After Week 3, evaluate:
1. Are the foundations solid enough for production?
2. Which performance optimizations are needed?
3. What monitoring/observability gaps remain?

Then proceed with remaining roadmap phases based on findings.