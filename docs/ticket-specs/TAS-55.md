# TAS-55: Complete TOML Configuration Documentation

**Status**: Planned
**Created**: 2025-10-19
**Priority**: Medium
**Category**: Documentation / Developer Experience
**Depends On**: TAS-50 (Configuration Documentation Foundation)

## Overview

Complete the `._docs` configuration documentation system introduced in TAS-50. The foundation is in place with ConfigDocumentation system, CLI `explain` command, and proof-of-concept documentation for 6 parameters. This ticket covers systematic expansion to all critical configuration parameters.

## Background

### TAS-50 Foundation (Complete)

**Infrastructure Delivered**:
- ✅ ConfigDocumentation system (`tasker-shared/src/config/documentation.rs`)
- ✅ TOML `._docs` namespace convention
- ✅ CLI `explain` command for interactive documentation
- ✅ Environment-specific recommendations structure
- ✅ Documentation loading and caching
- ✅ Type-safe Rust structs for documentation access

**Proof of Concept Documentation** (6 parameters):

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

**Reference Documentation**:
- `docs/ticket-specs/TAS-50/config-documentation.md` - Full system design and conventions

## Scope: What Needs Documentation

### Phase 1: Critical Performance Parameters (High Priority)

These parameters significantly impact system performance and require environment-specific tuning.

#### Database Configuration (common.toml)
- ✅ `database.pool.max_connections` (DONE)
- ✅ `database.pool.min_connections` (DONE)
- ⏳ `database.pool.connection_timeout_ms`
- ⏳ `database.pool.idle_timeout_seconds`
- ⏳ `database.pool.max_lifetime_seconds`

#### MPSC Channel Configuration (orchestration.toml, worker.toml)
**TAS-51 Bounded Channels** - Critical capacity management:

Orchestration channels:
- ⏳ `mpsc_channels.command_processor.command_buffer_size`
- ⏳ `mpsc_channels.event_systems.pgmq_event_buffer_size`
- ⏳ `mpsc_channels.event_systems.health_event_buffer_size`

Worker channels:
- ⏳ `mpsc_channels.command_processor.command_buffer_size`
- ⏳ `mpsc_channels.event_subscribers.completion_buffer_size`
- ⏳ `mpsc_channels.event_subscribers.result_buffer_size`
- ⏳ `mpsc_channels.in_process_events.broadcast_buffer_size`

#### Event System Configuration
Orchestration:
- ✅ `orchestration_events.deployment_mode` (DONE)
- ✅ `orchestration_events.processing.max_concurrent_operations` (DONE)
- ⏳ `orchestration_events.processing.batch_size`
- ⏳ `orchestration_events.timing.health_check_interval_seconds`
- ⏳ `orchestration_events.timing.fallback_polling_interval_seconds`

Worker:
- ✅ `worker_events.deployment_mode` (DONE)
- ⏳ `worker_events.processing.max_concurrent_operations`
- ⏳ `worker_events.processing.batch_size`
- ⏳ `worker_events.timing.claim_timeout_seconds`

#### Concurrency Limits
- ✅ `orchestration_events.processing.max_concurrent_operations` (DONE)
- ✅ `worker_system.step_processing.max_concurrent_steps` (DONE)
- ⏳ `worker_events.processing.max_concurrent_operations`

### Phase 2: Reliability and Resilience (Medium Priority)

#### Circuit Breaker Configuration (common.toml)
All circuit breaker settings:
- ⏳ `circuit_breaker.database.failure_threshold`
- ⏳ `circuit_breaker.database.timeout_seconds`
- ⏳ `circuit_breaker.database.reset_timeout_seconds`
- ⏳ `circuit_breaker.messaging.failure_threshold`
- ⏳ `circuit_breaker.messaging.timeout_seconds`

#### Backoff and Retry Configuration (common.toml)
- ⏳ `backoff.initial_delay_ms`
- ⏳ `backoff.max_delay_ms`
- ⏳ `backoff.multiplier`
- ⏳ `backoff.jitter_percent`

#### Health Monitoring (orchestration.toml, worker.toml)
- ⏳ `orchestration_events.health.enabled`
- ⏳ `orchestration_events.health.max_consecutive_errors`
- ⏳ `worker_events.health.enabled`
- ⏳ `worker_events.health.error_rate_threshold_per_minute`

### Phase 3: Polling and Timing (Medium Priority)

#### Fallback Poller Configuration
- ✅ `worker_system.fallback_poller.polling_interval_ms` (DONE)
- ⏳ `worker_system.fallback_poller.batch_size`
- ⏳ `worker_system.fallback_poller.age_threshold_seconds`
- ⏳ `worker_system.fallback_poller.max_age_hours`
- ⏳ `orchestration_events.timing.fallback_polling_interval_seconds`

#### Timeout Configuration
- ⏳ `orchestration_events.timing.processing_timeout_seconds`
- ⏳ `orchestration_events.timing.visibility_timeout_seconds`
- ⏳ `worker_events.timing.claim_timeout_seconds`
- ⏳ `worker_system.step_processing.claim_timeout_seconds`

### Phase 4: Resource Limits and Monitoring (Low Priority)

#### Resource Limits (worker.toml)
- ⏳ `worker_system.resource_limits.max_memory_mb`
- ⏳ `worker_system.resource_limits.max_cpu_percent`
- ⏳ `worker_system.resource_limits.max_database_connections`
- ⏳ `worker_system.resource_limits.max_queue_connections`

#### Web API Configuration (orchestration.toml, worker.toml)
- ⏳ `orchestration_system.web.bind_address`
- ⏳ `orchestration_system.web.request_timeout_ms`
- ⏳ `worker_system.web.bind_address`
- ⏳ `worker_system.web.rate_limiting.requests_per_minute`

### Phase 5: Advanced Configuration (Low Priority)

#### Query Cache (common.toml)
- ⏳ `query_cache.enabled`
- ⏳ `query_cache.ttl_seconds`
- ⏳ `query_cache.max_entries`

#### Telemetry (common.toml)
- ⏳ `telemetry.metrics_enabled`
- ⏳ `telemetry.tracing_enabled`
- ⏳ `telemetry.log_level`

## Documentation Standards

### Required Fields (from TAS-50 spec)

Each documented parameter must include:

```toml
[section.subsection._docs.parameter_name]
description = "Clear, concise explanation (1-2 sentences)"
type = "Rust type (u32, String, bool, etc.)"
valid_range = "Valid values or range"
default = "Default value as string"
system_impact = "Impact on performance, behavior, resources. Explain consequences of too high/low values."
related = ["other.parameter", "another.parameter"]  # Optional but encouraged
example = """
# Multiline TOML example showing usage
[section.subsection]
parameter_name = value
"""  # Optional but encouraged

[section.subsection._docs.parameter_name.recommendations]
test = { value = "test_value", rationale = "Why this for test" }
development = { value = "dev_value", rationale = "Why this for development" }
production = { value = "prod_value", rationale = "Why this for production" }
```

### Quality Standards (from TAS-50 spec)

Good documentation should:
1. **Be Concise**: Clear description in 1-2 sentences
2. **Explain Impact**: What happens when values are too high/low
3. **Show Relationships**: Link to related parameters
4. **Provide Examples**: Demonstrate real-world usage
5. **Give Guidance**: Environment-specific recommendations with rationale

### Anti-Patterns to Avoid

❌ Generic descriptions: "Controls the pool size"
❌ Missing impact: Don't just say what it is, explain what it affects
❌ No context: Always provide environment-specific guidance
❌ Stale examples: Keep examples synchronized with current config structure

## Implementation Strategy

### Recommended Approach

**Incremental Documentation by Phase**:
1. Document Phase 1 parameters (critical performance) - highest ROI
2. Test documentation with real deployment scenarios
3. Gather feedback from operations team
4. Document Phase 2 (reliability) based on learnings
5. Continue through phases as time permits

**Focus Areas**:
- Parameters that cause production incidents when misconfigured
- Settings frequently modified during troubleshooting
- Values that require environment-specific tuning

### Validation Process

For each documented parameter:
1. ✅ Add `._docs` section to appropriate TOML file
2. ✅ Test with CLI: `cargo run --package tasker-client --bin tasker-cli -- config explain --parameter <path>`
3. ✅ Verify all required fields present
4. ✅ Check environment recommendations are sensible
5. ✅ Ensure examples are valid TOML and match current structure

### Example Documentation Workflow

```bash
# 1. Add documentation to config file
vim config/tasker/base/common.toml

# 2. Test CLI access
cargo run --package tasker-client --bin tasker-cli -- config explain \
  --parameter database.pool.connection_timeout_ms

# 3. Verify output formatting
# Should show: description, type, range, default, impact, recommendations

# 4. Test in different environments
TASKER_ENV=test cargo run --package tasker-client --bin tasker-cli -- \
  config explain --parameter database.pool.connection_timeout_ms

# 5. Commit with descriptive message
git add config/tasker/base/common.toml
git commit -m "docs(TAS-55): document database.pool.connection_timeout_ms"
```

## Success Criteria

### Phase 1 Complete
- ✅ All critical performance parameters documented (database, MPSC, concurrency)
- ✅ Documentation accessible via CLI `explain` command
- ✅ Environment-specific recommendations for all documented parameters
- ✅ Real-world examples for common configuration patterns

### Phase 2 Complete
- ✅ All reliability parameters documented (circuit breakers, backoff, health)
- ✅ Cross-references between related parameters working
- ✅ Operational runbook references documentation

### Full Documentation Complete (All Phases)
- ✅ All configuration parameters documented
- ✅ Interactive configuration guide in CLI
- ✅ Documentation export to markdown reference
- ✅ Integration with deployment tooling

## Metrics and Tracking

### Documentation Coverage
Track percentage of parameters documented:
- **Current**: 6/150+ parameters (~4%)
- **Phase 1 Target**: 30 parameters (~20%)
- **Phase 2 Target**: 55 parameters (~36%)
- **Full Coverage Target**: 100% of tunable parameters

### Quality Metrics
- All documented parameters have environment recommendations
- All performance-critical parameters have system impact explanations
- All documented parameters have related parameter cross-references
- 80%+ of documented parameters have working examples

## Related Work

- **TAS-50**: Configuration documentation foundation (complete)
- **TAS-51**: Bounded MPSC channels (complete) - many parameters need docs
- **TAS-54**: Processor UUID ownership analysis - may affect configuration strategy
- **TAS-33**: Circuit breaker integration - configuration needs documentation

## Future Enhancements (Out of Scope)

These can be considered after base documentation is complete:

1. **Interactive Configuration Wizard**: CLI tool to generate optimized configs
2. **Configuration Validation**: Warn when values outside recommended ranges
3. **Performance Profiling**: Suggest configuration changes based on metrics
4. **Documentation Export**: Generate markdown reference from `._docs`
5. **Web UI**: Interactive configuration explorer
6. **Migration Guides**: Document configuration changes between versions

## References

### Code Locations
- Documentation system: `tasker-shared/src/config/documentation.rs`
- CLI explain command: `tasker-client/src/cli/commands/config.rs`
- Configuration files: `config/tasker/base/*.toml`
- Environment overrides: `config/tasker/environments/{test,development,production}/*.toml`

### Documentation
- Design spec: `docs/ticket-specs/TAS-50/config-documentation.md`
- MPSC channels: `docs/operations/mpsc-channel-tuning.md`
- Configuration architecture: `docs/ticket-specs/TAS-50/phase2-3-completion.md`

### Examples
- Existing docs: See `config/tasker/base/common.toml`, `orchestration.toml`, `worker.toml`
- CLI usage: `cargo run --package tasker-client --bin tasker-cli -- config explain --help`

---

**Estimated Effort**:
- Phase 1: 1-2 days (high value, immediate operational benefit)
- Phase 2: 1 day (resilience focus)
- Phase 3: 1 day (timing and polling)
- Phase 4-5: 1-2 days (nice-to-have coverage)

**Total**: 4-6 days for comprehensive documentation coverage

**Incremental Value**: Each phase delivers standalone value; can be completed over time without blocking other work.

---

**Last Updated**: 2025-10-19
**Status**: Ready for implementation after TAS-50 merge
