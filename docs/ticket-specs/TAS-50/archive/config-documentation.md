# TAS-50: Configuration Documentation System

## Overview

This document describes the `_docs` namespace convention for co-located configuration documentation in tasker-core. This system enables interactive parameter documentation through the CLI without maintaining separate documentation files.

## Design Principles

### 1. Co-Location Over Separation

Documentation lives alongside configuration values in the same TOML files using a `_docs` namespace:

```toml
# Actual configuration
[database.pool]
max_connections = 30
min_connections = 8

# Documentation co-located with config
[database.pool._docs.max_connections]
description = "Maximum number of concurrent database connections in the pool"
type = "u32"
valid_range = "1-1000"
default = "30"
system_impact = "Controls database connection concurrency..."
```

**Rationale**: Keeps documentation synchronized with configuration, making it easier to maintain and update.

### 2. Single Source of Truth

The `_docs` sections in base configuration files are the authoritative documentation source. The ConfigDocumentation system reads and caches these sections at runtime.

**Benefits**:
- No duplicate documentation to keep in sync
- Documentation updates happen in the same commit as config changes
- CLI can access documentation without external files

### 3. Environment-Aware Recommendations

Documentation includes environment-specific recommendations using a nested structure:

```toml
[database.pool._docs.max_connections.recommendations]
test = { value = "5", rationale = "Minimal connections for test isolation" }
development = { value = "10", rationale = "Small pool for local development" }
production = { value = "30-50", rationale = "Scale based on concurrent task volume" }
```

**Value**: Provides context-appropriate guidance for different deployment scenarios.

## TOML Structure

### Parameter Documentation Format

Each documented parameter has a `_docs` section with the following fields:

```toml
[section.subsection._docs.parameter_name]
description = "What this parameter controls"
type = "Rust type (u32, String, bool, etc.)"
valid_range = "Valid values or range"
default = "Default value as string"
system_impact = "How this affects system behavior and performance"
related = ["other.related.parameter", "another.parameter"]
example = """
# Multiline example showing usage
[section.subsection]
parameter_name = value
"""

# Environment-specific recommendations
[section.subsection._docs.parameter_name.recommendations]
test = { value = "test_value", rationale = "Why this value for test" }
development = { value = "dev_value", rationale = "Why this value for development" }
production = { value = "prod_value", rationale = "Why this value for production" }
```

### Field Descriptions

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `description` | String | Yes | Clear, concise explanation of the parameter's purpose |
| `type` | String | Yes | Rust type or type description |
| `valid_range` | String | Yes | Valid values, ranges, or constraints |
| `default` | String | Yes | Default value (as a string for display) |
| `system_impact` | String | Yes | Impact on performance, behavior, or resources |
| `related` | Array | No | Related parameters that should be considered together |
| `example` | String | No | TOML example showing usage |
| `recommendations` | Table | No | Environment-specific value recommendations |

### Recommendation Format

Each recommendation has:
- `value`: Recommended value for the environment
- `rationale`: Why this value is recommended

## Implementation

### ConfigDocumentation System

Location: `tasker-shared/src/config/documentation.rs`

The system provides:

1. **Loading**: Recursively extracts `_docs` sections from TOML files
2. **Caching**: Maintains an in-memory cache of parameter documentation
3. **Lookup**: Provides fast parameter documentation retrieval
4. **Type Safety**: Uses Rust structs for type-safe documentation access

### Key Types

```rust
pub struct ConfigDocumentation {
    cache: HashMap<String, ParameterDocumentation>,
}

pub struct ParameterDocumentation {
    pub description: String,
    pub param_type: String,
    pub valid_range: String,
    pub default: String,
    pub system_impact: String,
    pub related: Vec<String>,
    pub example: String,
    pub recommendations: Vec<EnvironmentRecommendation>,
}

pub struct EnvironmentRecommendation {
    pub environment: String,
    pub value: String,
    pub rationale: String,
}
```

### CLI Integration

The `tasker-cli` provides an `explain` command:

```bash
cargo run --package tasker-client --bin tasker-cli -- config explain --parameter database.pool.max_connections
```

Output:
```
📖 Configuration Parameter: database.pool.max_connections

Description:
  Maximum number of concurrent database connections in the pool

Type: u32
Valid Range: 1-1000
Default: 30

System Impact:
  Controls database connection concurrency. Too few = query queuing...

Environment-Specific Recommendations:
  test environment:
    Value: 5
    Rationale: Minimal connections for test isolation...
```

## Documentation Coverage

### Current Coverage (Phase 1)

1. **common.toml**:
   - `database.pool.max_connections`
   - `database.pool.min_connections`

2. **orchestration.toml**:
   - `orchestration_system.mode`
   - `orchestration_events.deployment_mode`
   - `orchestration_events.processing.max_concurrent_operations`

3. **worker.toml**:
   - `worker_system.step_processing.max_concurrent_steps`
   - `worker_system.event_driven.deployment_mode`
   - `worker_system.fallback_poller.polling_interval_ms`

### Expansion Strategy

Future phases should document:

1. **Phase 2**: All critical performance parameters
   - Database pool settings
   - MPSC channel buffer sizes
   - Concurrency limits
   - Timeout configurations

2. **Phase 3**: Event system configurations
   - Deployment modes
   - Health monitoring
   - Backoff strategies
   - Circuit breaker settings

3. **Phase 4**: Resource limits and monitoring
   - Memory thresholds
   - CPU limits
   - Queue configurations
   - Metrics collection

## Usage Guidelines

### When to Add Documentation

Add `_docs` sections when:
1. A parameter significantly impacts system performance
2. The parameter requires environment-specific tuning
3. The parameter has non-obvious interactions with other settings
4. Users frequently ask questions about the parameter

### Documentation Quality Standards

Good documentation should:
1. **Be Concise**: Clear description in 1-2 sentences
2. **Explain Impact**: Describe what happens when values are too high/low
3. **Show Relationships**: Link to related parameters
4. **Provide Examples**: Demonstrate real-world usage
5. **Give Guidance**: Environment-specific recommendations with rationale

### Anti-Patterns

Avoid:
1. **Generic Descriptions**: "Controls the pool size" is not helpful
2. **Missing Impact**: Don't just say what it is, explain what it affects
3. **No Context**: Always provide environment-specific guidance
4. **Stale Examples**: Keep examples synchronized with current config structure

## Example: Well-Documented Parameter

```toml
[orchestration_events.processing]
max_concurrent_operations = 10

[orchestration_events.processing._docs.max_concurrent_operations]
description = "Maximum number of concurrent orchestration operations (task initialization, result processing, finalization)"
type = "usize"
valid_range = "1-1000"
default = "10"
system_impact = "Controls orchestration concurrency and throughput. Too few = slow task processing and queuing. Too many = database connection exhaustion and CPU contention. Should align with database pool size."
related = ["database.pool.max_connections", "orchestration_events.processing.batch_size"]
example = """
# High-throughput production orchestration
[orchestration_events.processing]
max_concurrent_operations = 50
batch_size = 20

[database.pool]
max_connections = 50  # Match orchestration concurrency
"""

[orchestration_events.processing._docs.max_concurrent_operations.recommendations]
test = { value = "5", rationale = "Low concurrency for predictable test execution" }
development = { value = "10", rationale = "Moderate concurrency for local development" }
production = { value = "20-50", rationale = "Scale based on task volume and database capacity" }
```

This example demonstrates:
- Clear, specific description
- Explicit type and range
- Impact explanation with consequences
- Related parameter links
- Realistic usage example
- Environment-specific guidance with rationale

## Technical Notes

### TOML Parsing

The system uses recursive TOML traversal:
1. Load base configuration file
2. Walk the TOML tree looking for `_docs` keys
3. Extract and deserialize documentation structures
4. Cache by parameter path (e.g., `database.pool.max_connections`)

### Parameter Path Resolution

Parameter paths use dot notation matching the TOML structure:
- `database.pool.max_connections` → `[database.pool._docs.max_connections]`
- `worker_system.step_processing.max_concurrent_steps` → `[worker_system.step_processing._docs.max_concurrent_steps]`

### Reserved Keywords

The `type` field requires special handling in Rust due to being a reserved keyword:
```rust
#[serde(rename = "type")]
pub param_type: String,
```

## Future Enhancements

### Potential Additions

1. **Interactive Tuning**: CLI command to suggest values based on workload
2. **Validation**: Check current values against recommendations
3. **Documentation Export**: Generate markdown reference from `_docs`
4. **Web UI**: Interactive configuration explorer
5. **Migration Guides**: Document breaking changes in `_docs`

### Related Work

- **TAS-50 Phase 1**: Context-specific configuration (CommonConfig, OrchestrationConfig, WorkerConfig)
- **TAS-50 Phase 2**: Component-based TOML structure
- **TAS-50 Phase 3**: Single-file configuration consolidation
- **TAS-51**: Bounded MPSC channels configuration

## Maintenance

### Adding New Documentation

1. Add `_docs` section to appropriate TOML file
2. Follow the standard field structure
3. Include environment recommendations
4. Test with CLI explain command
5. Update this document's coverage section

### Reviewing Documentation

Periodically review:
1. Accuracy of system impact descriptions
2. Validity of recommendations for current architecture
3. Relevance of related parameter links
4. Completeness of examples

### Version Control

- Commit documentation changes with related config changes
- Update examples when config structure changes
- Tag breaking changes that affect recommendations
- Review documentation during configuration refactors

---

**Last Updated**: 2025-10-18
**Status**: Phase 1 Implementation Complete
**Related Tickets**: TAS-50 (Configuration Cleanup), TAS-51 (MPSC Channels)
