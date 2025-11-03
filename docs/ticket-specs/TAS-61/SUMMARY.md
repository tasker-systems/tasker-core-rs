# TAS-61 Configuration Analysis Summary

**Generated**: Sun Nov  2 19:06:26 EST 2025
**Analysis Root**: /Users/petetaylor/projects/tasker-systems/tasker-core

## Config Structs Found

     126 total `*Config` struct definitions

### By Crate

```
pgmq-notify: 1
tasker-client: 5
tasker-orchestration: 11
tasker-shared: 99
tasker-worker: 9
workers: 1
```

## Top-Level Config Field Access Counts

Top 20 most accessed config fields:

```
    96 config.rs
    78 config.clone
    34 config.validate
    30 config.orchestration
    29 config.execution
    28 config.queues
    28 config.backoff
    26 config.enabled
    22 config.event_systems
    22 config.database
    22 config.batch_size
    20 config.mpsc_channels
    18 config.worker
    18 config.environment
    18 config.deployment_mode
    17 config.metadata
    17 config.base_url
    14 config.route_requires_auth
    13 config.polling_interval
    13 config.auth
```

## TaskReadinessConfig Removal Readiness

### Struct References

```
tasker-shared/src/config/task_readiness.rs:12:pub struct TaskReadinessConfig {
tasker-shared/src/config/task_readiness.rs:235:impl Default for TaskReadinessConfig {
tasker-shared/src/config/task_readiness.rs:360:impl TaskReadinessConfig {
tasker-shared/src/config/task_readiness.rs:504:        let config = TaskReadinessConfig::default();
tasker-shared/src/config/task_readiness.rs:515:        let config = TaskReadinessConfig::default();
tasker-shared/src/config/task_readiness.rs:524:        let config = TaskReadinessConfig::default();
tasker-shared/src/config/task_readiness.rs:538:        let mut config = TaskReadinessConfig::default();
tasker-shared/src/config/task_readiness.rs:558:        let config = TaskReadinessConfig::default();
tasker-shared/src/config/task_readiness.rs:565:        let deserialized: TaskReadinessConfig =
tasker-shared/src/config/tasker.rs:66:    ReadinessFallbackConfig, TaskReadinessConfig, TaskReadinessCoordinatorConfig,
... (      13 total)
```

### Field Accesses (config.task_readiness.*)

```
tasker-shared/src/config/mpsc_channels.rs:359:        assert_eq!(config.task_readiness.event_channel.buffer_size, 1000);
tasker-shared/src/config/mpsc_channels.rs:360:        assert_eq!(config.task_readiness.event_channel.send_timeout_ms, 1000);
... (       2 total)
```

## Analysis Artifacts

All analysis outputs available in: `docs/ticket-specs/TAS-61/`

- `config-structs.txt` - All *Config struct locations
- `config-by-crate.txt` - Config count per crate
- `config-accesses-raw.txt` - All config field access patterns
- `config-top-level-accesses.txt` - Top-level field access counts
- `task-readiness-config-usage.txt` - TaskReadinessConfig references
- `task-readiness-field-accesses.txt` - config.task_readiness field accesses

## Available CLI Tools (Phase 0 Complete)

Built-in permanent CLI commands for ongoing configuration management:

```bash
# Dump configuration structure (JSON, YAML, TOML)
tasker-cli config dump --context complete --environment test --format json

# Analyze configuration usage patterns (prevents drift)
tasker-cli config analyze-usage --format text
tasker-cli config analyze-usage --format json --output analysis.json
tasker-cli config analyze-usage --show-unused --show-locations
```

## Next Steps - Phase 1 Cleanup

1. ✅ Phase 0 Complete - Analysis and tooling ready
2. **Phase 1a**: Delete TaskReadinessConfig (574 lines, 0 runtime usage)
3. **Phase 1a**: Add missing required fields to common.toml
4. **Phase 1b**: Remove extraneous TOML fields
5. **Phase 1b**: Consolidate merge logic and simplify loading

