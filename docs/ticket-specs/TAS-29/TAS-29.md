# TAS-29: Comprehensive Observability & Benchmarking Modernization

## Overview
Complete overhaul of logging, tracing, and performance measurement systems with correlation ID propagation, OpenTelemetry integration, and robust benchmarking infrastructure.

---

## Phase 1: Correlation ID Foundation (Week 1)

### 1.1 Core Type Extensions
**TaskRequest Enhancement**
- Add `correlation_id: Uuid` field (generated or provided at creation)
- Add `parent_correlation_id: Option<Uuid>` for nested workflows
- Builder method `.with_correlation_id(id: Uuid)`
- Auto-generation using Uuid::now_v7() if not provided

**Message Type Updates**
- `TaskRequestMessage`: Add correlation_id field
- `StepMessage`: Add correlation_id field
- `StepResultMessage`: Add correlation_id field
- `PgmqStepMessage`: Add correlation_id to metadata

**Database Schema**
- Add `correlation_id UUID` column to `tasker_tasks` table
- Add `parent_correlation_id UUID` column for workflow chains
- Index on correlation_id for query performance
- Migration: `20250000_add_correlation_ids.sql`

### 1.2 Correlation ID Propagation
**Orchestration Flow**
- TaskInitializer: Extract correlation_id from TaskRequest
- StepEnqueuer: Include correlation_id in all enqueued messages
- ResultProcessor: Preserve correlation_id in result handling
- TaskFinalizer: Include in finalization logging

**Worker Flow**
- Message claim: Extract correlation_id from queue message
- Handler execution: Pass correlation_id to business logic
- Result submission: Include correlation_id in StepResultMessage

**FFI Bridge**
- Expose correlation_id through Ruby FFI bindings
- Add to `ExecuteStepParams` structure
- Available in Ruby handler context

---

## Phase 2: Tracing Instrumentation (Week 2)

### 2.1 Hot Path Instrumentation
**Orchestration Hot Paths**
```rust
#[instrument(skip(self), fields(
    correlation_id = %correlation_id,
    task_uuid = %task_uuid,
    namespace = %namespace
))]
```

**Target Areas**:
1. **Task Initialization** (`task_initializer.rs`)
   - `create_and_enqueue_task_from_request()`
   - `validate_and_create_task()`
   - `enqueue_initial_steps()`

2. **Step Discovery & Enqueuing** (`step_enqueuer_service.rs`)
   - `discover_ready_steps()`
   - `enqueue_steps_batch()`
   - SQL function calls with timing

3. **Result Processing** (`step_result_processor.rs`)
   - `process_single_step_result()`
   - `handle_step_completion()`
   - `trigger_next_steps()`

4. **Task Finalization** (`task_finalizer.rs`)
   - `check_task_finalization_ready()`
   - `finalize_task()`
   - `transition_to_terminal_state()`

### 2.2 Worker Hot Paths
1. **Event System** (`worker/event_systems/`)
   - Message polling loops
   - Event notification handlers
   - Queue claim operations

2. **Step Execution** (`worker/step_executor.rs`)
   - `execute_step_with_context()`
   - FFI handler invocation
   - Result marshaling

3. **Message Handling** (`worker/message_processor.rs`)
   - Queue message reading
   - Step deserialization
   - Result submission

### 2.3 SQL Function Instrumentation
**Add timing spans around**:
- `get_next_ready_tasks()` calls
- `get_step_readiness_status()` calls
- `transition_task_state_atomic()` calls
- `claim_task_for_finalization()` calls

---

## Phase 3: OpenTelemetry Integration (Week 3)

### 3.1 OTLP Exporter Setup
**Configuration** (`config/tasker/base/telemetry.toml`):
```toml
[telemetry]
enabled = true
service_name = "tasker-orchestration"
service_version = "0.1.0"

[telemetry.otlp]
endpoint = "http://localhost:4317"
protocol = "grpc"  # or "http/protobuf"
timeout_seconds = 10
batch_size_limit = 512

[telemetry.sampling]
strategy = "parent_based_always_on"  # or "trace_id_ratio"
ratio = 1.0  # 100% in dev, 0.1 in prod

[telemetry.spans]
max_attributes = 128
max_events = 128
max_links = 128

[telemetry.metrics]
enabled = true
export_interval_seconds = 60
```

**Environment Overrides**:
- `config/tasker/environments/production/telemetry.toml`
- Lower sampling ratio (0.1 = 10%)
- Production OTLP endpoint

### 3.2 Tracing Subscriber Setup
**Initialize in `main.rs`** of orchestration and worker:
```rust
use opentelemetry::global;
use opentelemetry_otlp::WithExportConfig;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn setup_telemetry(config: &TelemetryConfig) -> Result<()> {
    // Create OTLP tracer
    let tracer = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(&config.otlp_endpoint)
        )
        .with_trace_config(
            opentelemetry_sdk::trace::config()
                .with_sampler(config.sampler())
                .with_resource(Resource::new(vec![
                    KeyValue::new("service.name", config.service_name.clone()),
                    KeyValue::new("service.version", config.service_version.clone()),
                ]))
        )
        .install_batch(opentelemetry_sdk::runtime::Tokio)?;

    // Layer stack: console, file, OTLP
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .init();

    Ok(())
}
```

### 3.3 Metrics Collection

**Status**: ✅ **COMPLETE** (2025-10-08)

**Implementation Details**:
- 39 metrics defined across 4 domains (orchestration, worker, database, messaging)
- 21 metrics fully instrumented and verified (orchestration + worker hot paths)
- 18 metrics defined but not yet instrumented (database + messaging layers)
- 60-second OTLP export interval to Grafana LGTM stack
- Correlation IDs included in all metric labels

**Verified Metrics**:

**Orchestration (11 metrics)**:
- ✅ `tasker_tasks_requests_total` - Task creation counter
- ✅ `tasker_tasks_completions_total` - Successful completions
- ✅ `tasker_tasks_failures_total` - Failed tasks
- ✅ `tasker_steps_enqueued_total` - Steps sent to workers
- ✅ `tasker_step_results_processed_total` - Results processed
- ✅ `tasker_task_initialization_duration_milliseconds_*` - Init timing histogram
- ✅ `tasker_task_finalization_duration_milliseconds_*` - Finalization timing histogram
- ✅ `tasker_step_result_processing_duration_milliseconds_*` - Result processing histogram
- ⚠️ `tasker_tasks_active` - Gauge (defined, not instrumented)
- ⚠️ `tasker_steps_ready` - Gauge (defined, not instrumented)

**Worker (10 metrics)**:
- ✅ `tasker_steps_executions_total` - Execution attempts
- ✅ `tasker_steps_successes_total` - Successful executions
- ✅ `tasker_steps_failures_total` - Failed executions (with error_type labels)
- ✅ `tasker_steps_claimed_total` - Claims from queue (event vs poll)
- ✅ `tasker_steps_results_submitted_total` - Results sent to orchestration
- ✅ `tasker_step_execution_duration_milliseconds_*` - Execution timing histogram
- ✅ `tasker_step_claim_duration_milliseconds_*` - Claim timing histogram
- ✅ `tasker_step_result_submission_duration_milliseconds_*` - Submission timing histogram
- ⚠️ `tasker_steps_active_executions` - Gauge (defined, not actively tracked)
- ⚠️ `tasker_queue_depth` - Gauge (defined, not instrumented)

**Database (7 metrics)**: ⚠️ Defined, not yet instrumented
**Messaging (11 metrics)**: ⚠️ Defined, not yet instrumented

**Key Learning**: OpenTelemetry automatically appends `_milliseconds` to histogram metrics when unit "ms" is specified. Both instant and rate-based query patterns documented.

**Verification**:
- Test task: `mathematical_sequence` (rust_e2e_linear namespace)
- Correlation ID: `0199c3e0-ccdb-7581-87ab-3f67daeaa4a5`
- Trace ID: `d640f82572e231322edba0a5ef6e1405`
- All counters and histograms returning expected data

**Documentation**:
- `docs/observability/metrics-reference.md` - Complete metrics reference
- `docs/observability/metrics-verification.md` - Verification checklist
- `docs/observability/VERIFICATION_RESULTS.md` - Live system test results

**Original Specification** (for reference):
```rust
use opentelemetry::metrics::{Counter, Histogram, ObservableGauge};

// Counters
task_requests_total: Counter<u64>
task_completions_total: Counter<u64>
step_executions_total: Counter<u64>
step_failures_total: Counter<u64>

// Histograms (for distributions)
task_initialization_duration: Histogram<f64>
step_execution_duration: Histogram<f64>
sql_query_duration: Histogram<f64>
message_latency: Histogram<f64>

// Gauges (for snapshots)
active_tasks: ObservableGauge<u64>
queue_depth: ObservableGauge<i64>
ready_steps: ObservableGauge<i64>
```

---

## Phase 4: Ruby FFI Tracing Bridge (Week 4)

### 4.1 Rust Tracing FFI Exports
**New module**: `workers/ruby/ext/tasker_core/src/tracing.rs`

```rust
#[magnus::wrap(class = "TaskerCore::Tracing")]
struct RubyTracing;

impl RubyTracing {
    fn trace_span(level: String, name: String, fields: HashMap<String, Value>) {
        // Create Rust span from Ruby
        let span = match level.as_str() {
            "debug" => tracing::debug_span!(&name),
            "info" => tracing::info_span!(&name),
            "warn" => tracing::warn_span!(&name),
            "error" => tracing::error_span!(&name),
            _ => tracing::info_span!(&name),
        };

        // Add fields to span
        for (key, value) in fields {
            span.record(key, value);
        }

        span.in_scope(|| {
            // Ruby block will execute in this span
        });
    }

    fn log_event(level: String, message: String, fields: HashMap<String, Value>) {
        // Emit structured log event
    }
}
```

### 4.2 Ruby Logger Replacement
**Deprecate**: `workers/ruby/lib/tasker_core/logger.rb`
**Replace with**: `workers/ruby/lib/tasker_core/tracing.rb`

```ruby
module TaskerCore
  module Tracing
    class << self
      def span(level, name, **fields)
        # Delegate to Rust FFI tracing
        Internal::Tracing.trace_span(level.to_s, name, fields) do
          yield if block_given?
        end
      end

      def event(level, message, **fields)
        Internal::Tracing.log_event(level.to_s, message, fields.merge(
          correlation_id: current_correlation_id,
          component: "ruby_worker"
        ))
      end

      # Convenience methods
      def debug(message, **fields) = event(:debug, message, **fields)
      def info(message, **fields) = event(:info, message, **fields)
      def warn(message, **fields) = event(:warn, message, **fields)
      def error(message, **fields) = event(:error, message, **fields)

      private

      def current_correlation_id
        Thread.current[:correlation_id]
      end
    end
  end
end
```

### 4.3 Handler Context Enhancement
**Add to handler execution context**:
```ruby
class BaseHandler
  attr_reader :correlation_id, :task_uuid, :step_uuid

  def execute(context)
    Thread.current[:correlation_id] = context[:correlation_id]

    Tracing.span(:info, "handler_execution",
      handler: self.class.name,
      correlation_id: correlation_id,
      task_uuid: task_uuid,
      step_uuid: step_uuid
    ) do
      perform(context)
    end
  end
end
```

---

## Phase 5: Benchmarking Infrastructure (Week 5-6)

### 5.1 Test Data Generation Framework
**New crate**: `tasker-benchmarks/`

```rust
// benchmarks/data_generator.rs
pub struct WorkflowDataGenerator {
    pool: PgPool,
    config: GeneratorConfig,
}

pub struct GeneratorConfig {
    task_count: usize,
    avg_steps_per_task: usize,
    step_count_variance: f64,
    namespace_count: usize,
    complexity_distribution: ComplexityDistribution,
}

pub enum ComplexityDistribution {
    Linear,      // Simple linear workflows
    Diamond,     // 2-path convergence
    Tree,        // Fan-out patterns
    MixedDAG,    // Complex multi-convergence
}

impl WorkflowDataGenerator {
    pub async fn generate_dataset(&self) -> Result<GeneratedDataset> {
        // Create namespaces
        let namespaces = self.create_namespaces().await?;

        // Create task templates with varied complexity
        let templates = self.create_task_templates(&namespaces).await?;

        // Create tasks in various states
        let tasks = self.create_tasks_distribution(&templates).await?;

        Ok(GeneratedDataset {
            namespaces,
            templates,
            tasks,
            statistics: self.calculate_statistics(&tasks),
        })
    }

    async fn create_tasks_distribution(&self, templates: &[NamedTask]) -> Result<Vec<Task>> {
        // Distribution:
        // - 60% complete
        // - 20% in-progress (various states)
        // - 10% waiting_for_retry
        // - 5% error
        // - 5% pending
    }
}
```

### 5.2 SQL Query Benchmarking Suite
**New module**: `benchmarks/sql_queries.rs`

```rust
use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};

pub struct SqlBenchmarkSuite {
    pool: PgPool,
    dataset: GeneratedDataset,
}

impl SqlBenchmarkSuite {
    // Benchmark 1: get_next_ready_tasks performance
    pub fn bench_get_next_ready_tasks(&self, c: &mut Criterion) {
        let mut group = c.benchmark_group("get_next_ready_tasks");

        for task_count in [100, 1000, 10000, 50000] {
            group.bench_with_input(
                BenchmarkId::from_parameter(task_count),
                &task_count,
                |b, &count| {
                    b.to_async(&self.runtime).iter(|| async {
                        sqlx::query!(...)
                            .fetch_all(&self.pool)
                            .await
                    });
                }
            );
        }
        group.finish();
    }

    // Benchmark 2: get_step_readiness_status
    pub fn bench_step_readiness(&self, c: &mut Criterion) {
        let mut group = c.benchmark_group("step_readiness");

        for complexity in [Linear, Diamond, Tree, MixedDAG] {
            group.bench_with_input(
                BenchmarkId::from_parameter(complexity),
                &complexity,
                |b, complexity| {
                    let task_uuid = self.dataset.get_task_by_complexity(complexity);
                    b.to_async(&self.runtime).iter(|| async {
                        sqlx::query!(
                            "SELECT * FROM get_step_readiness_status($1)",
                            task_uuid
                        )
                        .fetch_all(&self.pool)
                        .await
                    });
                }
            );
        }
    }

    // Benchmark 3: State transition atomicity
    pub fn bench_state_transitions(&self, c: &mut Criterion) {
        // Measure contention under concurrent transitions
    }
}
```

### 5.3 EXPLAIN ANALYZE Integration
**New module**: `benchmarks/query_analysis.rs`

```rust
pub struct QueryAnalyzer {
    pool: PgPool,
}

pub struct QueryPlan {
    query: String,
    plan: String,
    execution_time_ms: f64,
    planning_time_ms: f64,
    rows_returned: i64,
    cost: QueryCost,
}

pub struct QueryCost {
    startup_cost: f64,
    total_cost: f64,
}

impl QueryAnalyzer {
    pub async fn analyze_query(&self, query: &str, params: &[&dyn ToSql]) -> Result<QueryPlan> {
        let explain_query = format!("EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) {}", query);

        let plan_json: serde_json::Value = sqlx::query_scalar(&explain_query)
            .bind_all(params)
            .fetch_one(&self.pool)
            .await?;

        // Parse and structure the plan
        Ok(self.parse_plan(plan_json)?)
    }

    pub async fn generate_report(&self) -> Result<AnalysisReport> {
        let queries = vec![
            ("get_next_ready_tasks", "SELECT * FROM get_next_ready_tasks($1, $2)"),
            ("step_readiness", "SELECT * FROM get_step_readiness_status($1)"),
            ("task_finalization", "SELECT * FROM can_finalize_task($1)"),
        ];

        let mut plans = Vec::new();
        for (name, query) in queries {
            plans.push((name, self.analyze_query(query, &[]).await?));
        }

        Ok(AnalysisReport { plans })
    }
}
```

### 5.4 End-to-End Latency Benchmarks
**Benchmark Suite Structure**:

1. **API → Task Creation** (`bench_task_initialization.rs`)
   - Measure: TaskRequest parsing → Task UUID return
   - Target: < 50ms p99

2. **Step Enqueuing** (`bench_step_enqueueing.rs`)
   - Measure: Ready step discovery → Queue publish with notify
   - Target: < 100ms for 10-step workflow

3. **Worker Processing** (`bench_worker_execution.rs`)
   - Measure: Queue claim → Handler execution → Result submit
   - Target: < 200ms (excluding business logic)

4. **Event Latency** (`bench_event_propagation.rs`)
   - Measure: pg_notify publish → listener reception
   - Target: < 10ms p95

5. **SQL Function Performance** (`bench_sql_functions.rs`)
   - Individual function timing under various loads
   - EXPLAIN ANALYZE output capture

6. **Business Logic Isolation** (`bench_handler_overhead.rs`)
   - FFI overhead measurement
   - Framework overhead vs pure handler time

---

## Phase 6: Structured Logging Coherence (Week 7)

### 6.1 Log Level Standardization
**Guidelines**:
- `ERROR`: Unrecoverable failures requiring intervention
- `WARN`: Degraded operation, retryable failures, unusual conditions
- `INFO`: Lifecycle events, state transitions, significant operations
- `DEBUG`: Detailed diagnostic info, parameter values
- `TRACE`: Very verbose, hot-path entry/exit

### 6.2 Structured Field Conventions
**Required fields in all logs**:
```rust
tracing::info!(
    correlation_id = %correlation_id,  // Always present
    task_uuid = %task_uuid,            // If applicable
    step_uuid = %step_uuid,            // If applicable
    namespace = %namespace,            // If applicable
    operation = "step_execution",      // Operation identifier
    duration_ms = duration.as_millis(), // For timed operations
    "Message text"
);
```

### 6.3 Lifecycle Event Coherence
**Standard lifecycle events**:
- `task.initialized`
- `task.steps_enqueued`
- `task.evaluation_started`
- `task.finalization_ready`
- `task.completed`
- `task.failed`
- `step.enqueued`
- `step.claimed`
- `step.executing`
- `step.completed`
- `step.failed`
- `step.retry_scheduled`

---

## Implementation Order

**Week 1**: Correlation IDs (Phase 1)
**Week 2**: Rust Tracing Instrumentation (Phase 2)
**Week 3**: OpenTelemetry Setup (Phase 3)
**Week 4**: Ruby FFI Bridge (Phase 4)
**Week 5-6**: Benchmarking Infrastructure (Phase 5)
**Week 7**: Logging Coherence (Phase 6)

---

## Success Criteria

✅ Correlation IDs propagate through entire request lifecycle
✅ All hot paths instrumented with tracing spans
✅ OpenTelemetry exports to Jaeger/Tempo
✅ Ruby handlers use Rust tracing (no Ruby Logger)
✅ Benchmark suite generates realistic test data
✅ SQL EXPLAIN ANALYZE integrated into benchmarks
✅ E2E latency measured for all 7 critical paths
✅ Structured logging follows consistent conventions
✅ Log levels used appropriately throughout
✅ Metrics dashboards available (Grafana)

---

## Files to Create

**New**:
- `migrations/20250000_add_correlation_ids.sql`
- `config/tasker/base/telemetry.toml`
- `config/tasker/environments/*/telemetry.toml`
- `tasker-benchmarks/` (new crate)
- `workers/ruby/ext/tasker_core/src/tracing.rs`
- `workers/ruby/lib/tasker_core/tracing.rb`

**Modified**:
- `tasker-shared/src/models/core/task_request.rs`
- `tasker-shared/src/messaging/*.rs` (all message types)
- All `*_processor.rs` files (add instrumentation)
- `main.rs` in orchestration and worker (telemetry setup)
- All Ruby handlers (use new Tracing module)

---

This plan provides production-grade observability while enabling data-driven performance optimization through comprehensive benchmarking.
