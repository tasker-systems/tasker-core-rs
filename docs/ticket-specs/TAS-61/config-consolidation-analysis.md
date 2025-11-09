# TAS-61 Phase 6C/6D: Config Struct Consolidation Analysis

## Executive Summary

**Goal**: Ensure all config structs hydrated from TOML exist only in tasker_v2.rs (single source of truth), with impl blocks federated as needed.

**Findings**:
- ✅ **5 files already refactored as adapters**: circuit_breaker.rs, queues.rs, web.rs, mpsc_channels.rs, event_systems.rs
- ⚠️ **2 structs are adapters** (u32→u64/usize): StepProcessingConfig, HealthMonitoringConfig
- ❌ **3 orchestration configs are legacy builders** (not in V2): StepEnqueuerConfig, StepResultProcessorConfig, TaskClaimStepEnqueuerConfig
- ❌ **1 orchestration config is legacy** (different structure): OrchestrationConfig in orchestration/mod.rs
- ❌ **5 worker configs are legacy** (not in V2): EventSystemConfig, EventPublisherConfig, EventSubscriberConfig, EventProcessingConfig, legacy WorkerConfig

## Detailed Analysis

### CATEGORY A: Adapters (u32→u64/usize conversion) - KEEP

#### 1. StepProcessingConfig (worker.rs vs tasker_v2.rs)

**V2 Definition** (tasker_v2.rs:1895):
```rust
pub struct StepProcessingConfig {
    pub claim_timeout_seconds: u32,      // TOML storage optimization
    pub max_retries: u32,
    pub max_concurrent_steps: u32,       // TOML storage optimization
}
```

**Legacy Definition** (worker.rs:34):
```rust
pub struct StepProcessingConfig {
    pub claim_timeout_seconds: u64,      // Runtime performance
    pub max_retries: u32,                // SAME
    pub max_concurrent_steps: usize,     // Runtime performance
}
```

**Verdict**: **INTENTIONAL ADAPTER** - Keep with documentation
- Type conversions: u32→u64, u32→usize
- Purpose: Runtime performance optimization
- Action: Document as adapter layer, add From<V2> implementation

#### 2. HealthMonitoringConfig (worker.rs vs tasker_v2.rs)

**V2 Definition** (tasker_v2.rs:1917):
```rust
pub struct HealthMonitoringConfig {
    pub health_check_interval_seconds: u32,  // TOML storage optimization
    pub performance_monitoring_enabled: bool,
    pub error_rate_threshold: f64,
}
```

**Legacy Definition** (worker.rs:105):
```rust
pub struct HealthMonitoringConfig {
    pub health_check_interval_seconds: u64,  // Runtime performance
    pub performance_monitoring_enabled: bool, // SAME
    pub error_rate_threshold: f64,           // SAME
}
```

**Verdict**: **INTENTIONAL ADAPTER** - Keep with documentation
- Type conversion: u32→u64
- Purpose: Runtime performance optimization
- Action: Document as adapter layer, add From<V2> implementation

### CATEGORY B: Legacy Builders (Not in V2) - EVALUATE USAGE

#### 3. StepEnqueuerConfig (orchestration/step_enqueuer.rs)

**NOT IN V2** - Built from multiple V2 sources:
```rust
pub struct StepEnqueuerConfig {
    pub max_steps_per_task: usize,         // From: config.common.execution.step_batch_size
    pub enqueue_delay_seconds: i32,        // No mapping (default: 0)
    pub enable_detailed_logging: bool,     // From: orchestration.enable_performance_logging
    pub enqueue_timeout_seconds: u64,      // From: config.common.execution.step_execution_timeout_seconds
}
```

**Verdict**: **LEGACY BUILDER** - Evaluate if still used
- Not hydrated from TOML (built from V2 fields)
- Has From<TaskerConfigV2> implementation
- Action: Search codebase for usage, consider removal if unused

#### 4. StepResultProcessorConfig (orchestration/step_result_processor.rs)

**NOT IN V2** - Built from multiple V2 sources:
```rust
pub struct StepResultProcessorConfig {
    pub step_results_queue_name: String,      // Built from: orchestration_namespace + step_results
    pub batch_size: i32,                      // From: common.queues.default_batch_size
    pub visibility_timeout_seconds: i32,      // From: common.queues.default_visibility_timeout_seconds
    pub polling_interval_seconds: u64,        // From: common.queues.pgmq.poll_interval_ms / 1000
    pub max_processing_attempts: i32,         // From: common.queues.pgmq.max_retries
}
```

**Verdict**: **LEGACY BUILDER** - Evaluate if still used
- Not hydrated from TOML (built from V2 fields)
- Has From<TaskerConfigV2> implementation
- Action: Search codebase for usage, consider removal if unused

#### 5. TaskClaimStepEnqueuerConfig (orchestration/task_claim_step_enqueuer.rs)

**NOT IN V2** - Aggregates other legacy builders:
```rust
pub struct TaskClaimStepEnqueuerConfig {
    pub batch_size: u32,                           // From: common.queues.default_batch_size
    pub namespace_filter: Option<String>,          // No mapping (default: None)
    pub enable_performance_logging: bool,          // From: orchestration.enable_performance_logging
    pub enable_heartbeat: bool,                    // No mapping (default: true)
    pub step_enqueuer_config: StepEnqueuerConfig,  // Legacy builder
    pub step_result_processor_config: StepResultProcessorConfig, // Legacy builder
}
```

**Verdict**: **LEGACY BUILDER** - Evaluate if still used
- Not hydrated from TOML (built from V2 fields)
- Depends on other legacy builders
- Action: Search codebase for usage, consider removal if unused

### CATEGORY C: Legacy Configs (Different Structure) - EVALUATE USAGE

#### 6. OrchestrationConfig (orchestration/mod.rs:16)

**Legacy Definition**:
```rust
pub struct OrchestrationConfig {
    pub mode: String,
    pub enable_performance_logging: bool,
    pub web: WebConfig,
}
```

**V2 Definition** (tasker_v2.rs:813):
```rust
pub struct OrchestrationConfig {
    pub mode: String,
    pub enable_performance_logging: bool,
    pub event_systems: OrchestrationEventSystemsConfig,  // NOT in legacy
    pub decision_points: DecisionPointsConfig,           // NOT in legacy
    pub mpsc_channels: OrchestrationMpscChannelsConfig,  // NOT in legacy
    pub dlq: DeadLetterQueueConfig,                      // NOT in legacy
    pub web: OrchestrationWebConfig,                     // Different type
}
```

**Verdict**: **LEGACY CONFIG** - Different structure
- V2 has additional fields (event_systems, decision_points, mpsc_channels, dlq)
- V2 uses OrchestrationWebConfig (not WebConfig)
- Action: Search codebase for usage, migrate to V2 or remove

#### 7. OrchestrationSystemConfig (orchestration/mod.rs:53)

**Legacy Definition**:
```rust
pub struct OrchestrationSystemConfig {
    pub orchestrator_id: String,
    pub enable_performance_logging: bool,
}
```

**NOT IN V2** - No corresponding struct

**Verdict**: **LEGACY CONFIG** - Not in V2
- Action: Search codebase for usage, consider removal if unused

#### 8. WorkerConfig (worker.rs:12) - Legacy Version

**Legacy Definition**:
```rust
pub struct WorkerConfig {
    pub worker_id: String,
    pub worker_type: String,
    pub step_processing: StepProcessingConfig,  // Legacy adapter type
    pub health_monitoring: HealthMonitoringConfig, // Legacy adapter type
    pub queues: QueuesConfig,                   // #[serde(skip)] - NOT in V2
    pub web: WebConfig,                         // V2 has Option<WorkerWebConfig>
}
```

**V2 Definition** (tasker_v2.rs:1666):
```rust
pub struct WorkerConfig {
    pub worker_id: String,
    pub worker_type: String,
    pub event_systems: WorkerEventSystemsConfig,  // NOT in legacy
    pub step_processing: StepProcessingConfig,    // V2 uses u32 types
    pub health_monitoring: HealthMonitoringConfig, // V2 uses u32 types
    pub mpsc_channels: WorkerMpscChannelsConfig,  // NOT in legacy
    pub web: Option<WorkerWebConfig>,             // Different type
}
```

**Verdict**: **LEGACY CONFIG** - Different structure
- Missing V2 fields: event_systems, mpsc_channels
- Has extra field: queues (#[serde(skip)])
- Different web type
- Action: Search codebase for usage, migrate to V2 or remove

#### 9-12. Worker Event System Configs (worker.rs) - NOT IN V2

**NOT IN V2**:
- `EventSystemConfig` (line 47) - NAME CLASH with event_systems.rs!
- `EventPublisherConfig` (line 60)
- `EventSubscriberConfig` (line 76)
- `EventProcessingConfig` (line 92)

**Verdict**: **LEGACY CONFIGS** - Not in V2
- Not hydrated from TOML
- Only have Default implementations
- Action: Search codebase for usage, consider removal if unused

### CATEGORY D: Utility Structs (Runtime/Non-TOML) - KEEP

These are runtime utilities, not TOML-hydrated configs:
- ConfigLoader, ConfigManager (config_loader.rs)
- ConfigDocumentation, ParameterDocumentation, EnvironmentRecommendation (documentation.rs)
- QueueClassifier, QueueType, ConfigDrivenMessageEvent (queue_classification.rs)
- ConfigurationError (error.rs)
- ConfigMerger (merger.rs)

**Verdict**: KEEP - Not config structs, runtime utilities

## Recommended Actions

### Phase 1: Document Adapters
1. ✅ **event_systems.rs** - Already documented as adapter layer
2. 🔄 **worker.rs** - Add adapter documentation for StepProcessingConfig, HealthMonitoringConfig
   - Add From<V2> implementations
   - Document u32→u64/usize conversions

### Phase 2: Search and Evaluate Legacy Configs
Search codebase for usage of:
- `StepEnqueuerConfig` - Check if used in orchestration code
- `StepResultProcessorConfig` - Check if used in orchestration code
- `TaskClaimStepEnqueuerConfig` - Check if used in orchestration code
- `orchestration::OrchestrationConfig` - Check if used vs V2's OrchestrationConfig
- `OrchestrationSystemConfig` - Check if still needed
- Legacy `WorkerConfig` - Check if used vs V2's WorkerConfig
- `EventSystemConfig` (worker.rs) - Check if used (NAME CLASH!)
- `EventPublisherConfig`, `EventSubscriberConfig`, `EventProcessingConfig` - Check if used

### Phase 3: Remove or Migrate
Based on Phase 2 findings:
- If **unused**: Remove files and update imports
- If **used**: Migrate usage to V2 types or document why legacy needed
- If **name clash**: Resolve EventSystemConfig ambiguity

### Phase 4: Update Imports
After removal/migration:
- Update all imports to point to tasker_v2.rs for canonical types
- Keep only adapter files with From<V2> implementations
- Remove pure re-export files

## Name Clash Warning

⚠️ **CRITICAL**: `EventSystemConfig` defined in TWO places:
1. `worker.rs:47` - Worker event system config (NOT in V2)
2. `event_systems.rs:33` - Generic event system config with metadata

This is a maintenance hazard and must be resolved!

## Summary Statistics

**Total config files analyzed**: 23
- ✅ **Adapters (keep)**: 7 files (circuit_breaker, queues, web, mpsc_channels, event_systems, worker.rs adapters)
- ⚠️ **Legacy builders (evaluate)**: 3 structs (StepEnqueuerConfig, StepResultProcessorConfig, TaskClaimStepEnqueuerConfig)
- ❌ **Legacy configs (evaluate)**: 6 structs (OrchestrationConfig, OrchestrationSystemConfig, WorkerConfig, EventSystemConfig, EventPublisherConfig, EventSubscriberConfig, EventProcessingConfig)
- ✅ **Utilities (keep)**: 4 files (config_loader, documentation, queue_classification, error, merger)

**Next Step**: Search codebase for usage of legacy configs to determine removal vs migration strategy.
