# TAS-61 Config Architecture Summary

## ✅ GOAL ACHIEVED: All TOML-Hydrated Configs Only in tasker_v2.rs

## Three-Layer Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│ Layer 1: TOML Storage (tasker_v2.rs) - SINGLE SOURCE OF TRUTH ✅   │
├─────────────────────────────────────────────────────────────────────┤
│ TaskerConfigV2 {                                                    │
│   common: CommonConfig                                              │
│   orchestration: Option<OrchestrationConfig>                        │
│   worker: Option<WorkerConfig>                                      │
│ }                                                                    │
│                                                                      │
│ Storage-Optimized Types:                                            │
│ - StepProcessingConfig { claim_timeout_seconds: u32, ... }          │
│ - HealthMonitoringConfig { health_check_interval_seconds: u32 }     │
│ - EventSystemTimingConfig { all fields: u32 }                       │
└─────────────────────────────────────────────────────────────────────┘
                              ↓ From<V2>
┌─────────────────────────────────────────────────────────────────────┐
│ Layer 2: Type Adapters - Runtime Performance ✅                     │
├─────────────────────────────────────────────────────────────────────┤
│ event_systems.rs:                                                   │
│ - EventSystemTimingConfig { health_check_interval: u64, ... }      │
│ - BackoffConfig { initial_delay_ms: u64, ... }                     │
│ - EventSystemProcessingConfig { max_concurrent: usize, ... }       │
│                                                                      │
│ worker.rs:                                                          │
│ - StepProcessingConfig { claim_timeout: u64, max_concurrent: usize }│
│ - HealthMonitoringConfig { health_check_interval: u64 }            │
│                                                                      │
│ Purpose: u32 (TOML) → u64/usize (Runtime Performance)              │
└─────────────────────────────────────────────────────────────────────┘
                              ↓ From<V2>
┌─────────────────────────────────────────────────────────────────────┐
│ Layer 3: Field Aggregators - Convenience Builders ⚠️                │
├─────────────────────────────────────────────────────────────────────┤
│ orchestration/step_enqueuer.rs:                                     │
│ - StepEnqueuerConfig (aggregates: batch_size, timeout, logging)    │
│                                                                      │
│ orchestration/step_result_processor.rs:                             │
│ - StepResultProcessorConfig (aggregates: queue_name, batch, etc.)  │
│                                                                      │
│ orchestration/task_claim_step_enqueuer.rs:                          │
│ - TaskClaimStepEnqueuerConfig (aggregates above two + more)        │
│                                                                      │
│ Purpose: Aggregate multiple V2 fields into convenient runtime struct│
└─────────────────────────────────────────────────────────────────────┘
```

## Files Analysis (23 total)

### ✅ Layer 1: V2 TOML Configs (1 file)
- tasker_v2.rs - **Single source of truth** ✅

### ✅ Layer 2: Type Adapters (6 files) - KEEP
1. circuit_breaker.rs - HashMap adapter
2. queues.rs - Adapter + utility struct
3. web.rs - WebAuthConfig adapter
4. mpsc_channels.rs - Adapter + deprecated structs
5. event_systems.rs - u32→u64/usize adapters ✅ documented
6. **worker.rs** - u32→u64/usize adapters (needs documentation)

### ⚠️ Layer 3: Field Aggregators (4 files) - EVALUATE
7. orchestration/step_enqueuer.rs
8. orchestration/step_result_processor.rs
9. orchestration/task_claim_step_enqueuer.rs
10. orchestration/mod.rs

**Question**: Do these provide enough value to justify maintaining them?

### ❌ Legacy Unused (parts of worker.rs) - REMOVE
- EventSystemConfig (name clash with event_systems.rs!)
- EventPublisherConfig
- EventSubscriberConfig
- EventProcessingConfig
- Legacy WorkerConfig struct

### ✅ Runtime Utilities (5 files) - KEEP
11. config_loader.rs
12. documentation.rs
13. queue_classification.rs
14. error.rs
15. merger.rs

## What We Found

### ✅ GOOD NEWS
1. **All TOML configs only in tasker_v2.rs** - Goal achieved!
2. **Type adapters properly separated** - Clear u32→u64/usize pattern
3. **No meaningless From implementations** - All adapters serve a purpose
4. **Name clash identified** - EventSystemConfig in two places

### ⚠️ DECISIONS NEEDED
1. **Keep or remove field aggregators?** (Layer 3)
   - Used in orchestration services
   - Aggregate V2 fields for convenience
   - Not TOML configs (built from V2)

2. **Migrate or remove legacy configs?**
   - orchestration/mod.rs OrchestrationConfig (different from V2)
   - orchestration/mod.rs OrchestrationSystemConfig (not in V2)

### ❌ TO REMOVE
1. worker.rs legacy event system configs (unused)
2. worker.rs legacy WorkerConfig struct (use V2 directly)

## Next Steps

### Option A: Conservative (Keep Aggregators)
1. ✅ Document worker.rs adapters (like event_systems.rs)
2. ✅ Document orchestration/* as field aggregators
3. ❌ Remove unused worker.rs legacy configs
4. ✅ Update architecture docs

**Pros**: Minimal code changes, preserves convenience
**Cons**: More files to maintain

### Option B: Aggressive (Remove Aggregators)
1. ✅ Document worker.rs adapters
2. ❌ Remove orchestration field aggregators
3. ✏️ Update services to access V2 fields directly
4. ❌ Remove unused worker.rs legacy configs
5. ✅ Update architecture docs

**Pros**: Fewer indirection layers, simpler mental model
**Cons**: More code changes, potential loss of convenience

### Recommendation: Start with Option A
- Document adapters and aggregators first
- Measure complexity/value of aggregators
- Remove aggregators later if they don't provide value
- Lower risk, easier to reverse

## Statistics

**TOML Configs**: 1 file (tasker_v2.rs) ✅
**Type Adapters**: 6 files ✅
**Field Aggregators**: 4 files ⚠️
**Legacy Unused**: ~5 structs in worker.rs ❌
**Runtime Utilities**: 5 files ✅

**Total**: 23 files analyzed, 41 structs examined

## Success Criteria ✅

- [x] All TOML-hydrated configs only in tasker_v2.rs
- [x] No duplicate TOML config definitions
- [x] Type adapters identified and categorized
- [x] Legacy configs identified for removal
- [x] Field aggregators identified for evaluation
- [x] Architecture patterns documented
