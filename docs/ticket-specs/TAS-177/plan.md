# TAS-177: Tonic gRPC Implementation Plan

## Overview

Add Tonic gRPC support to orchestration, worker, and client crates. This builds on TAS-76 (service layer extraction) and TAS-176 (resource-based authorization) to provide a gRPC API alongside the existing REST API.

**Branch**: `jcoletaylor/tas-177-tonic-and-grpc-orchestration-workers-and-client`

---

## Progress Summary

| Phase | Status | Notes |
|-------|--------|-------|
| Phase 1: Proto & Build | ✅ Complete | All proto files audited against REST API, build.rs, dependencies |
| Phase 1.5: Proto Audit | ✅ Complete | All protos verified to match actual REST API types 1:1 |
| Phase 2: Orchestration gRPC | ✅ Complete | All 7 services implemented and wired to server |
| Phase 3: Configuration | ✅ Complete | GrpcConfig added to OrchestrationConfig |
| Phase 4: Worker gRPC | ⬚ Not Started | |
| Phase 5: gRPC Client | ⬚ Not Started | |
| Phase 6: Testing | ⬚ Not Started | |
| Phase 7: Documentation | ⬚ Not Started | |

---

## Phase 1: Proto Definitions & Build Infrastructure ✅

### 1.1 Proto Directory Structure ✅

```
proto/
  tasker/
    v1/
      common.proto         # Pagination, timestamps, shared types
      tasks.proto          # Task CRUD (CreateTask, GetTask, ListTasks, CancelTask)
      steps.proto          # Step queries and manual resolution
      templates.proto      # Template discovery
      analytics.proto      # Performance metrics, bottlenecks
      dlq.proto            # Dead letter queue operations
      health.proto         # Health checks (liveness, readiness, detailed)
      config.proto         # Configuration endpoint
```

### 1.2 Workspace Dependencies ✅

**Cargo.toml** (workspace):
```toml
[workspace.dependencies]
tonic = "0.13"
tonic-build = "0.13"
prost = "0.13"
prost-types = "0.13"
```

### 1.3 Proto Compilation ✅

**tasker-shared/build.rs**: Compiles all proto files with file descriptor set for reflection.

### 1.4 Proto Module Exports ✅

**tasker-shared/src/proto/mod.rs**:
- Exports generated types via `tonic::include_proto!`
- Re-exports common types at module level
- Provides `server` and `client` submodules for ergonomic imports
- `FILE_DESCRIPTOR_SET` for gRPC reflection

**tasker-shared/src/proto/conversions.rs** (NEW):
- `From<TaskState> for proto::TaskState` and reverse
- `From<WorkflowStepState> for proto::StepState` and reverse
- `From<&TaskResponse> for proto::Task`
- Helper functions for `DateTime<Utc>` ↔ `Timestamp` (orphan rules prevent traits)

---

## Phase 2: Orchestration gRPC Server ✅

### 2.1 Module Structure

```
tasker-orchestration/src/grpc/
  mod.rs                   ✅
  server.rs                ✅ (all 7 services wired)
  state.rs                 ✅
  conversions.rs           ✅ (simplified - delegates to tasker-shared)
  services/
    mod.rs                 ✅
    tasks.rs               ✅
    steps.rs               ✅
    templates.rs           ✅
    analytics.rs           ✅
    dlq.rs                 ✅
    health.rs              ✅
    config.rs              ✅
  interceptors/
    mod.rs                 ✅
    auth.rs                ✅
```

### 2.2 Completed Implementation Details

**GrpcState** (`state.rs`):
- Mirrors `web/state.rs` pattern
- Shares `TaskService`, `SecurityService`, `OrchestrationCore`
- Provides `check_backpressure()` for service protection

**Auth Interceptor** (`interceptors/auth.rs`):
- Async-only authentication via `AuthInterceptor::authenticate()`
- Extracts Bearer token or API key from gRPC metadata
- Returns `SecurityContext` for permission checking
- **Note**: Removed sync interceptor (security theater - can't do real validation)

**Task Service** (`services/tasks.rs`):
- `CreateTask` - Returns full `TaskResponse` (same as REST GET)
- `GetTask` - With optional context inclusion
- `ListTasks` - Paginated with filters
- `CancelTask` - State-checked cancellation
- `StreamTaskStatus` - Server-side streaming for task updates
- Uses `From<&TaskResponse> for proto::Task` trait from tasker-shared

**Conversions** (`conversions.rs`):
- Re-exports `datetime_to_timestamp` from tasker-shared
- Provides string-based state conversions for database fields
- Uses `From` traits from tasker-shared for enum conversions

### 2.3 Key Design Decision: Unified Response Shape

**TaskService.create_task()** now returns `TaskResponse` (same as `get_task`):
- Follows REST best practice: POST returns same representation as GET
- Updated REST handler, gRPC handler, client, CLI, and tests

### 2.4 Completed Services

- [x] `TaskService` - Task CRUD, status streaming
- [x] `StepService` - Step queries, manual resolution, audit
- [x] `TemplateService` - Template discovery (list, get)
- [x] `AnalyticsService` - Performance metrics, bottleneck analysis
- [x] `DlqService` - Investigation tracking (list, stats, queue, staleness)
- [x] `HealthService` - Liveness, readiness, detailed health
- [x] `ConfigService` - Safe configuration retrieval

### 2.5 Remaining Work

- [ ] Add dual-server startup in `bin/server.rs` (REST + gRPC)

---

## Phase 3: Configuration ✅

### 3.1 GrpcConfig Added

**tasker-shared/src/config/tasker.rs**:
```rust
pub struct GrpcConfig {
    pub enabled: bool,              // default: true
    pub bind_address: String,       // default: "0.0.0.0:9090"
    pub tls_enabled: bool,          // default: false
    pub tls_cert_path: Option<String>,
    pub tls_key_path: Option<String>,
    pub keepalive_interval_seconds: u64,  // default: 30
    pub keepalive_timeout_seconds: u64,   // default: 10
    pub max_concurrent_streams: u32,      // default: 200
    pub max_frame_size: u32,              // default: 16384
    pub enable_reflection: bool,          // default: true
    pub enable_health_service: bool,      // default: true
}
```

**OrchestrationConfig** now includes:
```rust
pub grpc: Option<GrpcConfig>,
```

### 3.2 Config Files to Update

- [ ] `config/tasker/base/orchestration.toml` - Add `[orchestration.grpc]` section
- [ ] `config/tasker/base/worker.toml` - Add `[worker.grpc]` section
- [ ] `config/tasker/environments/*/orchestration.toml` - Environment overrides

---

## Phase 4: Worker gRPC Server ⬚

Same pattern as orchestration but with worker-specific services:
- Health, Templates, Config, Metrics

**tasker-worker/src/grpc/** structure mirrors orchestration.

---

## Phase 5: gRPC Client ⬚

### 5.1 Module Structure

```
tasker-client/src/api_clients/grpc/
  mod.rs
  orchestration_grpc_client.rs
  worker_grpc_client.rs
```

### 5.2 Feature Flag

```toml
[features]
default = []
grpc = ["tonic", "prost", "prost-types"]
```

### 5.3 Client API

Mirror REST client API surface:
- `OrchestrationGrpcClient::create_task()`
- `OrchestrationGrpcClient::get_task()`
- `OrchestrationGrpcClient::list_tasks()`
- `OrchestrationGrpcClient::cancel_task()`
- `OrchestrationGrpcClient::health_check()`

---

## Files Modified

### Proto Files (Audited & Rewritten)
| File | Status | Change |
|------|--------|--------|
| `proto/tasker/v1/common.proto` | ✅ | Fixed TaskState (12 states) and StepState (10 states) enums |
| `proto/tasker/v1/tasks.proto` | ✅ | Updated Task message with all TaskResponse fields |
| `proto/tasker/v1/steps.proto` | ✅ | Added audit trail, fixed manual action types |
| `proto/tasker/v1/templates.proto` | ✅ | Rewritten: removed invented Handler types |
| `proto/tasker/v1/analytics.proto` | ✅ | Rewritten: 2 RPCs matching actual API |
| `proto/tasker/v1/dlq.proto` | ✅ | Rewritten: investigation tracking (not message queue) |
| `proto/tasker/v1/health.proto` | ✅ | Rewritten: Kubernetes probe endpoints |
| `proto/tasker/v1/config.proto` | ✅ | Rewritten: typed safe config response |

### Shared Library
| File | Status | Change |
|------|--------|--------|
| `tasker-shared/Cargo.toml` | ✅ | Added prost, tonic-build, grpc-api feature |
| `tasker-shared/build.rs` | ✅ | Proto compilation with file descriptor set |
| `tasker-shared/src/lib.rs` | ✅ | Export proto module |
| `tasker-shared/src/proto/mod.rs` | ✅ | Generated code exports, updated for audit |
| `tasker-shared/src/proto/conversions.rs` | ✅ | From/Into traits, health conversions |
| `tasker-shared/src/config/tasker.rs` | ✅ | GrpcConfig struct |

### Orchestration gRPC Services
| File | Status | Change |
|------|--------|--------|
| `tasker-orchestration/Cargo.toml` | ✅ | Added tonic, prost |
| `tasker-orchestration/src/lib.rs` | ✅ | Export grpc module |
| `tasker-orchestration/src/grpc/mod.rs` | ✅ | Module exports |
| `tasker-orchestration/src/grpc/server.rs` | ✅ | All 7 services wired |
| `tasker-orchestration/src/grpc/state.rs` | ✅ | GrpcState with shared services |
| `tasker-orchestration/src/grpc/conversions.rs` | ✅ | Conversion helpers |
| `tasker-orchestration/src/grpc/interceptors/auth.rs` | ✅ | Async auth interceptor |
| `tasker-orchestration/src/grpc/services/tasks.rs` | ✅ | TaskService (6 RPCs) |
| `tasker-orchestration/src/grpc/services/steps.rs` | ✅ | StepService (4 RPCs) |
| `tasker-orchestration/src/grpc/services/templates.rs` | ✅ | TemplateService (2 RPCs) |
| `tasker-orchestration/src/grpc/services/analytics.rs` | ✅ | AnalyticsService (2 RPCs) |
| `tasker-orchestration/src/grpc/services/dlq.rs` | ✅ | DlqService (6 RPCs) |
| `tasker-orchestration/src/grpc/services/health.rs` | ✅ | HealthService (4 RPCs) |
| `tasker-orchestration/src/grpc/services/config.rs` | ✅ | ConfigService (1 RPC) |

### Other Changes
| File | Status | Change |
|------|--------|--------|
| `Cargo.toml` | ✅ | Added tonic/prost workspace dependencies |
| `tasker-orchestration/src/services/task_service.rs` | ✅ | Returns TaskResponse |
| `tasker-orchestration/src/web/handlers/tasks.rs` | ✅ | Uses TaskResponse |
| `tasker-orchestration/src/web/openapi.rs` | ✅ | Removed TaskCreationResponse |
| `tasker-orchestration/src/bin/server.rs` | ⬚ | TODO: Add gRPC server startup |
| `tasker-client/src/api_clients/orchestration_client.rs` | ✅ | Returns TaskResponse |
| `tasker-client/src/bin/cli/commands/task.rs` | ✅ | Uses TaskResponse fields |
| `config/tasker/base/orchestration.toml` | ⬚ | TODO: Add grpc section |

---

## Verification

### Unit Tests
- [x] Proto conversion roundtrips (From/Into traits)
- [x] Config tests pass with GrpcConfig
- [ ] Auth interceptor with various credential scenarios
- [ ] Permission checking integration

### Integration Tests
```bash
# Start server with gRPC enabled
cargo run --package tasker-orchestration --bin server

# Test with grpcurl
grpcurl -plaintext localhost:9090 list
grpcurl -plaintext -d '{"name":"test","namespace":"default","version":"1.0.0"}' \
  localhost:9090 tasker.v1.TaskService/CreateTask
```

### E2E Tests
- [ ] Task lifecycle via gRPC
- [ ] Auth required without credentials (UNAUTHENTICATED)
- [ ] Permission denied without permission (PERMISSION_DENIED)
- [ ] Parity with REST API responses

---

## Next Steps

1. ~~**Complete remaining gRPC services**~~ ✅ All 7 services implemented
2. **Add dual-server startup** - Integrate gRPC into `bin/server.rs` with `tokio::select!`
3. **Add config file sections** - Update TOML files with gRPC configuration
4. **Worker gRPC** - Mirror orchestration pattern for worker services
5. **gRPC client** - Feature-gated client implementations
6. **Testing** - Integration tests for gRPC endpoints
7. **Documentation** - Update CLAUDE.md and architecture docs

---

## Proto Audit Findings

During implementation, a comprehensive audit revealed significant issues with the original proto definitions. All protos were rewritten to match actual REST API types exactly.

### Issues Found and Fixed

#### 1. State Enum Mismatches (common.proto)
**Problem**: Proto enums had invented states and lossy mappings.
- Original had `TIMED_OUT`, `RETRYING` which don't exist in domain
- Missing states like `WaitingForDependencies`, `BlockedByFailures`, `ResolvedManually`
- Conversion code was mapping distinct states to same proto value (lossy)

**Fix**: 1:1 exact mapping for all 12 TaskState and 10 WorkflowStepState variants.

#### 2. DLQ Semantic Mismatch (dlq.proto)
**Problem**: Proto modeled traditional message queue DLQ with retry/discard operations.
- Had `RetryEntry`, `DiscardEntry`, `BulkRetry`, `BulkDiscard` RPCs
- Actual system is an **investigation tracking system**, not message queue

**Fix**: Complete rewrite with correct RPCs:
- `ListEntries`, `GetEntryByTask`, `UpdateInvestigation`
- `GetStats`, `GetInvestigationQueue`, `GetStalenessMonitoring`
- Added `DlqResolutionStatus`, `DlqReason`, `StalenessHealthStatus` enums

#### 3. Templates Invented Types (templates.proto)
**Problem**: Had `ListHandlers` RPC and `Handler` type that don't exist.

**Fix**: Rewritten to match actual API:
- `ListTemplates` → `NamespaceSummary`, `TemplateSummary`
- `GetTemplate` → `TemplateDetail`, `StepDefinition`

#### 4. Analytics Over-Designed (analytics.proto)
**Problem**: Had 5 detailed RPCs with percentile stats, queue metrics, throughput buckets.

**Fix**: Simplified to match actual 2 endpoints:
- `GetPerformanceMetrics` → `PerformanceMetrics`
- `GetBottleneckAnalysis` → `SlowStepInfo`, `SlowTaskInfo`, `ResourceUtilization`

#### 5. Health Complexity (health.proto)
**Problem**: Used enums for status, had `WatchHealth` streaming, `SystemMetrics` with CPU/memory.

**Fix**: Simplified to match Kubernetes probes:
- String-based status values ("healthy", "degraded", "unhealthy")
- 4 RPCs: `CheckHealth`, `CheckLiveness`, `CheckReadiness`, `CheckDetailedHealth`
- Removed unused `WatchHealth` streaming

#### 6. Config Invented RPCs (config.proto)
**Problem**: Had `GetConfigSection`, `ValidateConfig` that don't exist in REST.

**Fix**: Single `GetConfig` with typed `OrchestrationConfigResponse`:
- `ConfigMetadata`, `SafeAuthConfig`, `SafeCircuitBreakerConfig`
- `SafeDatabasePoolConfig`, `SafeMessagingConfig`

#### 7. Naming Convention (all protos)
**Problem**: Used `task_id`, `step_id` but codebase uses `task_uuid`, `step_uuid`.

**Fix**: All identifiers updated to `*_uuid` suffix for consistency.

### Key Lessons

1. **Proto-first doesn't mean proto-invented** - Protos must reflect actual API, not aspirational design
2. **Audit against REST handlers** - The REST handler is the source of truth for what the API actually does
3. **1:1 state mappings** - Never conflate distinct domain states; each must have unique proto representation
4. **Test roundtrips** - Conversion tests catch lossy mappings early

---

## Proto-Rust Type Validation

Since proto files are manually maintained and must match Rust types exactly, we need tooling to prevent drift.

### Validation Approach

1. **Compile-time tests** - Tests that enumerate expected fields on proto messages and verify they match Rust struct fields
2. **Source references** - Each proto message includes a comment referencing the Rust type it must match:
   ```protobuf
   // Task entity - matches TaskResponse from REST API
   // See: tasker-shared/src/types/api/orchestration.rs
   ```
3. **Roundtrip tests** - For state enums, verify lossless conversion (already implemented)

### Future Consideration

Investigate proto generation from Rust types if a mature tool emerges. Current options (`prost-reflect`, etc.) don't support this well.
