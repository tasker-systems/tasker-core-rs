# TAS-65: Observability Configuration Integration

**Purpose**: Integration notes for TAS-65 observability with existing configuration system and dynamic namespace handling.

## Configuration System Integration

### Existing Configuration Architecture

From `docs/configuration-management.md`, Tasker uses **component-based TOML** with environment overrides:

```
config/tasker/
├── base/
│   ├── common.toml          # Shared: database, circuit_breakers, telemetry
│   ├── orchestration.toml
│   └── worker.toml
└── environments/
    ├── test/common.toml
    ├── development/common.toml
    └── production/common.toml
```

**Key Points**:
- `telemetry` configuration already exists in `common.toml` (shared across orchestration and worker)
- Environment overrides use **partial merging** (environment values win)
- Runtime loading via `TASKER_CONFIG_PATH` (single merged file for Docker/K8s)

### TAS-65 Telemetry Configuration Location

**Add to existing telemetry section in `config/tasker/base/common.toml`**:

```toml
# Existing telemetry configuration
[telemetry]
enabled = true
log_level = "info"
service_name = "tasker-core"
exporter_endpoint = "${OTEL_EXPORTER_OTLP_ENDPOINT:-http://localhost:4317}"

# TAS-65: OpenTelemetry tracing for system events
[telemetry.tracing]
enabled = true
sampler = "parent_based"
sample_rate = 1.0

# TAS-65: Metrics configuration with namespace-aware sampling
[telemetry.metrics]
enabled = true

# Detailed metrics (high cardinality)
[telemetry.metrics.detailed]
enabled = true
sample_rate = 1.0  # Global default

# Namespace-specific sampling overrides (optional)
# NOTE: Namespaces are dynamically registered by workers at boot
# These overrides are optional - missing namespaces use global sample_rate
[telemetry.metrics.namespaces]
# Example overrides (workers may register namespaces not listed here)
# payments = 1.0      # 100% sampling for critical workflows
# analytics = 0.01    # 1% sampling for high-volume analytics

# Spans for internal operations
[telemetry.spans.internal]
enabled = false  # Disabled by default
```

### Production Environment Override

**`config/tasker/environments/production/common.toml`**:

```toml
[telemetry.tracing]
sample_rate = 0.1  # Only trace 10% of requests in production

[telemetry.metrics.detailed]
enabled = false  # Disable high-cardinality metrics by default

# Production namespace-specific overrides
[telemetry.metrics.namespaces]
payments = 1.0        # Critical: always sample
orders = 0.5          # Medium priority
analytics = 0.01      # High volume: 1% sample
integrations = 0.1    # Debug third-party issues
```

---

## Dynamic Namespace Handling

### The Challenge

From `tasker-shared/src/registry/task_handler_registry.rs`:
- **Workers dynamically register namespaces** at boot when discovering task templates
- **Orchestration does not police namespaces** - any namespace can exist
- Telemetry config **cannot enumerate all namespaces** at build time

### Solution: Graceful Fallback

**Configuration Strategy**:
```rust
pub struct NamespaceAwareSampler {
    config: Arc<TelemetryConfig>,
}

impl NamespaceAwareSampler {
    pub fn should_sample_detailed(&self, namespace: &str) -> bool {
        // 1. Check namespace-specific override (if configured)
        let sample_rate = self.config
            .metrics
            .namespaces
            .get(namespace)
            .copied()
            // 2. Fallback to global default if namespace not in config
            .unwrap_or(self.config.metrics.detailed.sample_rate);
        
        rand::thread_rng().gen::<f64>() < sample_rate
    }
}
```

**Key Property**: New namespaces registered by workers automatically use global `sample_rate` until explicitly overridden.

### Avoiding N+1 Queries for Namespace Resolution

**Problem**: Getting namespace for telemetry shouldn't cause N+1 queries.

**Solution 1: Use `Task::for_orchestration()`** (already exists):

```rust
// From tasker-shared/src/models/core/task.rs
pub struct TaskForOrchestration {
    pub task: Task,
    pub task_name: String,
    pub task_version: String,
    pub namespace_name: String,  // ✅ Includes namespace name
}

// Single query that joins everything
impl Task {
    pub async fn for_orchestration(
        pool: &PgPool,
        task_uuid: Uuid,
    ) -> Result<TaskForOrchestration, sqlx::Error> {
        // Single JOIN query fetches task + named_task + namespace
        // No N+1 problem
    }
}
```

**Usage in Telemetry**:
```rust
// When publishing metrics/spans, use for_orchestration()
let task_info = Task::for_orchestration(&pool, task_uuid).await?;

// Now have namespace name without extra query
event!(
    Level::INFO,
    "task.state_transition",
    task_uuid = %task_info.task.task_uuid,
    correlation_id = %task_info.task.correlation_id,
    namespace = %task_info.namespace_name,  // ✅ No extra query
    // ...
);

// Namespace-aware sampling
if sampler.should_sample_detailed(&task_info.namespace_name) {
    metrics::histogram!(
        "tasker.task.duration.seconds",
        duration.as_secs_f64(),
        "namespace" => task_info.namespace_name,
        "task_name" => task_info.task_name
    );
}
```

**Solution 2: Cache Namespace Name in Context**:

If repeatedly instrumenting the same task, cache namespace in execution context:

```rust
pub struct ExecutionContext {
    pub task_uuid: Uuid,
    pub correlation_id: Uuid,
    pub namespace_name: String,  // ✅ Cached from initial query
    // ... other fields
}

// Initialize once per task processing
let context = ExecutionContext::fetch(task_uuid).await?;  // Single query

// Reuse throughout task lifecycle
event!(Level::INFO, "step.execution", namespace = %context.namespace_name);
```

---

## Rust Configuration Types

**Telemetry Config Rust Structure** (`tasker-shared/src/config/telemetry.rs`):

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryConfig {
    pub enabled: bool,
    pub log_level: String,
    pub service_name: String,
    pub exporter_endpoint: String,
    pub tracing: TracingConfig,
    pub metrics: MetricsConfig,
    pub spans: SpansConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingConfig {
    pub enabled: bool,
    pub sampler: String,
    pub sample_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub detailed: DetailedMetricsConfig,
    #[serde(default)]
    pub namespaces: HashMap<String, f64>,  // Optional namespace overrides
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedMetricsConfig {
    pub enabled: bool,
    pub sample_rate: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpansConfig {
    pub enabled: bool,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            log_level: "info".to_string(),
            service_name: "tasker-core".to_string(),
            exporter_endpoint: "http://localhost:4317".to_string(),
            tracing: TracingConfig {
                enabled: true,
                sampler: "parent_based".to_string(),
                sample_rate: 1.0,
            },
            metrics: MetricsConfig {
                enabled: true,
                detailed: DetailedMetricsConfig {
                    enabled: true,
                    sample_rate: 1.0,
                },
                namespaces: HashMap::new(),  // Empty by default
            },
            spans: SpansConfig { enabled: false },
        }
    }
}
```

**Integration with SystemContext**:

```rust
// tasker-shared/src/system_context.rs
pub struct SystemContext {
    pub tasker_config: TaskerConfig,
    // ... other fields
}

// Access telemetry config
let telemetry_config = &context.tasker_config.common.telemetry;
let sampler = NamespaceAwareSampler::new(telemetry_config.clone());

// Use in instrumentation
if sampler.should_sample_detailed(&namespace) {
    // Emit detailed metrics
}
```

---

## Configuration Best Practices

### 1. Namespace Overrides Are Optional

**Good Configuration** (minimal overrides):
```toml
[telemetry.metrics]
enabled = true

[telemetry.metrics.detailed]
sample_rate = 0.1  # Global 10% sampling

[telemetry.metrics.namespaces]
payments = 1.0  # Only override critical namespaces
```

**Anti-Pattern** (exhaustive overrides):
```toml
# ❌ DON'T DO THIS - trying to enumerate all namespaces
[telemetry.metrics.namespaces]
namespace1 = 1.0
namespace2 = 0.5
namespace3 = 0.1
# ... 50 more namespaces
# Unmaintainable and defeats dynamic registration
```

### 2. Global Defaults for Most Namespaces

**Philosophy**: Most namespaces use global `sample_rate`. Only override for:
- **Critical workflows** (payments, orders) → 100% sampling
- **High-volume analytics** (reporting, metrics) → 1-10% sampling
- **Debug scenarios** (specific namespace investigation) → temporary 100% override

### 3. Environment-Specific Tuning

**Development** (full visibility):
```toml
# config/tasker/environments/development/common.toml
[telemetry.metrics.detailed]
sample_rate = 1.0  # See everything in development

[telemetry.metrics.namespaces]
# No overrides needed - use global 100%
```

**Production** (cost-conscious):
```toml
# config/tasker/environments/production/common.toml
[telemetry.metrics.detailed]
sample_rate = 0.01  # 1% global sampling to reduce costs

[telemetry.metrics.namespaces]
payments = 1.0      # But always sample critical paths
orders = 1.0
```

---

## Runtime Configuration Inspection

### Check Active Sampling Configuration

**Via `/config` Endpoint**:
```bash
# Orchestration config (includes telemetry)
curl http://localhost:8080/config | jq '.common.telemetry'

# Output shows active namespace sampling config:
{
  "enabled": true,
  "metrics": {
    "enabled": true,
    "detailed": {
      "enabled": true,
      "sample_rate": 0.1
    },
    "namespaces": {
      "payments": 1.0,
      "analytics": 0.01
    }
  }
}
```

### Verify Namespace Registration

**Query Database for Active Namespaces**:
```sql
-- See all registered namespaces
SELECT name, description, created_at 
FROM tasker_task_namespaces 
ORDER BY created_at DESC;
```

**Compare to Telemetry Config**:
```bash
# Get namespaces from DB
psql $DATABASE_URL -c "SELECT name FROM tasker_task_namespaces;"

# Get namespaces from telemetry config
curl -s http://localhost:8080/config | jq '.common.telemetry.metrics.namespaces | keys[]'

# Diff shows namespaces using global sample_rate (expected)
```

---

## Migration Path

### Phase 1: Add Base Configuration (Week 1)

**Files to Modify**:
- `config/tasker/base/common.toml` - Add telemetry.tracing, telemetry.metrics sections
- `tasker-shared/src/config/telemetry.rs` - Add new config types
- `tasker-shared/src/config/mod.rs` - Export new types

**Testing**:
```bash
# Validate config loads correctly
cargo run --bin config-validator

# Check via API
curl http://localhost:8080/config | jq '.common.telemetry'
```

### Phase 2: Implement Namespace-Aware Sampler (Week 2)

**Files to Create**:
- `tasker-shared/src/observability/sampler.rs` - NamespaceAwareSampler implementation
- `tasker-shared/src/observability/mod.rs` - Module exports

**Integration Points**:
- State machine actions (use for system events)
- Event consumer (use for domain events)
- Step execution (use for detailed step metrics)

### Phase 3: Production Tuning (Week 3)

**Environment Overrides**:
- `config/tasker/environments/production/common.toml` - Add namespace-specific overrides
- Monitor metric cardinality in Prometheus
- Adjust sample rates based on actual volume

---

## Summary

**Key Decisions**:
1. ✅ Telemetry config in `common.toml` (shared across orchestration and worker)
2. ✅ Namespace overrides are **optional** - missing namespaces use global `sample_rate`
3. ✅ Use `Task::for_orchestration()` to avoid N+1 queries for namespace name
4. ✅ Cache namespace in `ExecutionContext` for repeated instrumentation
5. ✅ Production override reduces global sample rate, increases for critical namespaces

**No Breaking Changes**:
- Existing telemetry config structure unchanged
- New fields have sensible defaults
- Missing namespace overrides fallback gracefully

**Operational Benefits**:
- Workers dynamically register namespaces without config changes
- Operators tune sampling via config overrides only when needed
- `/config` endpoint shows active sampling configuration
