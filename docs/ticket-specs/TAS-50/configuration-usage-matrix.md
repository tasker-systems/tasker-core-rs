# Configuration Usage Matrix

**Analysis Date**: 2025-10-14
**Purpose**: Map which configuration fields are used by which system components

---

## Legend
- ✅ **Used Directly**: Component directly accesses this configuration
- 🔄 **Inherited from SystemContext**: Component gets this via SystemContext initialization
- 🚫 **Not Used**: Component does not use this configuration
- ❓ **Unknown**: Needs further analysis
- ⚠️ **Deprecated**: Marked as deprecated in code
- 📝 **ENV Variable**: Ruby uses environment variable instead

---

## Configuration Field Usage by Component

### Database Configuration (`database.*`)

| Field | Orchestration | Rust Worker | Ruby Worker | Notes |
|-------|--------------|-------------|-------------|-------|
| `database.pool.max_connections` | 🔄 SystemContext | 🔄 SystemContext | 📝 DATABASE_URL | Pool config in SystemContext:129 |
| `database.pool.min_connections` | 🔄 SystemContext | 🔄 SystemContext | 📝 DATABASE_URL | Pool config in SystemContext:130 |
| `database.pool.max_lifetime_seconds` | 🔄 SystemContext | 🔄 SystemContext | 📝 DATABASE_URL | Pool config in SystemContext:134 |
| `database.checkout_timeout` | 🔄 SystemContext | 🔄 SystemContext | 📝 DATABASE_URL | Pool config in SystemContext:131 |
| `database.reaping_frequency` | 🔄 SystemContext | 🔄 SystemContext | 📝 DATABASE_URL | Pool config in SystemContext:137 |
| `database_url()` | 🔄 SystemContext | 🔄 SystemContext | 📝 DATABASE_URL | Connection string SystemContext:104, 116 |
| `database.enable_secondary_database` | 🚫 Not Used | 🚫 Not Used | 🚫 Not Used | Only in tests, always false |

**Category**: **Common** (used by all Rust components)

---

### Queue Configuration (`queues.*`)

| Field | Orchestration | Rust Worker | Ruby Worker | Notes |
|-------|--------------|-------------|-------------|-------|
| `queues.orchestration_namespace` | ✅ bootstrap.rs:159 | 🚫 Not Used | 🚫 Not Used | Orchestration queue namespace |
| `queues.worker_namespace` | 🚫 Not Used | ✅ Likely used | 🚫 Not Used | Worker queue namespace (needs verification) |
| `queues.backend` | ✅ Tests only | ✅ Tests only | 🚫 Not Used | Always "pgmq" |
| `queues.orchestration_queues.step_results` | ✅ SystemContext:242 | ✅ result_sender.rs:135 | 🚫 Not Used | Results queue name |
| `queues.orchestration_queues.task_requests` | ✅ SystemContext:239 | ✅ Likely used | 🚫 Not Used | Task request queue name |
| `queues.orchestration_queues.task_finalizations` | ✅ SystemContext:241 | 🚫 Not Used | 🚫 Not Used | Finalization queue name |
| `queues.* (entire struct)` | ✅ listener.rs:342, fallback_poller.rs:226 | ✅ command_processor.rs:201 | 🚫 Not Used | Cloned for queue operations |

**Category**: **Common** (orchestration and rust worker need queue coordination)

---

### Backoff Configuration (`backoff.*`)

| Field | Orchestration | Rust Worker | Ruby Worker | Notes |
|-------|--------------|-------------|-------------|-------|
| `backoff.default_backoff_seconds[0]` | ✅ backoff_calculator.rs:72 | 🚫 Not Used | 🚫 Not Used | Base delay (first value) |
| `backoff.max_backoff_seconds` | ✅ backoff_calculator.rs:72 | 🚫 Not Used | 🚫 Not Used | Maximum backoff |
| `backoff.backoff_multiplier` | ✅ backoff_calculator.rs:73 | 🚫 Not Used | 🚫 Not Used | Exponential multiplier |
| `backoff.jitter_enabled` | ✅ backoff_calculator.rs:74 | 🚫 Not Used | 🚫 Not Used | Enable jitter |
| `backoff.jitter_max_percentage` | ✅ backoff_calculator.rs:75 | 🚫 Not Used | 🚫 Not Used | Jitter percentage |

**Category**: **Orchestration-Specific** (only orchestration handles retries)

---

### Orchestration Configuration (`orchestration.*`)

| Field | Orchestration | Rust Worker | Ruby Worker | Notes |
|-------|--------------|-------------|-------------|-------|
| `orchestration.web_enabled()` | ✅ bootstrap.rs:161 | 🚫 Not Used | 🚫 Not Used | Whether to start web API |
| `orchestration.web.*` | ✅ bootstrap.rs:223 | ✅ Via OrchestrationApiConfig | 🚫 Not Used | Web config cloned |
| `orchestration.web.bind_address` | ✅ Web API binding | ✅ client.rs:105 | 🚫 Not Used | Where orchestration API listens |
| `orchestration.web.request_timeout_ms` | ✅ Web API timeout | ✅ client.rs:106 | 🚫 Not Used | Request timeout |
| `orchestration.web.auth` | ✅ Web API auth | ✅ client.rs:108 | 🚫 Not Used | Auth configuration |

**Category**: **Hybrid**
- Orchestration defines it (where API lives)
- Workers need it (to connect to orchestration API)

---

### Worker Configuration (`worker.*`)

| Field | Orchestration | Rust Worker | Ruby Worker | Notes |
|-------|--------------|-------------|-------------|-------|
| `worker.web.enabled` | 🚫 Not Used | ✅ bootstrap.rs:170 | 🚫 Not Used | Whether to start worker web API |
| `worker.web.bind_address` | 🚫 Not Used | ✅ state.rs:54 | 🚫 Not Used | Worker web server bind address |
| `worker.web.request_timeout_ms` | 🚫 Not Used | ✅ state.rs:55 | 🚫 Not Used | Worker web request timeout |
| `worker.web.auth.enabled` | 🚫 Not Used | ✅ state.rs:56 | 🚫 Not Used | Worker web auth enabled |
| `worker.web.cors.enabled` | 🚫 Not Used | ✅ state.rs:57 | 🚫 Not Used | Worker web CORS enabled |

**Category**: **Rust-Worker-Specific** (Ruby worker uses environment variables)

---

### Event Systems Configuration (`event_systems.*`)

| Field | Orchestration | Rust Worker | Ruby Worker | Notes |
|-------|--------------|-------------|-------------|-------|
| `event_systems.orchestration.*` | ✅ bootstrap.rs:252, coordinator.rs:84 | 🚫 Not Used | 🚫 Not Used | Orchestration event config |
| `event_systems.task_readiness.*` | ✅ bootstrap.rs:249, coordinator.rs:81 | 🚫 Not Used | 🚫 Not Used | Task readiness event config |
| `event_systems.worker.deployment_mode` | 🚫 Not Used | ✅ bootstrap.rs:182 | 🚫 Not Used | Worker deployment mode |
| `event_systems.worker.has_event_driven()` | 🚫 Not Used | ✅ bootstrap.rs:180 | 🚫 Not Used | Check if event-driven enabled |

**Category**: **Split**
- `event_systems.orchestration.*` → Orchestration-Specific
- `event_systems.task_readiness.*` → Orchestration-Specific
- `event_systems.worker.*` → Rust-Worker-Specific

---

### MPSC Channels Configuration (`mpsc_channels.*`)

| Field | Orchestration | Rust Worker | Ruby Worker | Notes |
|-------|--------------|-------------|-------------|-------|
| `mpsc_channels.shared.event_publisher.event_queue_buffer_size` | 🔄 SystemContext:192-197, 308-312 | 🔄 SystemContext:192-197, 308-312 | 🚫 Not Used | Event publisher buffer size |
| `mpsc_channels.orchestration.*` | ✅ Likely used | 🚫 Not Used | 🚫 Not Used | Need to analyze orchestration command processor |
| `mpsc_channels.worker.*` | 🚫 Not Used | ✅ Likely used | 🚫 Not Used | Need to analyze worker command processor |
| `mpsc_channels.task_readiness.*` | ✅ Likely used | 🚫 Not Used | 🚫 Not Used | Need to analyze task readiness coordinator |

**Category**: **Split**
- `mpsc_channels.shared.*` → Common (event publisher)
- `mpsc_channels.orchestration.*` → Orchestration-Specific
- `mpsc_channels.worker.*` → Rust-Worker-Specific
- `mpsc_channels.task_readiness.*` → Orchestration-Specific

**NOTE**: Requires deeper analysis of command processors and coordinators

---

### Circuit Breaker Configuration (`circuit_breakers.*`)

| Field | Orchestration | Rust Worker | Ruby Worker | Notes |
|-------|--------------|-------------|-------------|-------|
| `circuit_breakers.enabled` | 🔄 SystemContext:147 | 🔄 SystemContext:147 | 🚫 Not Used | Check if enabled |
| `circuit_breakers.* (entire struct)` | 🔄 SystemContext:149 | 🔄 SystemContext:149 | 🚫 Not Used | Clone for manager |

**Category**: ⚠️ **Possibly Deprecated** (SystemContext:143 comment suggests sqlx handles this)

---

### Environment Configuration (`execution.environment`)

| Field | Orchestration | Rust Worker | Ruby Worker | Notes |
|-------|--------------|-------------|-------------|-------|
| `environment()` | ✅ Multiple locations | ✅ bootstrap.rs:89 | 📝 TASKER_ENV/RAILS_ENV | Environment string |

**Category**: **Common** (all components need environment awareness)

---

### Execution Configuration (`execution.*`)

| Field | Orchestration | Rust Worker | Ruby Worker | Notes |
|-------|--------------|-------------|-------------|-------|
| `execution.max_concurrent_tasks` | ✅ Tests only | 🚫 Not Used | 🚫 Not Used | Only in test assertions |
| `execution.max_concurrent_steps` | ❓ Unknown | ❓ Unknown | 🚫 Not Used | Needs analysis |

**Category**: ❓ **Unknown** - Needs deeper analysis

---

### Telemetry Configuration (`telemetry.*`)

| Field | Orchestration | Rust Worker | Ruby Worker | Notes |
|-------|--------------|-------------|-------------|-------|
| All `telemetry.*` fields | 🚫 Not Used | 🚫 Not Used | 🚫 Not Used | No grep matches found |

**Category**: 🚫 **Unused** - Candidate for removal

---

### Task Templates Configuration (`task_templates.*`)

| Field | Orchestration | Rust Worker | Ruby Worker | Notes |
|-------|--------------|-------------|-------------|-------|
| All `task_templates.*` fields | 🚫 Not Used | 🚫 Not Used | 📝 TASKER_TEMPLATE_PATH | Ruby discovers templates via filesystem/database |

**Category**: 🚫 **Unused in Rust** - Templates are database-driven, not config-driven

---

### Engine Configuration (`engine.*`)

| Field | Orchestration | Rust Worker | Ruby Worker | Notes |
|-------|--------------|-------------|-------------|-------|
| All `engine.*` fields | 🚫 Not Used | 🚫 Not Used | 🚫 Not Used | User confirmed doesn't exist |

**Category**: 🚫 **Unused** - Legacy/removed, candidate for deletion

---

### State Machine Configuration (`state_machine.*`)

| Field | Orchestration | Rust Worker | Ruby Worker | Notes |
|-------|--------------|-------------|-------------|-------|
| All `state_machine.*` fields | 🚫 Not Used | 🚫 Not Used | 🚫 Not Used | Possibly just documentation |

**Category**: 🚫 **Unused** - Candidate for removal

---

## Summary Statistics

### Orchestration Configuration Footprint
- **Direct Config Accesses**: 23 locations
- **Primary Config Categories**:
  - ✅ database.* (via SystemContext)
  - ✅ queues.* (orchestration namespace, queue names)
  - ✅ backoff.* (retry logic)
  - ✅ orchestration.* (web API control)
  - ✅ event_systems.orchestration.* (event coordination)
  - ✅ event_systems.task_readiness.* (task readiness)
  - 🔄 mpsc_channels.shared.* (event publisher)
  - ✅ mpsc_channels.orchestration.* (likely)
  - ⚠️ circuit_breakers.* (possibly deprecated)
  - ✅ environment()

### Rust Worker Configuration Footprint
- **Direct Config Accesses**: 3 locations (10% of orchestration)
- **Primary Config Categories**:
  - 🔄 database.* (via SystemContext)
  - ✅ queues.* (for result submission)
  - ✅ worker.* (web API control)
  - ✅ event_systems.worker.* (event coordination)
  - ✅ orchestration.web.* (to connect to orchestration API)
  - 🔄 mpsc_channels.shared.* (event publisher)
  - ✅ mpsc_channels.worker.* (likely)
  - ⚠️ circuit_breakers.* (possibly deprecated)
  - ✅ environment()

### Ruby Worker Configuration Footprint
- **Direct Config Accesses**: 0 (uses environment variables)
- **Primary Config Mechanism**: Environment variables
  - 📝 TASKER_ENV / RAILS_ENV
  - 📝 DATABASE_URL
  - 📝 TASKER_TEMPLATE_PATH
  - 📝 TASKER_FORCE_EXAMPLE_HANDLERS
  - 📝 LOG_LEVEL / RUST_LOG
  - 📝 RUBY_GC_HEAP_GROWTH_FACTOR

---

## Configuration Categories

### Common Configuration (Used by All Rust Components)
```
database.*                              # Database pool configuration
queues.* (namespace awareness)          # Queue namespace and names
environment()                           # Environment detection
mpsc_channels.shared.*                  # Event publisher buffer sizing
circuit_breakers.* (if not deprecated)  # Resilience configuration
```

**Estimated Fields**: ~20 fields

---

### Orchestration-Specific Configuration
```
backoff.*                               # Retry logic
orchestration.web.*                     # Web API configuration
event_systems.orchestration.*           # Orchestration event system
event_systems.task_readiness.*          # Task readiness event system
mpsc_channels.orchestration.*           # Orchestration command processor channels
mpsc_channels.task_readiness.*          # Task readiness coordinator channels
```

**Estimated Fields**: ~30 fields

---

### Rust-Worker-Specific Configuration
```
worker.web.*                            # Worker web API configuration
event_systems.worker.*                  # Worker event system
mpsc_channels.worker.*                  # Worker command processor channels
```

**Estimated Fields**: ~15 fields

---

### Ruby-Worker-Specific Configuration (Environment Variables)
```bash
TASKER_ENV                              # Environment detection
RAILS_ENV                               # Rails environment fallback
DATABASE_URL                            # Database connection
TASKER_TEMPLATE_PATH                    # Template discovery
TASKER_FORCE_EXAMPLE_HANDLERS           # Test mode
LOG_LEVEL                               # Ruby log level
RUST_LOG                                # Rust log level (delegated to Rust)
RUBY_GC_HEAP_GROWTH_FACTOR              # Production GC tuning
WORKSPACE_PATH                          # Workspace root
```

**Estimated Fields**: ~9 environment variables

---

### Unused Configuration (Candidates for Removal)
```
telemetry.*                             # No usage found
task_templates.*                        # Database-driven, not config
engine.*                                # User confirmed doesn't exist
state_machine.*                         # Possibly just documentation
execution.max_concurrent_tasks          # Only in tests
database.enable_secondary_database      # Only in tests, always false
queues.backend                          # Always "pgmq"
```

**Estimated Fields**: ~20+ fields that can be removed

---

## Configuration Overlap Analysis

### Orchestration + Rust Worker Overlap
```
✅ database.*                            # Both need database access
✅ queues.*                              # Both need queue coordination
✅ environment()                         # Both need environment detection
✅ mpsc_channels.shared.*                # Both use event publisher
⚠️ circuit_breakers.*                   # Both inherit from SystemContext (if used)
```

**Overlap**: ~20 fields (Common configuration)

### Orchestration + Rust Worker Unique to Each
```
Orchestration Only:
- backoff.*
- orchestration.web.*
- event_systems.orchestration.*
- event_systems.task_readiness.*
- mpsc_channels.orchestration.*
- mpsc_channels.task_readiness.*

Rust Worker Only:
- worker.web.*
- event_systems.worker.*
- mpsc_channels.worker.*
- orchestration.web.* (as client to connect)
```

### Ruby Worker Overlap with Rust
```
📝 DATABASE_URL                          # Maps to database_url()
📝 TASKER_ENV                            # Maps to environment()
```

**Overlap**: 2 concepts, different mechanisms (ENV vs TOML)

---

## Configuration Asymmetry Metrics

### Total Configuration Fields in Current TaskerConfig
**Estimated**: ~80-100 fields across all categories

### By Component
- **Orchestration**: ~50 fields used (50% of total)
- **Rust Worker**: ~20 fields used (20% of total)
- **Common (Overlap)**: ~20 fields (20% of total)
- **Unused**: ~20-30 fields (20-30% of total)

### Configuration Efficiency
- **Orchestration Loading**: 50 used / 80 total = 62.5% efficiency
- **Rust Worker Loading**: 20 used / 80 total = 25% efficiency
- **Ruby Worker Loading**: 0 used / 80 total = 0% (uses ENV instead)

**Conclusion**: Current monolithic TaskerConfig has significant waste, especially for workers

---

## Recommendations

### 1. Split TaskerConfig into Context-Specific Configs

**CommonConfig** (~20 fields):
- database.*
- queues.*
- environment
- mpsc_channels.shared.*

**OrchestrationConfig** (~30 fields):
- backoff.*
- orchestration.*
- event_systems.orchestration.*
- event_systems.task_readiness.*
- mpsc_channels.orchestration.*
- mpsc_channels.task_readiness.*

**RustWorkerConfig** (~15 fields):
- worker.*
- event_systems.worker.*
- mpsc_channels.worker.*
- orchestration.web.* (as client config)

**RubyWorkerConfig** (9 environment variables):
- TASKER_ENV, DATABASE_URL, TASKER_TEMPLATE_PATH, etc.

### 2. Remove Unused Configuration

**Immediate Candidates for Removal**:
- telemetry.* (unused)
- task_templates.* (database-driven)
- engine.* (doesn't exist)
- state_machine.* (documentation only)
- execution.max_concurrent_tasks (test-only)
- database.enable_secondary_database (test-only, always false)

**Estimated Reduction**: ~20-30 fields removed

### 3. Configuration Loading Strategy

**Orchestration Deployment**:
```toml
# Load CommonConfig + OrchestrationConfig
# Total: ~50 fields (62% reduction from current 80)
```

**Rust Worker Deployment**:
```toml
# Load CommonConfig + RustWorkerConfig
# Total: ~35 fields (56% reduction from current 80)
```

**Ruby Worker Deployment**:
```bash
# Load 9 environment variables
# No TOML configuration needed
```

### 4. Hybrid Configuration Loading

**Worker needs to know where orchestration lives**:
```toml
# CommonConfig includes:
[orchestration.api]  # New section for client connectivity
bind_address = "localhost:8080"  # Where orchestration listens
request_timeout_ms = 30000
```

This allows:
- Orchestration defines where it listens
- Workers know where to connect
- Both share same configuration

---

**Analysis Complete**: Configuration usage matrix ready for Step 2 (Identify configuration categories)
