# Configuration Audit: tasker-shared/src/config/

## Executive Summary

**Total config files analyzed**: 16 files (excluding tasker_v2.rs and components/)
**Config structs found**: 62 structs/enums

## Classification Categories

### 1️⃣ Infrastructure (Keep - Not Configs)
**Purpose**: Core configuration loading/management infrastructure
**Action**: No changes needed

| File | Structs | Purpose |
|------|---------|---------|
| `config_loader.rs` | `ConfigLoader`, `ConfigManager` | Load and manage V2 configs |
| `merger.rs` | `ConfigMerger` | Merge base + environment TOML files |
| `merge.rs` | (merge logic) | TOML merging implementation |
| `error.rs` | `ConfigurationError` (enum) | Configuration error types |
| `documentation.rs` | `ConfigDocumentation`, `ParameterDocumentation`, `EnvironmentRecommendation` | Config docs generation |
| `mod.rs` | (module exports) | Public API surface |
| `tasker/mod.rs` | (module exports) | Tasker config exports |

**Recommendation**: ✅ **KEEP AS-IS** - Essential infrastructure

---

### 2️⃣ Adapter/Bridge Configs (Keep - Derived from V2)
**Purpose**: Convenience configs that derive values from TaskerConfigV2
**Pattern**: Have `From<TaskerConfigV2>` implementations
**Action**: These are useful adapters that flatten V2 config into use-case-specific structs

| File | Structs | Derives From V2 | Purpose |
|------|---------|-----------------|---------|
| `orchestration/step_enqueuer.rs` | `StepEnqueuerConfig` | ✅ Yes | Step enqueueing behavior config |
| `orchestration/step_result_processor.rs` | `StepResultProcessorConfig` | ✅ Yes | Result processing config |
| `orchestration/task_claim_step_enqueuer.rs` | `TaskClaimStepEnqueuerConfig` | ✅ Yes | Orchestration loop config |

**Fields** (StepEnqueuerConfig example):
- `max_steps_per_task`: from `execution.step_batch_size`
- `enqueue_delay_seconds`: hardcoded default
- `enable_detailed_logging`: from `orchestration.enable_performance_logging`
- `enqueue_timeout_seconds`: from `execution.step_execution_timeout_seconds`

**Recommendation**: ✅ **KEEP** - Useful adapters, consider documenting as derived configs

---

### 3️⃣ Legacy Configs (Potential Removal - Similar to V2)
**Purpose**: Pre-V2 configuration structs that overlap with V2
**Action**: Evaluate for removal, already have `From` implementations or are deprecated

#### A. Legacy Event Systems (`event_systems.rs`)

**Generic vs Non-Generic Issue**:
- **Legacy**: `EventSystemConfig<T = ()>` - Generic with metadata parameter
- **V2**: `EventSystemConfig` - Non-generic, metadata handled differently
  
| Legacy Struct | V2 Equivalent | Status |
|---------------|---------------|--------|
| `EventSystemConfig<T>` | `EventSystemConfig` (non-generic) | ⚠️ Different - Keep for compatibility |
| `EventSystemTimingConfig` | `EventSystemTimingConfig` | ✅ Same struct in V2 |
| `EventSystemProcessingConfig` | `EventSystemProcessingConfig` | ✅ Same struct in V2 |
| `EventSystemHealthConfig` | `EventSystemHealthConfig` | ✅ Same struct in V2 |
| `BackoffConfig` | `EventSystemBackoffConfig` | ⚠️ Different name |
| `OrchestrationEventSystemMetadata` | (empty placeholder) | Legacy compat only |
| `TaskReadinessEventSystemMetadata` | (empty placeholder) | Legacy compat only |
| `WorkerEventSystemMetadata` | `WorkerEventSystemMetadata` in V2 | ⚠️ Different structure |
| `InProcessEventConfig` | `InProcessEventsConfig` in V2 | ⚠️ Different name |
| `WorkerFallbackPollerConfig` | `FallbackPollerConfig` in V2 | ⚠️ Different name |
| `WorkerListenerConfig` | `ListenerConfig` in V2 | ⚠️ Different name |
| `WorkerResourceLimits` | `ResourceLimitsConfig` in V2 | ⚠️ Different name |

**Recommendation**: 
- ⚠️ **KEEP `EventSystemConfig<T>`** - Used by existing code, different from V2 non-generic version
- ✅ **RE-EXPORT** timing/processing/health configs from V2 (they're identical)
- 📝 **DOCUMENT** as legacy compatibility layer

#### B. Legacy Orchestration (`orchestration/mod.rs`)

| Legacy Struct | V2 Equivalent | Notes |
|---------------|---------------|-------|
| `OrchestrationConfig` | `OrchestrationConfig` in V2 | Different structure/fields |
| `OrchestrationSystemConfig` | Part of V2 `OrchestrationConfig` | Flattened in V2 |

**Recommendation**: ⚠️ **EVALUATE** - Check if still used, consider deprecation

#### C. Legacy Worker (`worker.rs`)

| Legacy Struct | V2 Equivalent | Notes |
|---------------|---------------|-------|
| `WorkerConfig` | `WorkerConfig` in V2 | Different structure |
| `StepProcessingConfig` | Part of V2 execution config | Consolidated in V2 |
| `EventSystemConfig` | Conflicts with event_systems.rs | Name collision |
| `EventPublisherConfig` | In V2 mpsc_channels | Moved to channels |
| `EventSubscriberConfig` | In V2 mpsc_channels | Moved to channels |
| `EventProcessingConfig` | In V2 event systems | Reorganized |
| `HealthMonitoringConfig` | Part of V2 health config | Consolidated |

**Recommendation**: ⚠️ **EVALUATE FOR REMOVAL** - Check usage, likely superseded by V2

---

### 4️⃣ Circuit Breaker Configs (`circuit_breaker.rs`)

**Question**: Are these duplicates of V2 or different?

| Struct | In V2? | Notes |
|--------|--------|-------|
| `CircuitBreakerConfig` | ✅ Yes - `CircuitBreakersConfig` | Different name, similar purpose |
| `CircuitBreakerGlobalSettings` | ❓ Check | Need to verify |
| `CircuitBreakerComponentConfig` | ❓ Check | Need to verify |

**Recommendation**: 🔍 **NEEDS INVESTIGATION** - Compare with V2 circuit breaker config

---

### 5️⃣ MPSC Channels (`mpsc_channels.rs`)

**All 19 structs in this file**:

| Struct | In V2? | Action |
|--------|--------|--------|
| `MpscChannelsConfig` | ✅ Yes | Check if duplicate |
| `OrchestrationChannelsConfig` | ✅ Yes (`OrchestrationMpscChannelsConfig`) | Different name |
| `OrchestrationCommandProcessorConfig` | ✅ Yes (nested in V2) | Check structure |
| `OrchestrationEventSystemsConfig` | ✅ Yes (nested in V2) | Check structure |
| `OrchestrationEventListenersConfig` | ✅ Yes (nested in V2) | Check structure |
| `TaskReadinessChannelsConfig` | ✅ Yes (nested in V2) | Check structure |
| `TaskReadinessEventChannelConfig` | ✅ Yes (nested in V2) | Check structure |
| `WorkerChannelsConfig` | ✅ Yes (`WorkerMpscChannelsConfig`) | Different name |
| `WorkerCommandProcessorConfig` | ✅ Yes (nested in V2) | Check structure |
| `WorkerEventSystemsConfig` | ✅ Yes (nested in V2) | Check structure |
| `WorkerEventSubscribersConfig` | ✅ Yes (nested in V2) | Check structure |
| `WorkerInProcessEventsConfig` | ✅ Yes (nested in V2) | Check structure |
| `WorkerEventListenersConfig` | ✅ Yes (nested in V2) | Check structure |
| `SharedChannelsConfig` | ✅ Yes (`SharedMpscChannelsConfig`) | Different name |
| `SharedEventPublisherConfig` | ✅ Yes (nested in V2) | Check structure |
| `SharedFfiConfig` | ✅ Yes (`FfiMpscChannelsConfig`) | Different name |
| `OverflowPolicyConfig` | ❓ TBD | Check if in V2 |
| `OverflowMetricsConfig` | ❓ TBD | Check if in V2 |
| `DropPolicy` (enum) | ❓ TBD | Check if in V2 |

**Recommendation**: 🔍 **DETAILED COMPARISON NEEDED** - Many likely duplicates with different names

---

### 6️⃣ Queue Classification (`queue_classification.rs`)

| Struct/Enum | Type | Purpose |
|-------------|------|---------|
| `QueueType` | Enum | Queue type classification |
| `QueueClassifier` | Logic | Queue classification logic |
| `ConfigDrivenMessageEvent<T>` | Enum | Message event types |

**Recommendation**: ✅ **KEEP** - Logic/utility types, not configuration structs

---

### 7️⃣ Queues Config (`queues.rs`)

| Struct | In V2? | Notes |
|--------|--------|-------|
| `QueuesConfig` | ✅ Yes | V2 has comprehensive queues config |
| `OrchestrationQueuesConfig` | ✅ Yes (nested) | Part of V2 |
| `PgmqBackendConfig` | ✅ Yes (`PgmqConfig`) | Different name |
| `RabbitMqBackendConfig` | ❌ No | Not in V2 - future feature? |
| `OrchestrationOwnedQueues` | ✅ Yes (nested) | Part of V2 |

**Recommendation**: 
- ✅ **RE-EXPORT** PGMQ/orchestration from V2
- ❓ **EVALUATE** RabbitMqBackendConfig - keep if planned feature

---

### 8️⃣ Web Config (`web.rs`)

| Struct | In V2? | Notes |
|--------|--------|-------|
| `WebConfig` | ✅ Yes (`OrchestrationWebConfig`) | Different name |
| `WebTlsConfig` | ✅ Yes (`TlsConfig`) | Different name |
| `WebDatabasePoolsConfig` | ✅ Yes (`DatabasePoolsConfig`) | Different name |
| `WebCorsConfig` | ✅ Yes (`CorsConfig`) | Different name |
| `WebAuthConfig` | ✅ Yes (`WebAuthConfig`) | Same in V2 |
| `RouteAuthConfig` | ✅ Yes (`ProtectedRoute`) | Different name |
| `WebRateLimitConfig` | ✅ Yes (`RateLimitingConfig`) | Different name |
| `WebResilienceConfig` | ✅ Yes (`ResilienceConfig`) | Different name |

**Recommendation**: ✅ **RE-EXPORT FROM V2** - All covered in V2, just different names

---

## Summary by Action Needed

### ✅ Keep As-Is (7 files)
- Infrastructure files (config_loader, merger, error, documentation, mod files)
- Adapter/bridge configs (orchestration adapters)
- Queue classification (logic, not config)

### 🔍 Needs Detailed Comparison (3 files)
- `circuit_breaker.rs` - Compare with V2
- `mpsc_channels.rs` - Many structs, likely duplicates
- `event_systems.rs` - Generic vs non-generic EventSystemConfig

### ✅ Can Re-export from V2 (2 files)
- `web.rs` - All structs exist in V2 with different names
- `queues.rs` - Most structs exist in V2

### ⚠️ Evaluate for Removal (2 files)
- `orchestration/mod.rs` - Check if still used
- `worker.rs` - Likely superseded by V2

---

## Next Steps

1. **Detailed field-by-field comparison** for:
   - `mpsc_channels.rs` vs V2 mpsc config
   - `circuit_breaker.rs` vs V2 circuit breaker config
   - `event_systems.rs` vs V2 event systems

2. **Usage analysis** - Find all imports/uses of legacy configs:
   ```bash
   grep -r "use.*config::(event_systems|worker|web|queues)" tasker-*/src
   ```

3. **Decision matrix** for each file:
   - If exact duplicate → Replace with re-export
   - If similar but different fields → Evaluate if differences needed
   - If unused → Mark for removal
   - If adapter pattern → Keep and document

4. **Deprecation plan** for anything being removed:
   - Add deprecation warnings
   - Update documentation
   - Create migration guide

