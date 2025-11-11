# Config Struct Comprehensive Audit - TAS-61 Final Consolidation

## Files Already Refactored ✅
1. circuit_breaker.rs - Adapter with HashMap flexibility
2. queues.rs - Adapter + utility struct (OrchestrationOwnedQueues)
3. web.rs - Adapter for WebAuthConfig (HashMap vs Vec)
4. mpsc_channels.rs - Adapter + deprecated TaskReadiness structs
5. event_systems.rs - Adapter layer (u32→u64/usize conversions)

## Files To Analyze

### CATEGORY A: Orchestration Configs (orchestration/*.rs)
**orchestration/mod.rs:**
- `OrchestrationConfig` (16 lines)
- `OrchestrationSystemConfig` (53 lines)

**orchestration/step_enqueuer.rs:**
- `StepEnqueuerConfig` (8 lines)

**orchestration/step_result_processor.rs:**
- `StepResultProcessorConfig` (8 lines)

**orchestration/task_claim_step_enqueuer.rs:**
- `TaskClaimStepEnqueuerConfig` (10 lines)

### CATEGORY B: Worker Configs (worker.rs)
**worker.rs:**
- `WorkerConfig` (12 lines)
- `StepProcessingConfig` (34 lines)
- `EventSystemConfig` (47 lines) - NAME CLASH with event_systems.rs!
- `EventPublisherConfig` (60 lines)
- `EventSubscriberConfig` (76 lines)
- `EventProcessingConfig` (92 lines)
- `HealthMonitoringConfig` (105 lines)

### CATEGORY C: Utility/Non-TOML Structs
**config_loader.rs:**
- `ConfigLoader` (23 lines) - Runtime utility
- `ConfigManager` (148 lines) - Runtime utility

**merger.rs:**
- `ConfigMerger` (36 lines) - Build-time utility

**documentation.rs:**
- `ConfigDocumentation` (105 lines) - Runtime utility
- `ParameterDocumentation` (55 lines) - Runtime utility
- `EnvironmentRecommendation` (92 lines) - Runtime utility

**queue_classification.rs:**
- `QueueClassifier` (26 lines) - Runtime utility
- `QueueType` (10 lines) - Runtime enum
- `ConfigDrivenMessageEvent<T>` (186 lines) - Runtime enum

**error.rs:**
- `ConfigurationError` (19 lines) - Error type

### CATEGORY D: Already Analyzed (Keep as Adapters)
**event_systems.rs:**
- EventSystemConfig<T> - Generic wrapper
- EventSystemTimingConfig - u32→u64 adapter
- BackoffConfig - u32→u64 adapter
- EventSystemProcessingConfig - u32→usize adapter
- Worker/Orchestration/TaskReadiness metadata types

**mpsc_channels.rs:**
- MpscChannelsConfig - Root adapter
- TaskReadinessChannelsConfig - Deprecated
- TaskReadinessEventChannelConfig - Deprecated
- DropPolicy - Type-safe enum (V2 uses String)

**queues.rs:**
- OrchestrationOwnedQueues - Utility struct

**web.rs:**
- WebAuthConfig - HashMap adapter

**circuit_breaker.rs:**
- CircuitBreakerConfig - HashMap adapter

## Next Steps
1. Check if orchestration/*.rs structs exist in tasker_v2.rs
2. Check if worker.rs structs exist in tasker_v2.rs
3. Identify pure re-exports (can be removed)
4. Identify missing V2 definitions (should be added)
5. Remove redundant files and update imports
