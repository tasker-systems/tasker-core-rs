# High-Throughput Concurrency Architecture

## Executive Summary

This document outlines the design and implementation plan for an **event-driven pub/sub architecture** that enables high-throughput concurrent step processing while solving Ruby threading constraints in the Tasker Core system.

**Current State**: Sequential step processing (stable, production-ready)
**Future State**: Event-driven concurrent processing (10x+ throughput improvement)
**Implementation Timeline**: 8 weeks when scalability bottlenecks justify complexity
**Target Throughput**: 100+ concurrent tasks with >10x performance improvement

---

## Architecture Overview

### Core Principle
Replace direct FFI step execution with an **internal event bus** that completely decouples Rust orchestration from Ruby step execution, enabling true concurrency while respecting Ruby's thread-local interpreter constraints.

### System Boundaries

```
┌─────────────────┐    Events    ┌─────────────────┐    Events    ┌─────────────────┐
│ Rust            │ ──────────► │ Internal        │ ──────────► │ Ruby            │
│ Orchestration   │              │ Event Bus       │              │ Execution       │
│ Layer           │ ◄────────── │                 │ ◄────────── │ Layer           │
└─────────────────┘    Results   └─────────────────┘    Results   └─────────────────┘
```

**Benefits**:
- **Zero Ruby Threading Issues**: Each Ruby subscriber runs in its own thread context
- **True Concurrency**: Multiple steps execute simultaneously using concurrent-ruby
- **Fault Isolation**: Step failures don't impact orchestration layer
- **Scalable Architecture**: Event-driven pattern supports high-throughput scenarios

---

## Event Types and Payload Structures

### Internal Event Types

| Event Type | Direction | Purpose |
|------------|-----------|---------|
| `StepExecutionBatch` | Rust → Ruby | Multiple ready steps for concurrent processing |
| `StepExecutionResult` | Ruby → Rust | Individual step completion results |
| `StepExecutionError` | Ruby → Rust | Step failures requiring retry/error handling |
| `BatchStatus` | Ruby → Rust | Heartbeat events for batch processing status |
| `StepCancellation` | Rust → Ruby | Cancel in-flight steps (task termination) |

### Key Payload Structures

#### StepExecutionBatch
```json
{
  "batch_id": "batch_12345",
  "created_at": "2025-01-22T15:30:00Z",
  "expected_completion_time": "2025-01-22T15:32:00Z",
  "priority": "normal",
  "steps": [
    {
      "step_id": 5227,
      "task_id": 5862,
      "step_name": "validate_order",
      "handler_class": "OrderFulfillment::ValidateOrderStepHandler",
      "handler_config": { "timeout": 30 },
      "task_context": { "order_id": 12345, "customer_id": 67890 },
      "dependencies": [
        { "step_id": 5226, "output": { "status": "verified" } }
      ]
    }
  ]
}
```

#### StepExecutionResult
```json
{
  "batch_id": "batch_12345",
  "step_id": 5227,
  "task_id": 5862,
  "status": "completed",
  "output_data": { "validation_result": "approved", "processed_at": "2025-01-22T15:31:15Z" },
  "execution_time_ms": 1250,
  "handler_class": "OrderFulfillment::ValidateOrderStepHandler"
}
```

#### StepExecutionError
```json
{
  "batch_id": "batch_12345",
  "step_id": 5227,
  "task_id": 5862,
  "error_message": "Order validation failed: insufficient inventory",
  "error_code": "VALIDATION_FAILED",
  "retry_eligible": true,
  "execution_time_ms": 500,
  "stack_trace": "...",
  "handler_class": "OrderFulfillment::ValidateOrderStepHandler"
}
```

---

## Detailed Workflow Lifecycles

### Phase 1: Step Discovery & Batching (Rust)

```rust
// WorkflowCoordinator discovers viable steps
let viable_steps = discover_viable_steps(task_ids).await?;

// Group steps for optimal concurrent execution
let batches = create_intelligent_batches(viable_steps);

for batch in batches {
    // Update step state to prevent duplicate processing
    update_steps_to_batched_state(&batch.step_ids).await?;

    // Publish batch event with timeout expectations
    event_publisher.publish_step_execution_batch(batch).await?;
}
```

**Intelligent Batching Logic**:
- **Task Affinity**: Group steps from same task for context sharing
- **Priority-Based**: High-priority steps get smaller batches for faster processing
- **Handler Class Optimization**: Group steps using same handler class
- **Dependency-Aware**: Never batch interdependent steps

**State Management**:
- New state: `"batched_for_execution"` (between "pending" and "in_progress")
- Prevents duplicate processing while enabling timeout recovery
- Atomic state transitions using database transactions

### Phase 2: Batch Reception & Distribution (Ruby)

```ruby
class OrchestrationEventSubscriber
  def on_step_execution_batch(event)
    batch = StepExecutionBatch.new(event.payload)

    # Validate batch integrity and step readiness
    validate_batch_integrity(batch)

    # Publish batch status heartbeat
    publish_batch_status(batch.id, 'processing_started')

    # Create concurrent execution plan
    futures = batch.steps.map do |step|
      Concurrent::Future.execute(executor: thread_pool) do
        execute_step_with_full_context(step)
      end
    end

    # Wait for completion and collect results
    handle_batch_completion(batch.id, futures)
  end

  private

  def thread_pool
    @thread_pool ||= Concurrent::ThreadPoolExecutor.new(
      min_threads: 2,
      max_threads: 10,
      max_queue: 100,
      fallback_policy: :caller_runs
    )
  end
end
```

**Ruby Concurrency Configuration**:
- **Thread Pool**: Bounded thread count (10-20) respecting Ruby GIL
- **Queue Management**: Bounded queue prevents memory overflow
- **Resource Isolation**: Each step gets isolated context
- **Graceful Degradation**: Falls back to smaller batches under pressure

### Phase 3: Concurrent Step Execution (Ruby)

```ruby
def execute_step_with_full_context(step)
  # Each Concurrent::Future runs in separate thread
  # Full Ruby interpreter access available

  # Instantiate handler using pre-registered classes
  handler_class = Object.const_get(step.handler_class)
  handler = handler_class.new(config: step.handler_config)

  # Execute step with context and dependencies
  result = handler.process(
    task_context: step.task_context,
    dependencies: step.dependencies,
    step_config: step.handler_config
  )

  # Return structured result for event publishing
  StepExecutionResult.new(
    batch_id: step.batch_id,
    step_id: step.step_id,
    status: 'completed',
    output_data: result,
    execution_time_ms: execution_time
  )
rescue => error
  StepExecutionError.new(
    batch_id: step.batch_id,
    step_id: step.step_id,
    error_message: error.message,
    retry_eligible: determine_retry_eligibility(error)
  )
end
```

**Concurrent Execution Benefits**:
- **Thread Isolation**: Each step runs in separate Ruby thread context
- **No GIL Blocking**: I/O-bound operations (most steps) benefit from concurrency
- **Error Isolation**: Individual step failures don't impact other steps
- **Resource Optimization**: Better CPU and I/O utilization

### Phase 4: Result Collection & Publishing (Ruby)

```ruby
def handle_batch_completion(batch_id, futures)
  # Wait for all futures to complete or timeout
  results = Concurrent::Promise.zip(*futures).execute.value(timeout: 30)

  # Publish individual step results
  results.each do |result|
    case result
    when StepExecutionResult
      publish_step_execution_result(result)
    when StepExecutionError
      publish_step_execution_error(result)
    end
  end

  # Publish batch completion status
  publish_batch_status(batch_id, 'completed', {
    total_steps: results.length,
    successful: results.count { |r| r.is_a?(StepExecutionResult) },
    failed: results.count { |r| r.is_a?(StepExecutionError) }
  })
end
```

**Result Processing Features**:
- **Individual Result Events**: Each step result published separately
- **Batch Summary**: Aggregate metrics for monitoring
- **Timeout Handling**: Graceful handling of slow steps
- **Error Aggregation**: Failed steps don't block successful ones

### Phase 5: Result Processing & State Updates (Rust)

```rust
// EventPublisher receives results via callback registration
impl EventCallback for StepResultProcessor {
    async fn on_step_execution_result(&self, result: StepExecutionResult) -> Result<(), Error> {
        // Process successful step completion
        self.state_manager
            .complete_step_with_results(result.step_id, Some(result.output_data))
            .await?;

        // Clear backoff for successful completion
        self.backoff_calculator.clear_backoff(result.step_id).await?;

        // Trigger task-level state evaluation
        self.evaluate_task_completion(result.task_id).await?;

        Ok(())
    }

    async fn on_step_execution_error(&self, error: StepExecutionError) -> Result<(), Error> {
        if error.retry_eligible {
            // Calculate backoff and requeue for retry
            let retry_delay = self.backoff_calculator
                .calculate_backoff(error.step_id, &error.error_message)
                .await?;
            self.schedule_step_retry(error.step_id, retry_delay).await?;
        } else {
            // Mark as permanently failed
            self.state_manager
                .fail_step(error.step_id, &error.error_message)
                .await?;
        }

        Ok(())
    }
}
```

**State Coordination Features**:
- **Individual Processing**: Each step result processed independently
- **Retry Logic**: Failed steps automatically scheduled for retry
- **Task Completion**: Triggers task-level evaluation after batch processing
- **Event Correlation**: Batch and step IDs ensure proper result attribution

---

## Error Handling and Fault Tolerance

### Partial Batch Failure Handling

**Philosophy**: Individual step failures never fail entire batches
- Ruby publishes both successful and failed step events
- Rust processes mixed results individually
- Failed retryable steps requeued for future batches
- Non-retryable failures immediately marked as failed

### Timeout and Orphan Prevention

**Watchdog Pattern**:
```rust
// Timeout watchdog checks for orphaned steps
async fn timeout_watchdog() {
    let orphaned_steps = find_steps_in_state("batched_for_execution")
        .filter(|step| step.batched_at < now() - Duration::from_secs(120))
        .collect();

    for step in orphaned_steps {
        warn!("Resetting orphaned step {} to pending", step.id);
        reset_step_to_pending(step.id).await?;
    }
}
```

**Heartbeat System**:
- Ruby sends `BatchStatus` heartbeats every 30 seconds
- Missing heartbeats trigger timeout recovery
- Orphaned steps reset to "pending" for reprocessing

### Event Delivery Guarantees

**At-Least-Once Delivery**:
- Acceptable since step execution should be idempotent
- Event deduplication using `batch_id + step_id`
- Ruby maintains in-memory processing set
- Rust retry logic for critical result events

---

## Concurrency Optimization

### Batch Size Optimization

| Step Complexity | Batch Size | Thread Count | Reasoning |
|----------------|------------|--------------|-----------|
| Simple (< 1s) | 10-20 steps | 8-10 threads | Maximize throughput |
| Medium (1-5s) | 5-10 steps | 6-8 threads | Balance resource usage |
| Complex (> 5s) | 2-5 steps | 4-6 threads | Prevent resource exhaustion |

### Dynamic Tuning

```yaml
# config/concurrency.yaml
step_execution:
  batching:
    max_batch_size: 20
    min_batch_size: 2
    timeout_seconds: 120

  ruby_executor:
    min_threads: 2
    max_threads: 10
    max_queue_size: 100

  monitoring:
    batch_timeout_threshold: 30
    error_rate_threshold: 0.10
    queue_depth_alert: 50
```

**Performance Optimization**:
- **Adaptive Batch Sizes**: Adjust based on step execution times
- **Circuit Breakers**: Fall back to sequential on high error rates
- **Queue Depth Monitoring**: Prevent memory overflow
- **Thread Pool Tuning**: Environment-specific configurations

---

## Monitoring and Observability

### Key Performance Indicators

| Metric | Target | Alert Threshold |
|--------|--------|-----------------|
| Batch Processing Time | < 30s | > 60s |
| Step Execution Throughput | 100+ steps/min | < 50 steps/min |
| Error Rate | < 5% | > 10% |
| Queue Depth | < 10 batches | > 50 batches |
| Thread Utilization | 60-80% | > 90% |

### Event Flow Tracing

```rust
// Correlation ID tracking through entire flow
struct BatchTrace {
    batch_id: String,
    created_at: SystemTime,
    rust_published_at: SystemTime,
    ruby_received_at: Option<SystemTime>,
    ruby_completed_at: Option<SystemTime>,
    rust_processed_at: Option<SystemTime>,
    step_results: Vec<StepTrace>,
}
```

**Observability Features**:
- **End-to-End Tracing**: Track batch lifecycle across system boundaries
- **Performance Histograms**: Step execution time distributions
- **Error Categorization**: Classify failures by type and handler
- **Resource Utilization**: Monitor Ruby memory and thread usage

---

## Implementation Phases

### Phase 1: Infrastructure Foundation (Week 1-2)
- [ ] Implement internal event types (`StepExecutionBatch`, `StepExecutionResult`, etc.)
- [ ] Create Ruby `OrchestrationEventSubscriber` with concurrent-ruby integration
- [ ] Build basic pub/sub infrastructure with in-memory channels
- [ ] Add event correlation ID system

### Phase 2: Core Workflow Implementation (Week 3-4)
- [ ] Implement intelligent batch creation in `WorkflowCoordinator`
- [ ] Build Ruby concurrent step execution with `Concurrent::Future`
- [ ] Create result collection and publishing mechanisms
- [ ] Add comprehensive error handling for partial failures

### Phase 3: State Machine Integration (Week 5)
- [ ] Add `"batched_for_execution"` state to state machine
- [ ] Implement atomic state transitions with event publishing
- [ ] Build timeout watchdog for orphaned step recovery
- [ ] Ensure consistency between async events and database state

### Phase 4: Performance Optimization (Week 6-7)
- [ ] Implement intelligent batching algorithms (task affinity, priority-based)
- [ ] Add dynamic thread pool sizing and batch size optimization
- [ ] Build comprehensive monitoring and alerting
- [ ] Performance testing and benchmarking

### Phase 5: Production Readiness (Week 8)
- [ ] Implement circuit breakers and fallback to sequential processing
- [ ] Add comprehensive observability and distributed tracing
- [ ] Build deployment automation and configuration management
- [ ] Conduct load testing and performance validation

---

## Migration Strategy

### Phased Rollout

**Phase 1 - Infrastructure**: Deploy event infrastructure without changing execution
**Phase 2 - Hybrid Mode**: Feature flag to route specific tasks through event-driven path
**Phase 3 - A/B Testing**: Performance comparison on production workloads
**Phase 4 - Full Migration**: Event-driven as default with sequential fallback

### Feature Flag Configuration

```yaml
# config/feature_flags.yaml
event_driven_execution:
  enabled: false
  rollout_percentage: 0
  task_types:
    - "order_fulfillment"  # Start with specific task types
  max_concurrent_tasks: 10
  fallback_enabled: true
```

### Backward Compatibility

**Preservation Guarantees**:
- All existing Ruby step handlers work unchanged
- `TaskHandler.handle()` and `handle_one_step()` APIs preserved
- Existing metrics and logging continue to work
- Sequential processing available as fallback

---

## Decision Criteria and Success Metrics

### Implementation Triggers

**When to Implement**:
- Sequential processing becomes genuine bottleneck (>100 concurrent tasks regularly)
- Team has 8+ weeks for focused implementation and testing
- Current sequential system is stable and can serve as reliable fallback
- Need >10x throughput improvement to justify complexity

### Success Metrics

**Performance Targets**:
- Achieve 10x+ step execution throughput vs sequential
- Maintain <5% error rate under high concurrency
- Sub-30-second batch processing latency
- Zero data loss during event processing failures

**Operational Targets**:
- 99.9% uptime during migration
- <1% performance regression on existing workloads
- Complete rollback capability within 5 minutes

---

## Risk Assessment and Mitigation

### High-Risk Areas

| Risk | Impact | Probability | Mitigation |
|------|---------|-------------|------------|
| Event delivery failures | High | Medium | At-least-once delivery + deduplication |
| Ruby memory leaks | High | Medium | Resource monitoring + circuit breakers |
| Partial state corruption | Critical | Low | Atomic transactions + consistency checks |
| Performance regression | Medium | Medium | A/B testing + automatic fallback |

### Contingency Plans

**Rollback Strategy**:
- Feature flag instant disable
- Automatic fallback to sequential processing
- Database state consistency verification
- Performance monitoring during rollback

---

## Conclusion

The event-driven pub/sub architecture provides a scalable solution for high-throughput concurrent step processing while completely solving Ruby threading constraints. The 8-week implementation timeline delivers a production-ready system with comprehensive monitoring, fault tolerance, and backward compatibility.

**Key Benefits**:
- **10x+ Performance**: Genuine concurrent step execution
- **Ruby Thread Safety**: Each subscriber in own thread context
- **Fault Isolation**: Step failures don't impact orchestration
- **Scalable Design**: Event-driven pattern supports massive scale

**Implementation Decision**: Proceed when sequential processing becomes a genuine bottleneck and the team can dedicate focused effort to this significant architectural enhancement.
