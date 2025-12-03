# TAS-65: Observability Cost Guidelines (Updated with TAS-29 Integration)

**Purpose**: Define where observability is valuable vs. where it adds unacceptable overhead.

**Key Updates**:
- Integration with existing `correlation_id` system (TAS-29)
- Namespace-aware sampling strategies
- TaskTemplate and TaskNamespace considerations

## Core Principle

**Observability is worthwhile when the cost is dwarfed by existing I/O operations.**  
**Observability is NOT worthwhile in tight loops, polling intervals, or purely computational hot paths.**

---

## TAS-29 Correlation ID Integration

**CRITICAL**: The system already has distributed tracing infrastructure via `correlation_id` (TAS-29).

### Existing Correlation ID System

```rust
// From tasker-shared/src/models/core/task.rs
pub struct Task {
    pub task_uuid: Uuid,
    /// TAS-29: Correlation ID for distributed tracing
    pub correlation_id: Uuid,
    /// TAS-29: Optional parent correlation ID for workflow hierarchies
    pub parent_correlation_id: Option<Uuid>,
    // ... other fields
}
```

**Usage Pattern**:
- Every task has a `correlation_id` assigned at creation
- Workflow hierarchies use `parent_correlation_id` to link tasks
- Workers and orchestrators propagate `correlation_id` through the execution pipeline

### Span Integration with Correlation IDs

**✅ CORRECT**: Use existing correlation_id
```rust
use tracing::{span, Level};

// Task orchestration span
let task_span = span!(
    Level::INFO,
    "task.orchestration",
    task_uuid = %task.task_uuid,
    namespace = %task.namespace,
    correlation_id = %task.correlation_id,  // ✅ From Task struct (TAS-29)
    parent_correlation_id = ?task.parent_correlation_id  // ✅ Optional hierarchy
);

// Step execution span (child of task span)
let step_span = span!(
    Level::INFO,
    "step.execution",
    step_uuid = %step.step_uuid,
    step_name = %step.name,
    task_uuid = %task.task_uuid,
    correlation_id = %task.correlation_id,  // ✅ Inherit from task
    namespace = %task.namespace  // ✅ For namespace-based aggregation
);
```

**❌ WRONG**: Create new correlation IDs
```rust
// DON'T DO THIS
let new_correlation_id = Uuid::new_v4();  // ❌ Breaks distributed tracing
let span = span!(Level::INFO, "operation", correlation_id = %new_correlation_id);
```

---

## Namespace-Aware Sampling

**Key Insight**: TaskNamespace provides natural organizational boundaries for cardinality control.

### Namespace Architecture

From `tasker-shared/src/models/core/task_namespace.rs`:
```rust
pub struct TaskNamespace {
    pub task_namespace_uuid: Uuid,
    pub name: String,  // e.g., "payments", "orders", "analytics"
    pub description: Option<String>,
    // ...
}
```

Namespaces organize tasks by business domain:
- **payments**: Payment processing workflows (high-value, sample 100%)
- **orders**: Order fulfillment workflows (medium-value, sample 50%)
- **analytics**: Reporting and analytics (high-volume, sample 1-10%)
- **integrations**: Third-party integrations (variable volume)

### Namespace-Specific Sampling Configuration

**Config**: `config/tasker/base/telemetry.toml`
```toml
[telemetry]
enabled = true
log_level = "info"

[telemetry.tracing]
enabled = true
sampler = "parent_based"
sample_rate = 1.0  # Global default

[telemetry.metrics]
enabled = true

[telemetry.metrics.detailed]
enabled = true
sample_rate = 1.0  # Global detailed metrics sample rate

# Namespace-specific sampling overrides
[telemetry.metrics.namespaces]
payments = 1.0        # 100% - critical business path
orders = 0.5          # 50% - medium priority
analytics = 0.01      # 1% - high volume, sample for trends
integrations = 0.1    # 10% - debug third-party issues

[telemetry.spans.internal]
enabled = false
```

### Implementation: Namespace-Aware Sampling

```rust
use crate::config::TelemetryConfig;
use rand::Rng;

pub struct NamespaceAwareSampler {
    config: Arc<TelemetryConfig>,
}

impl NamespaceAwareSampler {
    pub fn should_sample_detailed(&self, namespace: &str) -> bool {
        let sample_rate = self.config
            .metrics
            .namespaces
            .get(namespace)
            .copied()
            .unwrap_or(self.config.metrics.detailed.sample_rate);
        
        rand::thread_rng().gen::<f64>() < sample_rate
    }
}

// Usage in step execution
async fn execute_step(&self, step: &WorkflowStep) -> Result<StepResult> {
    let start = Instant::now();
    
    // Always track high-level metrics (low cardinality)
    metrics::counter!(
        "tasker.steps.executions.total",
        1,
        "namespace" => &step.namespace
    );
    
    // Execute step
    let result = self.handler.execute(step).await?;
    
    // Sample detailed metrics based on namespace
    if self.sampler.should_sample_detailed(&step.namespace) {
        metrics::histogram!(
            "tasker.steps.duration.detailed",
            start.elapsed().as_secs_f64(),
            "namespace" => &step.namespace,
            "step_name" => &step.name  // High cardinality, but sampled
        );
    }
    
    Ok(result)
}
```

### Cardinality Analysis with Namespaces

**Low Cardinality** (Always Safe):
```rust
metrics::counter!(
    "tasker.tasks.completions.total",
    1,
    "namespace" => namespace,  // ~10-50 namespaces
    "final_state" => state     // 4 terminal states: Complete, Error, Cancelled, ResolvedManually
);
// Total cardinality: 10 × 4 = 40 unique metrics
```

**Medium Cardinality** (Namespace-Aggregated):
```rust
metrics::counter!(
    "tasker.domain_events.published.total",
    1,
    "namespace" => namespace,  // ~10-50 namespaces
    "event_name" => event_name  // ~10-100 events per namespace
);
// Total cardinality: 50 × 100 = 5,000 unique metrics (manageable)
```

**High Cardinality** (Sampled by Namespace):
```rust
// Only emit if namespace sample rate allows
if sampler.should_sample_detailed(namespace) {
    metrics::counter!(
        "tasker.steps.executions.detailed",
        1,
        "namespace" => namespace,    // ~10-50 namespaces
        "step_name" => step_name,    // ~100-1000 steps per namespace
        "handler_type" => handler    // ~5-10 handler types
    );
}
// Total cardinality: 50 × 1000 × 10 = 500,000 unique metrics
// But with 10% sampling: 50,000 active metrics
```

---

## Instrumentation Decision Matrix

| Path Type | I/O Already Present? | Instrument? | Why |
|-----------|----------------------|-------------|-----|
| **State Transition** | ✅ Database write | ✅ YES - Span event | DB write is 1-10ms, span event is <100μs |
| **PGMQ Send** | ✅ Database insert + notify | ✅ YES - Span event + counter | PGMQ is 1-5ms, metrics are <100μs |
| **PGMQ Read** | ✅ Database query | ✅ YES - Span event (conditional) | Only instrument when messages found |
| **Polling Loop Tick** | ❌ None (sleep) | ❌ NO - Atomic counter only | Sleep dominates (30s), don't add metrics |
| **Handler Dispatch** | ✅ Handler execution (Ruby FFI) | ✅ YES - Span + histogram | FFI call is expensive, metrics negligible |
| **Event Registry Lookup** | ❌ None (HashMap) | ❌ NO - No instrumentation | Pure computation, metrics would slow it down |
| **Circuit Breaker Check** | ❌ None (atomic) | ❌ NO - State change only | Hot path, instrument state changes not checks |

---

## Specific Guidance for TAS-65

### Phase 1: System Events → OpenTelemetry (CORRECT AS-IS with TAS-29 Integration)

**DO Instrument:**
- State machine transitions (already doing DB write)
- Task/step creation, completion, failure (already doing DB write)
- Queue message send/receive (already doing PGMQ operations)

**Example - State Transition with Correlation ID:**
```rust
impl StateAction<Task> for PublishTransitionEventAction {
    async fn execute(&self, task: &Task, from_state: Option<String>, to_state: String, event: &str, pool: &PgPool) -> ActionResult<()> {
        // We're already doing a database write in the state machine transition
        // Adding a span event here costs <100μs vs 1-10ms for the DB write
        
        event!(
            Level::INFO,
            event_name = determine_task_event_name(&from_state, &to_state).unwrap_or("unknown"),
            task_uuid = %task.task_uuid,
            correlation_id = %task.correlation_id,  // ✅ TAS-29 integration
            namespace = %task.namespace,
            from_state = from_state.as_deref().unwrap_or("none"),
            to_state = %to_state,
            "Task state transition"
        );
        
        // GOOD: Only update metrics when state actually changes
        TaskMetrics::record_state_transition(&from_state, &to_state, &task.namespace);
        
        Ok(())
    }
}
```

**DON'T Instrument:**
- Internal state machine guard checks (pure logic, no I/O)
- Readiness evaluation loops (SQL function, but internal)

### Phase 2: Domain Events - WITH NAMESPACE-AWARE SAMPLING

**Event Consumer with Namespace Sampling**:
```rust
pub struct EventConsumer {
    namespace: String,
    pool: PgPool,
    registry: Arc<EventRegistry>,
    config: EventConsumerConfig,
    sampler: Arc<NamespaceAwareSampler>,  // ✅ Namespace-aware sampling
    stats: EventConsumerStats,  // ✅ Atomic counters, not metrics
}

impl EventConsumer {
    pub async fn start(self: Arc<Self>) -> Result<()> {
        let queue_name = format!("{}_domain_events", self.namespace);
        let mut interval = tokio::time::interval(Duration::from_millis(self.config.polling_interval_ms));
        
        loop {
            interval.tick().await;
            
            // ✅ GOOD: Fast atomic increment, no allocation
            self.stats.polling_cycles.fetch_add(1, Ordering::Relaxed);
            *self.stats.last_poll_at.lock().await = Some(Instant::now());
            
            // ✅ GOOD: Only debug logs (can be disabled)
            debug!("Polling domain events queue");
            
            let events = self.fetch_events(&queue_name).await?;
            
            if events.is_empty() {
                continue;  // No work, no metrics
            }
            
            // ✅ GOOD: Only emit metrics when we have work to do
            info!(
                count = events.len(),
                namespace = %self.namespace,
                "Processing domain events batch"
            );
            
            // Process events with namespace-aware sampling
            self.process_events_with_sampling(events).await?;
        }
    }
    
    async fn process_event(&self, msg_id: i64, event: DomainEvent) -> Result<()> {
        let start = Instant::now();
        let event_name = event.event_name.clone();
        
        // Dispatch to handlers (expensive FFI call)
        let result = tokio::time::timeout(
            Duration::from_secs(self.config.handler_timeout_seconds),
            self.registry.dispatch(event.clone())
        ).await;
        
        match result {
            Ok(Ok(())) => {
                self.delete_message(msg_id).await?;
                
                // ✅ GOOD: Sample detailed metrics by namespace
                if self.sampler.should_sample_detailed(&self.namespace) {
                    metrics::histogram!(
                        "tasker.domain_events.handler_duration.seconds",
                        start.elapsed().as_secs_f64(),
                        "event_name" => event_name.as_str(),
                        "namespace" => &self.namespace,
                        "status" => "success"
                    );
                }
            }
            Ok(Err(e)) => {
                // ✅ GOOD: Always track failures (rare, important)
                metrics::counter!(
                    "tasker.domain_events.handler_failures.total",
                    1,
                    "event_name" => event_name.as_str(),
                    "namespace" => &self.namespace
                );
                
                self.send_to_dlq(msg_id, event, e.to_string()).await?;
            }
            // ... timeout handling
        }
        
        Ok(())
    }
}
```

---

## Production Configuration Examples

### High-Volume Analytics Namespace
```toml
# config/tasker/environments/production/telemetry.toml
[telemetry.metrics.namespaces]
analytics = 0.01  # 1% sampling for analytics workflows
```

**Rationale**: Analytics workflows may process millions of events daily. Sampling 1% still provides 10,000+ data points for trend analysis while reducing metric cardinality by 99%.

### Critical Payment Namespace
```toml
[telemetry.metrics.namespaces]
payments = 1.0  # 100% sampling for payment workflows
```

**Rationale**: Payment workflows are high-value, low-volume, and require complete audit trails for compliance. Always capture full telemetry.

### Development Environment (Full Observability)
```toml
# config/tasker/environments/development/telemetry.toml
[telemetry.metrics.detailed]
enabled = true
sample_rate = 1.0  # Global 100% sampling

[telemetry.metrics.namespaces]
# No namespace overrides - use global 100%
```

---

## Summary: Instrumentation Checklist

Before adding observability, ask:

1. **Is there already I/O here?**  
   ✅ YES → Observability cost is negligible  
   ❌ NO → Proceed to question 2

2. **Is this a hot path?** (runs >100x/sec per worker)  
   ❌ NO → Observability cost is acceptable  
   ✅ YES → Use atomic counters or namespace-aware sampling

3. **What's the cardinality?** (unique label combinations)  
   - **Low** (<100): Safe for production  
   - **Medium** (100-1000): Aggregate by namespace first  
   - **High** (>1000): Namespace-aware sampling required

4. **Is there already a correlation ID?** (TAS-29)  
   ✅ YES → Use `task.correlation_id` and `task.parent_correlation_id`  
   ❌ NO → Create only at task request boundary

5. **Can this be namespace-scoped?**  
   ✅ YES → Use namespace-specific sampling rates  
   ❌ NO → Apply global sampling rate

---

## TAS-65 Specific Recommendations

### Phase 1: System Events → OpenTelemetry
- **Keep as-is**: State transitions already have DB I/O
- **Add**: TAS-29 correlation_id integration in spans
- **Use**: `task.correlation_id` and `task.parent_correlation_id`
- **Remove**: Any internal guard/validation span creation

### Phase 2: Domain Events → PGMQ
- **Change**: Use atomic counters in polling loop (not metrics)
- **Keep**: Metrics on event processing (handler execution is expensive)
- **Add**: Namespace-aware sampling with configuration
- **Add**: Sample rate per namespace in telemetry config
- **Document**: Cardinality concerns for `event_name` label

### Phase 3: Message Broker Abstraction
- **Add**: Provider-level metrics (PGMQ vs RabbitMQ throughput)
- **Don't add**: Per-message provider selection metrics (hot path)
- **Consider**: Namespace-specific provider routing

### Phase 4: Testing
- **Measure**: Overhead of observability (with/without telemetry enabled)
- **Benchmark**: 99th percentile latency impact of instrumentation
- **Test**: Namespace-specific sampling rates under load
- **Document**: Production tuning guide with namespace recommendations

---

## References

- **TAS-29**: Correlation ID system for distributed tracing
- **TaskNamespace**: `tasker-shared/src/models/core/task_namespace.rs`
- **TaskTemplate**: `tasker-shared/src/models/core/task_template.rs`
- **Task Model**: `tasker-shared/src/models/core/task.rs` (correlation_id fields)
- **Existing Patterns**: `tasker-orchestration/src/orchestration/orchestration_queues/fallback_poller.rs`
