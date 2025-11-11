# Configuration Consolidation Plan

## Goal
Move all config structs to `tasker_v2.rs` while keeping business logic methods in separate adapter/extension files.

## Files to Consolidate

### 1. ✅ `circuit_breaker.rs` - ALREADY IN V2 (Keep as Adapter)

**Status**: V2 has equivalent structs with same field structure

**Structs in Legacy**:
- `CircuitBreakerConfig` (HashMap-based, flexible)
- `CircuitBreakerGlobalSettings`
- `CircuitBreakerComponentConfig`

**V2 Equivalents**:
- `CircuitBreakerConfig` (structured, fixed components)
- `GlobalCircuitBreakerSettings`
- `CircuitBreakerDefaultConfig` / `CircuitBreakerComponentConfig`

**Business Logic Methods**:
- `CircuitBreakerConfig::config_for_component()` - Get config for named component
- `CircuitBreakerComponentConfig::to_resilience_config()` - Convert to runtime type
- `CircuitBreakerGlobalSettings::to_resilience_config()` - Convert to runtime type

**Recommendation**: ✅ **KEEP AS ADAPTER**
- V2 has structured config (fixed component names)
- Legacy provides HashMap flexibility + conversion methods
- This is the intentional three-layer architecture:
  1. V2 TOML config (structured)
  2. circuit_breaker.rs (HashMap adapter + conversions)
  3. resilience/config.rs (runtime types with Duration)

---

### 2. ⚠️ `event_systems.rs` - PARTIAL V2 COVERAGE (Complex Migration)

**Status**: Some structs in V2, but generic `EventSystemConfig<T>` is different

**Structs in Legacy**:
- `EventSystemConfig<T = ()>` - **Generic** with type parameter for metadata
- `EventSystemTimingConfig` - ✅ Same in V2
- `EventSystemProcessingConfig` - ✅ Same in V2
- `EventSystemHealthConfig` - ✅ Same in V2
- `BackoffConfig` → `EventSystemBackoffConfig` in V2
- `OrchestrationEventSystemMetadata` - Empty placeholder
- `TaskReadinessEventSystemMetadata` - Empty placeholder
- `WorkerEventSystemMetadata` - Different from V2
- `InProcessEventConfig` → `InProcessEventsConfig` in V2
- `WorkerFallbackPollerConfig` → `FallbackPollerConfig` in V2
- `WorkerListenerConfig` → `ListenerConfig` in V2
- `WorkerResourceLimits` → `ResourceLimitsConfig` in V2

**V2 Equivalents**:
- `EventSystemConfig` - **Non-generic**, metadata handled differently
- All the timing/processing/health configs are identical

**Business Logic Methods**:
- `WorkerEventSystemConfig::get_listener_config()` - Extract listener config
- `EventSystemTimingConfig::notification_timeout_duration()` - Convert to Duration
- `BackoffConfig::max_delay_duration()` - Convert to Duration
- Multiple `From<V2>` conversion implementations

**Recommendation**: ⚠️ **COMPLEX - NEEDS CAREFUL MIGRATION PLAN**
- Generic `EventSystemConfig<T>` vs non-generic V2 version needs resolution
- Shared structs (Timing/Processing/Health) could be consolidated
- Metadata types may need to stay for backward compatibility

---

### 3. 📋 `mpsc_channels.rs` - LIKELY MATCHES V2 (Field-by-Field Check Needed)

**Status**: V2 has equivalent nested structure, need detailed comparison

**Structs in Legacy** (19 total):
- `MpscChannelsConfig` (top-level)
- `OrchestrationChannelsConfig`
- `OrchestrationCommandProcessorConfig`
- `OrchestrationEventSystemsConfig`
- `OrchestrationEventListenersConfig`
- `TaskReadinessChannelsConfig`
- `TaskReadinessEventChannelConfig`
- `WorkerChannelsConfig`
- `WorkerCommandProcessorConfig`
- `WorkerEventSystemsConfig`
- `WorkerEventSubscribersConfig`
- `WorkerInProcessEventsConfig`
- `WorkerEventListenersConfig`
- `SharedChannelsConfig`
- `SharedEventPublisherConfig`
- `SharedFfiConfig`
- `OverflowPolicyConfig`
- `OverflowMetricsConfig`
- `DropPolicy` (enum)

**V2 Naming**:
- V2 uses suffix `MpscChannelsConfig` for context configs:
  - `SharedMpscChannelsConfig` (not `SharedChannelsConfig`)
  - `OrchestrationMpscChannelsConfig` (not `OrchestrationChannelsConfig`)
  - Nested structs like `EventPublisherChannels`, `FfiChannels`, etc.

**Business Logic Methods** (minimal):
- `TaskReadinessEventChannelConfig::validate()` - Validation logic
- `OverflowMetricsConfig::new()` - Constructor

**Recommendation**: 📋 **NEEDS FIELD-BY-FIELD COMPARISON**
- Naming differs (suffix pattern)
- Structure likely matches but need to verify fields
- Minimal business logic, mostly just data

---

### 4. ✅ `queues.rs` - ALREADY IN V2 (Keep Utility Methods)

**Status**: V2 has equivalent structs

**Structs in Legacy**:
- `QueuesConfig`
- `OrchestrationQueuesConfig`
- `PgmqBackendConfig` → `PgmqConfig` in V2
- `RabbitMqBackendConfig` → `RabbitmqConfig` in V2
- `OrchestrationOwnedQueues`

**V2 Equivalents**: ✅ All present

**Business Logic Methods**:
- `QueuesConfig::get_queue_name()` - Dynamic queue name generation
- `QueuesConfig::get_orchestration_queue_name()` - Orchestration-specific
- `QueuesConfig::get_worker_queue_name()` - Worker-specific
- `QueuesConfig::step_results_queue_name()` - Convenience getter
- `QueuesConfig::task_requests_queue_name()` - Convenience getter
- `QueuesConfig::task_finalizations_queue_name()` - Convenience getter

**Recommendation**: ✅ **KEEP AS UTILITY ADAPTER**
- Structs already in V2
- Methods provide useful queue name utilities
- Pattern: Data in V2, utility methods in queues.rs

---

### 5. ✅ `web.rs` - ALREADY IN V2 (Keep Route Matching Logic)

**Status**: V2 has `OrchestrationWebConfig` with nested structs

**Structs in Legacy**:
- `WebConfig` → `OrchestrationWebConfig` in V2
- `WebTlsConfig` → `TlsConfig` in V2
- `WebDatabasePoolsConfig` → `DatabasePoolsConfig` in V2
- `WebCorsConfig` → `CorsConfig` in V2
- `WebAuthConfig` → `WebAuthConfig` in V2 (same name)
- `RouteAuthConfig` → `ProtectedRoute` in V2
- `WebRateLimitConfig` → `RateLimitingConfig` in V2
- `WebResilienceConfig` → `ResilienceConfig` in V2

**V2 Equivalents**: ✅ All present

**Business Logic Methods**:
- `WebAuthConfig::route_requires_auth()` - Route authorization check
- `WebAuthConfig::auth_type_for_route()` - Get auth type for route
- `WebAuthConfig::route_matches_pattern()` - Pattern matching with {param} support
- `From<V2>` conversion implementations

**Recommendation**: ✅ **KEEP AS ROUTE MATCHING ADAPTER**
- Structs already in V2
- Route matching logic is substantial (86 lines)
- Pattern: Data in V2, route matching logic in web.rs

---

### 6. ⚠️ `worker.rs` - DIFFERENT STRUCTURE (Evaluate for Removal)

**Status**: V2 has WorkerConfig but different structure

**Structs in Legacy**:
- `WorkerConfig`
- `StepProcessingConfig`
- `EventSystemConfig` - **Name collision** with event_systems.rs
- `EventPublisherConfig` - Moved to mpsc_channels in V2
- `EventSubscriberConfig` - Moved to mpsc_channels in V2
- `EventProcessingConfig` - Reorganized in V2
- `HealthMonitoringConfig`

**V2 Structure**:
- Different organization
- Event configs moved to mpsc_channels
- Processing configs consolidated

**Business Logic Methods**:
- `WorkerConfig::get_step_processing_config()` - Extract processing config

**Recommendation**: ⚠️ **CHECK USAGE, LIKELY REMOVABLE**
- Different structure suggests this is truly legacy
- Overlapping names suggest superseded by V2
- Need usage analysis to confirm safe removal

---

## Consolidation Strategy

### Phase 1: Keep Well-Designed Adapters ✅
**Files**: `circuit_breaker.rs`, `queues.rs`, `web.rs`

**Action**: Document as intentional adapters
- These provide valuable utility methods on top of V2 data
- They follow good architectural patterns (data vs logic separation)
- No consolidation needed

**Pattern**:
```
tasker_v2.rs           → Pure data structures with Builder + Validation
circuit_breaker.rs     → HashMap flexibility + to_resilience_config()
queues.rs              → Queue name generation utilities
web.rs                 → Route matching logic
```

### Phase 2: Detailed Comparison for mpsc_channels 📋
**File**: `mpsc_channels.rs`

**Action**: Field-by-field comparison with V2
1. Compare all 19 structs with V2 equivalents
2. Document field name differences
3. Document default value differences
4. Decide: re-export V2 or keep as adapter

### Phase 3: Complex Migration Planning ⚠️
**Files**: `event_systems.rs`, `worker.rs`

**Action**: Create migration plan
1. **event_systems.rs**: Generic vs non-generic resolution
2. **worker.rs**: Usage analysis, likely remove

---

## Recommended Next Steps

1. ✅ **Accept Adapter Pattern** for circuit_breaker, queues, web
   - These are well-designed and serve clear purposes
   - No action needed beyond documentation

2. 📋 **Compare mpsc_channels.rs** field-by-field
   - Create detailed comparison table
   - Decide on consolidation approach

3. ⚠️ **Analyze event_systems.rs** migration complexity
   - Understand generic vs non-generic requirements
   - Plan migration if beneficial

4. ⚠️ **Check worker.rs usage**
   - Search for references
   - Remove if unused

---

## Summary

**Keep as Adapters** (3 files):
- `circuit_breaker.rs` - Three-layer architecture (V2 → HashMap → Runtime)
- `queues.rs` - Queue name utilities
- `web.rs` - Route matching logic

**Investigate** (1 file):
- `mpsc_channels.rs` - Likely matches V2, need field comparison

**Migrate or Remove** (2 files):
- `event_systems.rs` - Complex generic vs non-generic
- `worker.rs` - Likely legacy, check usage

**Architecture Pattern Validated**:
The separation of data (V2) from logic (adapters) is **intentional and good design**.
