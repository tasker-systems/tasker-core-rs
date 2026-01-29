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
| Phase 2: Orchestration gRPC | ✅ Complete | All 7 services (20+ RPCs), SharedApiServices refactor, bootstrap refactoring |
| Phase 3: Configuration | ✅ Complete | GrpcConfig added, TOML files updated with environment overrides |
| Phase 4: Worker gRPC | ✅ Complete | Health, Templates, Config services on port 9100 |
| Phase 5: gRPC Client | ✅ Complete | OrchestrationGrpcClient, WorkerGrpcClient, "fail loudly" principle |
| Phase 6: Testing | ✅ Complete | Profile system, unified client, transport abstraction, gRPC tests |
| Phase 7: Documentation | ✅ Complete | Tenets, API security, deployment patterns, crate architecture |
| Phase 8: CI Integration | ✅ Complete | gRPC transport tests in CI, composite actions for DRY patterns |

### Recent Updates (Latest Session - Phase 7 Documentation & CI Refactoring)

- **CI Workflow Refactoring**:
  - Created 4 new composite actions for DRY CI patterns:
    - `.github/actions/setup-rust-cache/action.yml` - Rust cache configuration (used 11x)
    - `.github/actions/install-protobuf/action.yml` - Protobuf compiler for gRPC (used 5x)
    - `.github/actions/collect-service-logs/action.yml` - Log collection on failure (used 2x)
    - `.github/actions/generate-test-config/action.yml` - Test config generation (used 2x)
  - Refactored 5 workflow files: 197 total lines reduced
  - All 9 GitHub workflow files validated successfully

- **Code Quality Improvements**:
  - Fixed `large_enum_variant` clippy lint - boxed `UnifiedOrchestrationClient` variants (1728 → 16 bytes)
  - Fixed `manual_find` clippy lint - replaced manual loop with iterator pattern
  - Fixed `field_reassign_with_default` clippy lint - used struct initialization pattern

- **Feature Flag Consolidation**:
  - Added `grpc` to `tasker-client` default features for consistency with other crates
  - gRPC now enabled by default across client and orchestration/worker crates

- **Test Assertion Corrections**:
  - Fixed 8 E2E test assertions that expected wrong status value
  - Changed `"all_complete"` to `"complete"` (correct TaskState enum value)

### Previous Session - Phase 6 Testing Infrastructure

- **Client Profile System** (nextest-style profiles):
  - Added `Transport` enum (REST/gRPC) to ClientConfig
  - Profile support: `load_profile()`, `list_profiles()`, `find_profile_config_file()`
  - CLI `--profile` flag for transport selection
  - `.config/tasker-client.toml` with profiles: default, grpc, auth, grpc-auth, ci, local
  - Enhanced `config show` command with transport and profile info

- **Transport Abstraction Layer**:
  - `OrchestrationClient` trait for transport-agnostic client interface
  - `RestOrchestrationClient` and `GrpcOrchestrationClient` implementations
  - `UnifiedOrchestrationClient` enum for runtime transport selection
  - Fixed `form_urlencoded::Serializer` async-trait Send issue with scope blocks

- **IntegrationTestManager gRPC Support**:
  - `TASKER_TEST_TRANSPORT` env var for transport selection
  - `setup_grpc()`, `setup_grpc_orchestration_only()`, `setup_from_env()` methods
  - gRPC health validation with retry logic
  - `unified_client` field alongside legacy REST clients

- **gRPC Test Infrastructure**:
  - `tests/grpc_tests.rs` entry point with feature gate
  - `tests/grpc/health_tests.rs` - connectivity and health response tests
  - `tests/grpc/parity_tests.rs` - REST/gRPC response comparison tests

- **Cargo Make Tasks**:
  - `test-grpc` (tg), `test-grpc-health`, `test-grpc-parity` (tgp)
  - `test-e2e-grpc` (tge), `test-both-transports`

### Previous Session - Phase 5 gRPC Client

- **gRPC Client Implementation (OrchestrationGrpcClient & WorkerGrpcClient)**:
  - Created `tasker-client/src/grpc_clients/` module with full client implementations
  - `OrchestrationGrpcClient`: All 7 services (tasks, steps, templates, analytics, health, dlq, config)
  - `WorkerGrpcClient`: Health, templates, config services
  - `GrpcClientConfig` with auth, TLS, timeout, and keepalive settings
  - `AuthInterceptor` for Bearer token and API key authentication

- **Proto Schema Alignment (Rust as Source of Truth)**:
  - Updated `proto/tasker/v1/health.proto`: `PoolDetail` from 5 to 9 fields, `PoolUtilizationInfo` from repeated to named pools
  - Updated `proto/tasker/v1/analytics.proto`: `SystemHealthCounts` from 6 to 24 fields (all task/step states)
  - Updated `proto/tasker/v1/worker.proto`: `WorkerPoolDetail` to 9 fields, `CacheStats` field names aligned
  - All protos now match Rust domain types exactly - no fabricated or aggregated fields

- **"Fail Loudly" Principle Implementation**:
  - Added `ClientError::InvalidResponse` variant for protocol violations
  - Refactored all `unwrap_or_default()` patterns to `ok_or_else(|| ClientError::invalid_response(...))`
  - Centralized all proto-to-domain conversions in `tasker-client/src/grpc_clients/conversions.rs`
  - Distinguishes optional fields (legitimate None) from required fields (protocol violation)
  - Prevents "phantom data" - fabricated values that look valid but represent nothing

- **Documentation - Tenet #11: Fail Loudly**:
  - Added to `docs/principles/tasker-core-tenets.md` as the 11th tenet
  - Created `docs/principles/fail-loudly.md` with comprehensive guidance
  - Updated `docs/principles/defense-in-depth.md` with relationship to fail loudly
  - Origin documented as TAS-177 gRPC client refactoring

- **Server-Side Fixes**:
  - Updated `tasker-orchestration/src/grpc/services/analytics.rs`: `convert_system_health_counts` now maps all 24 fields
  - Fixed `tasker-shared/build.rs`: Clippy lint for `cloned_ref_to_slice_refs`
  - Removed unfulfilled `#[expect]` attributes in conversions.rs and tasks.rs

- **All tests passing**: `cargo make fix && cargo make code-quality` clean

### Previous Sessions

**Phase 4 - Worker gRPC**:
- Created `SharedWorkerServices` pattern mirroring orchestration
- Worker bootstrap refactoring with helper functions
- FFI bridge fixes for async `stop()` method (Python, Ruby, TypeScript)
- Config generation updates with gRPC sections
- Environment variable allowlist for gRPC bind addresses
- Dead code cleanup (protected_routes removal)

**Phases 1-3 - Orchestration gRPC**:
- Proto definitions audited and rewritten to match REST API exactly
- SharedApiServices architecture for REST/gRPC service sharing
- Bootstrap refactoring with 11 focused helper functions
- All 7 orchestration services (20+ RPCs)
- Dual-server startup with feature gates

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
      templates.proto      # Template discovery (orchestration)
      worker.proto         # Worker-specific: health, config, templates (TAS-177)
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

### 2.5 Dual-Server Startup ✅

**Implementation** (`bootstrap.rs`):
- Added `grpc_server_handle` field to `OrchestrationSystemHandle` (feature-gated)
- gRPC server starts via `GrpcServer::spawn()` alongside REST when enabled
- `GrpcState::from_app_state()` creates gRPC state from existing `AppState`
- Graceful shutdown stops gRPC server before event coordinator
- Feature-gated behind `grpc-api` feature flag

**Key Pattern**: gRPC server reuses the service layer via `GrpcState`, which wraps the same `TaskService`, `SecurityService`, etc. used by REST handlers.

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

### 3.2 Config Files ✅

**Base Configuration** (`config/tasker/base/orchestration.toml`):
```toml
[orchestration.grpc]
enabled = true
bind_address = "${TASKER_ORCHESTRATION_GRPC_BIND_ADDRESS:-0.0.0.0:9090}"
tls_enabled = false
keepalive_interval_seconds = 30
keepalive_timeout_seconds = 20
max_concurrent_streams = 200
max_frame_size = 16384
enable_reflection = true
enable_health_service = true
```

**Environment Overrides**:
- `development/orchestration.toml` - Reflection enabled for grpcurl debugging
- `production/orchestration.toml` - Reflection disabled, higher concurrency (500 streams)
- `test/orchestration.toml` - Localhost binding (127.0.0.1:9090)

### 3.3 Environment & Cluster Configuration ✅

**Single-Instance** (`config/dotenv/orchestration.env`):
```bash
TASKER_ORCHESTRATION_GRPC_BIND_ADDRESS=0.0.0.0:9090
```

**Single-Instance Worker** (`config/dotenv/rust-worker.env`):
```bash
TASKER_WORKER_GRPC_BIND_ADDRESS=0.0.0.0:9100
```

**Multi-Instance Cluster** (`config/dotenv/cluster.env`):
```bash
# Port allocation (mirrors REST pattern)
TASKER_ORCHESTRATION_GRPC_BASE_PORT=9090    # 9090-9099 for up to 10 instances
TASKER_WORKER_RUST_GRPC_BASE_PORT=9100      # 9100-9109 for up to 10 instances

# Test URLs
TASKER_TEST_ORCHESTRATION_GRPC_URLS=http://localhost:9090,http://localhost:9091
TASKER_TEST_ORCHESTRATION_GRPC_URL=http://localhost:9090   # Single-instance compat
TASKER_TEST_WORKER_RUST_GRPC_URLS=http://localhost:9100,http://localhost:9101
TASKER_TEST_WORKER_GRPC_URL=http://localhost:9100          # Single-instance compat
```

**Cluster Script** (`cargo-make/scripts/multi-deploy/start-cluster.sh`):
- Calculates dynamic gRPC port: `GRPC_PORT = GRPC_BASE_PORT + (instance - 1)`
- Exports `TASKER_ORCHESTRATION_GRPC_BIND_ADDRESS` for orchestration instances
- Exports `TASKER_WORKER_GRPC_BIND_ADDRESS` for worker-rust instances
- Shows both REST and gRPC ports in startup output

---

## Phase 4: Worker gRPC Server ✅

### 4.1 Overview

Mirror orchestration gRPC pattern for worker services. Worker has a smaller API surface than orchestration.

### 4.2 Proto Files (Reuse Common + Worker-Specific)

Worker reuses existing proto files:
- Reuse: `common.proto`, `health.proto`, `templates.proto`, `config.proto`

### 4.3 REST Endpoints Mirrored

| REST Endpoint | gRPC RPC | Status |
|---------------|----------|--------|
| `/health` | `HealthService.CheckHealth()` | ✅ |
| `/health/ready` | `HealthService.CheckReadiness()` | ✅ |
| `/health/live` | `HealthService.CheckLiveness()` | ✅ |
| `/health/detailed` | `HealthService.CheckDetailedHealth()` | ✅ |
| `/v1/templates` | `TemplateService.ListTemplates()` | ✅ |
| `/v1/templates/{ns}/{name}/{ver}` | `TemplateService.GetTemplate()` | ✅ |
| `/config` | `ConfigService.GetConfig()` | ✅ |

Note: `/metrics/*` is Prometheus format only, not suitable for gRPC.

### 4.4 Module Structure ✅

```
tasker-worker/src/grpc/
├── mod.rs              # Module exports
├── server.rs           # GrpcServer + GrpcServerHandle
├── state.rs            # WorkerGrpcState (wraps shared services)
├── interceptors/
│   ├── mod.rs
│   └── auth.rs         # Reuse auth pattern from orchestration
└── services/
    ├── mod.rs
    ├── health.rs       # HealthServiceImpl
    ├── templates.rs    # TemplateServiceImpl
    └── config.rs       # ConfigServiceImpl
```

### 4.5 Configuration (worker.toml) ✅

`[worker.grpc]` section added to base and environment configs:
```toml
[worker.grpc]
enabled = true
bind_address = "${TASKER_WORKER_GRPC_BIND_ADDRESS:-0.0.0.0:9100}"
tls_enabled = false
keepalive_interval_seconds = 30
keepalive_timeout_seconds = 20
max_concurrent_streams = 1000
max_frame_size = 16384
enable_reflection = true
enable_health_service = true
```

### 4.6 Port Allocation

| Worker Type | gRPC Port Range |
|-------------|-----------------|
| Rust Worker | 9100-9109 |
| Ruby Worker | 9200-9209 |
| Python Worker | 9300-9309 |
| TS Worker | 9400-9409 |

### 4.7 Implementation Checklist ✅

- [x] Add `grpc-api` feature to `tasker-worker/Cargo.toml`
- [x] Create `tasker-worker/src/grpc/` module structure
- [x] Implement `WorkerGrpcState` (wrap worker services)
- [x] Implement `GrpcServer` with health/templates/config services
- [x] Add auth interceptor (reuse orchestration pattern)
- [x] Update `WorkerBootstrap::bootstrap()` for dual-server startup
- [x] Add `[worker.grpc]` config section
- [x] Update environment files with gRPC port allocation
- [ ] Add gRPC integration tests (deferred to Phase 6)

---

## Phase 5: gRPC Client ✅

### 5.1 Overview

Feature-gated gRPC client implementations mirroring the existing REST clients. Returns the same domain types as REST clients for seamless switching.

### 5.2 Module Structure ✅

```
tasker-client/src/grpc_clients/
├── mod.rs                          # Module exports
├── common.rs                       # GrpcClientConfig, GrpcAuthConfig, AuthInterceptor
├── conversions.rs                  # Proto-to-domain conversions ("fail loudly")
├── orchestration_grpc_client.rs    # OrchestrationGrpcClient
└── worker_grpc_client.rs           # WorkerGrpcClient
```

### 5.3 Feature Flag ✅

```toml
[features]
default = ["grpc"]  # gRPC enabled by default
grpc = ["tonic", "tasker-shared/grpc-api"]
```

### 5.4 OrchestrationGrpcClient ✅

```rust
pub struct OrchestrationGrpcClient {
    task_client: TaskServiceClient<InterceptedService<Channel, AuthInterceptor>>,
    step_client: StepServiceClient<InterceptedService<Channel, AuthInterceptor>>,
    template_client: TemplateServiceClient<InterceptedService<Channel, AuthInterceptor>>,
    analytics_client: AnalyticsServiceClient<InterceptedService<Channel, AuthInterceptor>>,
    health_client: HealthServiceClient<InterceptedService<Channel, AuthInterceptor>>,
    dlq_client: DlqServiceClient<InterceptedService<Channel, AuthInterceptor>>,
    config_client: ConfigServiceClient<InterceptedService<Channel, AuthInterceptor>>,
    endpoint: String,
}
```

**Implemented Methods**:
- Task operations: `create_task()`, `get_task()`, `list_tasks()`, `cancel_task()`, `get_task_context()`
- Step operations: `get_step()`, `list_steps()`, `resolve_step()`, `get_step_audit()`
- Template operations: `list_templates()`, `get_template()`
- Analytics: `get_performance_metrics()`, `get_bottleneck_analysis()`
- Health: `health_check()`, `liveness_probe()`, `readiness_probe()`, `get_detailed_health()`
- DLQ: `list_dlq_entries()`, `get_dlq_entry()`, `update_investigation()`, `get_dlq_stats()`, `get_investigation_queue()`, `get_staleness_monitoring()`
- Config: `get_config()`

### 5.5 WorkerGrpcClient ✅

```rust
pub struct WorkerGrpcClient {
    health_client: WorkerHealthServiceClient<InterceptedService<Channel, AuthInterceptor>>,
    template_client: WorkerTemplateServiceClient<InterceptedService<Channel, AuthInterceptor>>,
    config_client: WorkerConfigServiceClient<InterceptedService<Channel, AuthInterceptor>>,
    endpoint: String,
}
```

**Implemented Methods**:
- Health: `health_check()`, `liveness_probe()`, `readiness_probe()`, `get_detailed_health()`
- Templates: `list_templates()`, `get_template()`
- Config: `get_config()`

### 5.6 Configuration ✅

```rust
pub struct GrpcClientConfig {
    pub endpoint: String,
    pub timeout: Duration,
    pub auth: Option<GrpcAuthConfig>,
}

pub enum GrpcAuthConfig {
    Bearer(String),
    ApiKey(String),
}
```

### 5.7 Connection Pattern ✅

```rust
// Simple connection
let client = OrchestrationGrpcClient::connect("http://localhost:9090").await?;

// With authentication
let client = OrchestrationGrpcClient::connect_with_auth(
    "http://localhost:9090",
    GrpcAuthConfig::Bearer("token".to_string()),
).await?;

// Full configuration
let config = GrpcClientConfig::new("http://localhost:9090")
    .with_auth(GrpcAuthConfig::ApiKey("key".to_string()))
    .with_timeout(Duration::from_secs(60));
let client = OrchestrationGrpcClient::with_config(config).await?;
```

### 5.8 Error Handling ✅

```rust
// tonic::Status maps to ClientError
impl From<tonic::Status> for ClientError {
    fn from(status: tonic::Status) -> Self {
        match status.code() {
            Code::NotFound => ClientError::NotFound(status.message().to_string()),
            Code::Unauthenticated => ClientError::Unauthorized(status.message().to_string()),
            Code::PermissionDenied => ClientError::Forbidden(status.message().to_string()),
            Code::Unavailable => ClientError::ServiceUnavailable(status.message().to_string()),
            _ => ClientError::RequestFailed(status.message().to_string()),
        }
    }
}

// Protocol violations use InvalidResponse
ClientError::InvalidResponse { field, reason }
```

### 5.9 "Fail Loudly" Principle ✅

**Key Design Decision**: Proto-to-domain conversions never fabricate data.

```rust
// WRONG - phantom data (creates false values)
let checks = response.checks.unwrap_or_else(|| ReadinessChecks::default());

// RIGHT - fail loudly (surfaces protocol violations)
let checks = response.checks.ok_or_else(|| {
    ClientError::invalid_response(
        "ReadinessResponse.checks",
        "Readiness response missing required health checks",
    )
})?;
```

**Rationale**: A client that returns fabricated data is worse than one that fails. Operators cannot distinguish "system is idle" from "data was missing" when defaults are silently applied.

### 5.10 Implementation Checklist ✅

- [x] Add `grpc` feature to `tasker-client/Cargo.toml`
- [x] Create `grpc_clients/` module structure
- [x] Implement `OrchestrationGrpcClient` with all service methods
- [x] Implement `WorkerGrpcClient`
- [x] Add auth interceptor for bearer/api-key
- [x] Add channel management (connection pooling via tonic)
- [x] Implement connection methods (connect, connect_with_auth, with_config)
- [x] Map gRPC errors to ClientError
- [x] Map proto-to-domain with "fail loudly" pattern
- [x] Align proto schemas with Rust domain types (24-field SystemHealthCounts, 9-field PoolDetail)
- [ ] CLI `--transport` flag (deferred - not blocking for Phase 5)
- [ ] Client tests (deferred to Phase 6)

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
| `tasker-orchestration/src/grpc/state.rs` | ✅ | GrpcState wraps `Arc<SharedApiServices>` |
| `tasker-orchestration/src/grpc/conversions.rs` | ✅ | Conversion helpers |
| `tasker-orchestration/src/grpc/interceptors/auth.rs` | ✅ | Async auth interceptor |
| `tasker-orchestration/src/grpc/services/tasks.rs` | ✅ | TaskService (6 RPCs) - uses `state.services.task_service` |
| `tasker-orchestration/src/grpc/services/steps.rs` | ✅ | StepService (4 RPCs) - uses `state.services.step_service` |
| `tasker-orchestration/src/grpc/services/templates.rs` | ✅ | TemplateService (2 RPCs) |
| `tasker-orchestration/src/grpc/services/analytics.rs` | ✅ | AnalyticsService (2 RPCs) |
| `tasker-orchestration/src/grpc/services/dlq.rs` | ✅ | DlqService (6 RPCs) |
| `tasker-orchestration/src/grpc/services/health.rs` | ✅ | HealthService (4 RPCs) |
| `tasker-orchestration/src/grpc/services/config.rs` | ✅ | ConfigService (1 RPC) |

### SharedApiServices Architecture (TAS-177 Refactor)
| File | Status | Change |
|------|--------|--------|
| `tasker-orchestration/src/services/mod.rs` | ✅ | Export `SharedApiServices` |
| `tasker-orchestration/src/services/shared.rs` | ✅ | **NEW** - Central service container |
| `tasker-orchestration/src/web/state.rs` | ✅ | AppState wraps `Arc<SharedApiServices>`, accessor methods |
| `tasker-orchestration/src/web/handlers/*.rs` | ✅ | Updated to use accessor methods |
| `tasker-orchestration/src/web/middleware/auth.rs` | ✅ | Uses `state.security_service()` |
| `tasker-orchestration/src/orchestration/bootstrap.rs` | ✅ | Creates SharedApiServices once, passes to both APIs |
| `tasker-orchestration/tests/web/test_infrastructure.rs` | ✅ | Uses SharedApiServices pattern |
| `tasker-orchestration/tests/web/auth_test_helpers.rs` | ✅ | Uses SharedApiServices pattern |

### Other Changes
| File | Status | Change |
|------|--------|--------|
| `Cargo.toml` | ✅ | Added tonic/prost workspace dependencies |
| `tasker-orchestration/src/services/task_service.rs` | ✅ | Returns TaskResponse |
| `tasker-orchestration/src/web/handlers/tasks.rs` | ✅ | Uses TaskResponse |
| `tasker-orchestration/src/web/openapi.rs` | ✅ | Removed TaskCreationResponse |
| `tasker-orchestration/src/bin/server.rs` | ✅ | Updated startup logging |
| `tasker-orchestration/src/orchestration/bootstrap.rs` | ✅ | Dual-server startup (REST + gRPC) |
| `tasker-client/src/api_clients/orchestration_client.rs` | ✅ | Returns TaskResponse |
| `tasker-client/src/bin/cli/commands/task.rs` | ✅ | Uses TaskResponse fields |
| `config/tasker/base/orchestration.toml` | ✅ | Added `[orchestration.grpc]` section |
| `config/tasker/environments/*/orchestration.toml` | ✅ | Environment-specific gRPC overrides |
| `config/dotenv/orchestration.env` | ✅ | Added `TASKER_ORCHESTRATION_GRPC_BIND_ADDRESS` |
| `config/dotenv/rust-worker.env` | ✅ | Added `TASKER_WORKER_GRPC_BIND_ADDRESS` |
| `config/dotenv/cluster.env` | ✅ | Added gRPC port allocation and test URLs |
| `cargo-make/scripts/multi-deploy/start-cluster.sh` | ✅ | Dynamic gRPC port calculation |

### Test Updates
| File | Status | Change |
|------|--------|--------|
| `tests/e2e/rust/*.rs` (8 files) | ✅ | Updated `.step_count` → `.total_steps` for TaskResponse |
| `tests/e2e/multi_instance/concurrent_task_creation_test.rs` | ✅ | Updated for TaskResponse |
| `tests/common/lifecycle_test_manager.rs` | ✅ | Kept `.step_count` for TaskInitializationResult |

### Worker gRPC & Bootstrap (Latest Session)
| File | Status | Change |
|------|--------|--------|
| `tasker-worker/src/services/shared.rs` | ✅ | **NEW** - SharedWorkerServices pattern |
| `tasker-worker/src/services/mod.rs` | ✅ | Export SharedWorkerServices |
| `tasker-worker/src/bootstrap.rs` | ✅ | Refactored with helper functions, dual-server startup |
| `tasker-worker/src/web/state.rs` | ✅ | WorkerAppState wraps `Arc<SharedWorkerServices>` |
| `tasker-worker/src/grpc/state.rs` | ✅ | WorkerGrpcState wraps `Arc<SharedWorkerServices>` |
| `tasker-worker/src/lib.rs` | ✅ | Fixed doc comment, module exports |
| `tasker-worker/Cargo.toml` | ✅ | Updated cargo-machete ignores |

### FFI Bridge Fixes (Async stop())
| File | Status | Change |
|------|--------|--------|
| `workers/python/src/bridge.rs` | ✅ | `runtime.block_on(worker.stop())` |
| `workers/ruby/ext/tasker_core/src/bridge.rs` | ✅ | `runtime.block_on(worker.stop())` |
| `workers/typescript/src-rust/bridge.rs` | ✅ | `runtime.block_on(worker.stop())` |
| `workers/rust/src/main.rs` | ✅ | Added `.await` to `worker.stop()` |

### Config Files Updated
| File | Status | Change |
|------|--------|--------|
| `config/tasker/generated/orchestration-test.toml` | ✅ | Regenerated with gRPC section |
| `config/tasker/generated/worker-test.toml` | ✅ | Regenerated with gRPC section |
| `config/tasker/generated/complete-test.toml` | ✅ | Regenerated with gRPC section |
| `config/tasker/generated/worker-test-embedded.toml` | ✅ | Copy with web/gRPC disabled |
| `config/tasker/generated/memcached-test.toml` | ✅ | Regenerated, restored tcp:// URL |
| `config/tasker/generated/auth-test.toml` | ✅ | Regenerated, restored API key arrays |
| `config/tasker/environments/test/orchestration.toml` | ✅ | Added config_endpoint_enabled |
| `config/tasker/environments/test/worker.toml` | ✅ | Added config_endpoint_enabled |
| `tasker-shared/src/config/config_loader.rs` | ✅ | Added gRPC bind address env vars to allowlist |

### Dead Code Cleanup (protected_routes)
| File | Status | Change |
|------|--------|--------|
| `tasker-shared/src/config/tasker.rs` | ✅ | Removed ProtectedRouteConfig, RouteAuthConfig, routes_map() |
| `tasker-shared/src/config/web.rs` | ✅ | Removed route matching methods, protected_routes field |
| `tasker-shared/src/types/web.rs` | ✅ | Removed AuthConfig struct and related methods |
| `tasker-orchestration/src/web/middleware/auth.rs` | ✅ | Removed dead tests for protected_routes |
| `config/tasker/base/orchestration.toml` | ✅ | Removed [[orchestration.web.auth.protected_routes]] |

### Phase 5: gRPC Client Files
| File | Status | Change |
|------|--------|--------|
| `tasker-client/src/grpc_clients/mod.rs` | ✅ | Module exports for gRPC clients |
| `tasker-client/src/grpc_clients/common.rs` | ✅ | GrpcClientConfig, GrpcAuthConfig, AuthInterceptor |
| `tasker-client/src/grpc_clients/conversions.rs` | ✅ | Proto-to-domain conversions with "fail loudly" |
| `tasker-client/src/grpc_clients/orchestration_grpc_client.rs` | ✅ | OrchestrationGrpcClient (7 services) |
| `tasker-client/src/grpc_clients/worker_grpc_client.rs` | ✅ | WorkerGrpcClient (3 services) |
| `tasker-client/src/error.rs` | ✅ | Added InvalidResponse variant for protocol violations |
| `tasker-client/src/lib.rs` | ✅ | Export grpc_clients module |
| `tasker-client/Cargo.toml` | ✅ | Added tonic dependency, grpc feature |

### Phase 5: Proto Schema Alignment
| File | Status | Change |
|------|--------|--------|
| `proto/tasker/v1/health.proto` | ✅ | PoolDetail: 5→9 fields, PoolUtilizationInfo: named pools |
| `proto/tasker/v1/analytics.proto` | ✅ | SystemHealthCounts: 6→24 fields (all task/step states) |
| `proto/tasker/v1/worker.proto` | ✅ | WorkerPoolDetail: 9 fields, CacheStats field alignment |
| `tasker-orchestration/src/grpc/services/analytics.rs` | ✅ | convert_system_health_counts: maps all 24 fields |
| `tasker-orchestration/src/grpc/conversions.rs` | ✅ | Removed unfulfilled #[expect] attribute |
| `tasker-orchestration/src/grpc/services/tasks.rs` | ✅ | Removed unfulfilled #[expect] attribute |
| `tasker-shared/build.rs` | ✅ | Fixed clippy cloned_ref_to_slice_refs lint |

### Phase 5: Documentation ("Fail Loudly" Principle)
| File | Status | Change |
|------|--------|--------|
| `docs/principles/tasker-core-tenets.md` | ✅ | Added Tenet #11: Fail Loudly |
| `docs/principles/fail-loudly.md` | ✅ | **NEW** - Comprehensive principle documentation |
| `docs/principles/defense-in-depth.md` | ✅ | Added relationship to Fail Loudly |
| `docs/principles/README.md` | ✅ | Added Fail Loudly to index |

### Phase 8: CI Integration & Code Quality
| File | Status | Change |
|------|--------|--------|
| `.github/actions/setup-rust-cache/action.yml` | ✅ | **NEW** - Reusable Rust cache configuration |
| `.github/actions/install-protobuf/action.yml` | ✅ | **NEW** - Protobuf compiler installation for gRPC |
| `.github/actions/collect-service-logs/action.yml` | ✅ | **NEW** - Service log collection + artifact upload |
| `.github/actions/generate-test-config/action.yml` | ✅ | **NEW** - Test configuration generation |
| `.github/workflows/test-integration.yml` | ✅ | Refactored: 632 → 529 lines, gRPC transport test stage |
| `.github/workflows/build-workers.yml` | ✅ | Refactored: 394 → 335 lines |
| `.github/workflows/code-quality.yml` | ✅ | Refactored: 90 → 77 lines |
| `.github/workflows/test-ruby-framework.yml` | ✅ | Refactored: 153 → 142 lines |
| `.github/workflows/test-python-framework.yml` | ✅ | Refactored: 139 → 128 lines |
| `tasker-client/Cargo.toml` | ✅ | Added `grpc` to default features |
| `tasker-client/src/transport.rs` | ✅ | Boxed `UnifiedOrchestrationClient` variants for clippy |
| `tasker-client/src/config.rs` | ✅ | Fixed manual_find with iterator pattern |
| `tests/common/integration_test_manager.rs` | ✅ | Fixed field_reassign_with_default |
| `tests/grpc/health_tests.rs` | ✅ | Fixed import paths |
| `tests/e2e/python/conditional_approval_test.rs` | ✅ | Fixed 4 status assertions (all_complete → complete) |
| `tests/e2e/ruby/conditional_approval_test.rs` | ✅ | Fixed 4 status assertions (all_complete → complete) |

---

## Phase 6: Testing ✅

**Detailed plan**: See `docs/ticket-specs/TAS-177/testing-plan.md`

### 6.1 Unit Tests

- [x] Proto conversion roundtrips (From/Into traits)
- [x] Config tests pass with GrpcConfig
- [x] All 290+ library tests passing
- [x] All 17 config tests pass with profile support
- [x] Transport abstraction tests pass

### 6.2 Client Profile System ✅

Added nextest-style profiles to `tasker-client`:
- [x] Add `Transport` enum (REST/gRPC) to `tasker-client/src/config.rs`
- [x] Add profile support to `ClientConfig` (`load_profile()`, `list_profiles()`, `find_profile_config_file()`)
- [x] Add `--profile` flag to CLI (`tasker-client/src/bin/tasker-cli.rs`)
- [x] Create `.config/tasker-client.toml` with profiles: default, grpc, auth, grpc-auth, ci, local
- [x] Enhanced `config show` command to display transport, profiles, and env var overrides

### 6.3 Transport Abstraction Layer ✅

Created unified transport abstraction:
- [x] `OrchestrationClient` trait in `tasker-client/src/transport.rs`
- [x] `RestOrchestrationClient` implementing the trait
- [x] `GrpcOrchestrationClient` implementing the trait (feature-gated)
- [x] `UnifiedOrchestrationClient` enum for runtime transport selection
- [x] `UnifiedOrchestrationClient::from_config()` for config-driven transport

### 6.4 IntegrationTestManager gRPC Support ✅

Extended `tests/common/integration_test_manager.rs`:
- [x] Added `transport` field to `IntegrationConfig`
- [x] Added gRPC URL fields (`orchestration_grpc_url`, `worker_grpc_url`)
- [x] Added `TASKER_TEST_TRANSPORT` env var support (rest/grpc)
- [x] Added `setup_grpc()` and `setup_grpc_orchestration_only()` methods
- [x] Added `setup_from_env()` for transport-agnostic tests
- [x] Added gRPC health validation methods
- [x] Added `client()` accessor for unified client
- [x] Added `is_grpc()` / `is_rest()` helpers

### 6.5 gRPC Test Infrastructure ✅

Created gRPC test files:
- [x] `tests/grpc_tests.rs` - Entry point with feature gate
- [x] `tests/grpc/mod.rs` - Module definitions
- [x] `tests/grpc/health_tests.rs` - Health endpoint tests
- [x] `tests/grpc/parity_tests.rs` - REST/gRPC response parity tests

### 6.6 Cargo Make Tasks ✅

Added to `Makefile.toml`:
- [x] `test-grpc` (alias: `tg`) - All gRPC tests
- [x] `test-grpc-health` - gRPC health endpoint tests
- [x] `test-grpc-parity` (alias: `tgp`) - REST/gRPC parity tests
- [x] `test-e2e-grpc` (alias: `tge`) - E2E tests with gRPC transport
- [x] `test-both-transports` - E2E with both REST and gRPC

### 6.7 Integration Tests (grpcurl)

```bash
# Test reflection (service discovery)
grpcurl -plaintext localhost:9090 list
grpcurl -plaintext localhost:9090 describe tasker.v1.TaskService

# Test health checks
grpcurl -plaintext localhost:9090 tasker.v1.HealthService/CheckLiveness
grpcurl -plaintext localhost:9090 tasker.v1.HealthService/CheckReadiness

# Test task creation (with auth)
grpcurl -plaintext -H "X-API-Key: test-api-key-full-access" \
  -d '{"name":"test","namespace":"default","version":"1.0.0","context":{}}' \
  localhost:9090 tasker.v1.TaskService/CreateTask
```

---

## Phase 7: Documentation ✅

### 7.1 CLAUDE.md Updates

- [x] Add gRPC section to Quick Reference
- [x] Document `--features grpc-api` flag
- [x] Add grpcurl examples
- [x] Document gRPC port allocation (9090 for orchestration, 9100+ for workers)

### 7.2 Architecture Documentation

- [x] Document SharedApiServices pattern (in plan.md)
- [x] Document feature gating strategy
- [x] Update deployment patterns doc with gRPC ports
- [x] Update crate architecture doc with gRPC module structure

### 7.3 API Documentation

- [x] Document authentication for gRPC (Bearer token, API key) - in api-security.md
- [x] Document error codes (gRPC Status → ClientError mapping)
- [x] Proto file reference in plan.md

### 7.4 Principles Documentation

- [x] "Fail Loudly" principle documented in `docs/principles/`
- [x] Added to tasker-core-tenets.md as Tenet #11

---

## Phase 8: CI Integration ✅

### 8.1 gRPC Transport Tests in CI

- [x] Added gRPC transport test stage to `test-integration.yml`
- [x] Tests run against gRPC health and parity endpoints
- [x] E2E tests run with `TASKER_TEST_TRANSPORT=grpc` env var
- [x] Service logs collected on failure

### 8.2 Composite Actions for DRY Patterns

- [x] `setup-rust-cache` - Rust cache with consistent key patterns
- [x] `install-protobuf` - Protobuf compiler for gRPC builds
- [x] `collect-service-logs` - Log collection + artifact upload
- [x] `generate-test-config` - Test configuration generation

### 8.3 Workflow Refactoring

- [x] 5 workflows refactored, 197 lines reduced total
- [x] All 9 GitHub workflow files validated

---

## Implementation Complete

1. ~~**Complete remaining gRPC services**~~ ✅ All 7 services implemented (20+ RPCs)
2. ~~**Add dual-server startup**~~ ✅ Integrated into `bootstrap.rs` with feature gate
3. ~~**Add config file sections**~~ ✅ TOML files updated with environment overrides
4. ~~**SharedApiServices refactor**~~ ✅ Central service container, bootstrap refactoring
5. ~~**Phase 4: Worker gRPC**~~ ✅ SharedWorkerServices pattern, dual-server startup, FFI bridges fixed
6. ~~**Phase 5: gRPC Client**~~ ✅ OrchestrationGrpcClient, WorkerGrpcClient, "fail loudly" principle
7. ~~**Phase 6: Testing**~~ ✅ Profile system, transport abstraction, gRPC tests, CI integration
8. ~~**Phase 7: Documentation**~~ ✅ CLAUDE.md, architecture docs, api-security, tenets
9. ~~**Phase 8: CI Integration**~~ ✅ gRPC test stage, composite actions, workflow refactoring

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

---

## Implementation Learnings

### TaskResponse Unification Cascade

When `TaskService::create_task()` was changed to return `TaskResponse` (same shape as GET), this required updates across multiple layers:

1. **Service layer** - `task_service.rs` now calls `get_task()` after creation
2. **REST handlers** - Already returned JSON, minimal change
3. **gRPC handlers** - Use same `TaskResponse` → proto conversion
4. **Client library** - `orchestration_client.rs` updated return type
5. **CLI** - Updated to use `TaskResponse` fields
6. **Tests** - Required bulk update: `.step_count` → `.total_steps`

**Key insight**: Tests using `TaskResponse` needed field name updates, but tests using internal `TaskInitializationResult` (which still has `step_count`) did not. Always verify which type a test is actually using before bulk replacements.

### Feature-Gated gRPC Integration

The gRPC server is conditionally compiled behind `grpc-api` feature:

```rust
#[cfg(feature = "grpc-api")]
pub grpc_server_handle: Option<crate::grpc::GrpcServerHandle>,
```

This required:
- Two versions of `OrchestrationSystemHandle::new()` with conditional compilation
- Conditional gRPC startup logic in `bootstrap()`
- Graceful degradation when feature is disabled

**Pattern**: Use `#[cfg(feature = "...")]` on struct fields and provide separate `new()` implementations rather than trying to make one implementation handle both cases.

### SharedApiServices Architecture ✅

**Problem**: The initial `GrpcState::from_app_state()` pattern created unnecessary coupling between REST and gRPC. Having two construction paths was unnecessary indirection since both hold Arc references to the same services.

**Solution**: Created `SharedApiServices` (`services/shared.rs`) as the single source of truth:

```rust
pub struct SharedApiServices {
    pub security_service: Option<Arc<SecurityService>>,
    pub write_pool: PgPool,
    pub read_pool: PgPool,
    pub circuit_breaker: WebDatabaseCircuitBreaker,
    pub task_service: TaskService,
    pub step_service: StepService,
    pub health_service: HealthService,
    pub template_query_service: TemplateQueryService,
    pub analytics_service: Arc<AnalyticsService>,
    pub orchestration_core: Arc<OrchestrationCore>,
    pub task_initializer: Arc<TaskInitializer>,
    pub orchestration_status: Arc<RwLock<OrchestrationStatus>>,
}
```

**Benefits**:
- Services are created once via `SharedApiServices::from_orchestration_core()`
- Both `AppState` (REST) and `GrpcState` (gRPC) hold `Arc<SharedApiServices>`
- No duplication of service construction logic
- Clean deployment flexibility: REST-only, gRPC-only, or both
- Bootstrap creates `SharedApiServices` once, passes to both APIs

**State Wrappers**:
- `AppState` holds `services: Arc<SharedApiServices>` + web-specific config
- `GrpcState` holds `services: Arc<SharedApiServices>` + gRPC-specific config
- Both use accessor methods: `state.task_service()`, `state.step_service()`, etc.

### Bootstrap Refactoring ✅

The `bootstrap()` method was refactored from a monolithic function into focused helper functions:

**Extracted Helper Functions**:
1. `initialize_orchestration_core()` - Creates SystemContext and OrchestrationCore, starts the core
2. `initialize_queues()` - Sets up orchestration-owned and namespace queues
3. `is_any_api_enabled()` - Checks feature flags + configuration
4. `create_shared_services_if_needed()` - Conditionally creates SharedApiServices only if APIs are enabled
5. `create_web_state_if_enabled()` - Creates AppState for REST API
6. `build_coordinator_config()` - Builds UnifiedCoordinatorConfig
7. `create_and_start_event_coordinator()` - Creates and starts event coordinator
8. `start_web_server()` - Spawns Axum web server
9. `start_grpc_server_if_enabled()` - Creates and spawns gRPC server
10. `setup_shutdown_handler()` - Creates shutdown channel
11. `create_system_handle()` - Wraps everything in OrchestrationSystemHandle

**Feature Dependency**: `grpc-api` now requires `web-api` in Cargo.toml because `SharedApiServices` depends on types from the web module (`WebDatabaseCircuitBreaker`, `OrchestrationStatus`). This constraint is documented with a note that these types could be refactored to a common module to enable gRPC-only deployments in the future.

**Feature Combinations** (mutually exclusive in bootstrap):
1. `#[cfg(feature = "grpc-api")]` - gRPC enabled (web-api automatically enabled)
2. `#[cfg(all(feature = "web-api", not(feature = "grpc-api")))]` - Web-only
3. `#[cfg(not(feature = "web-api"))]` - Neither API (orchestration-only)

### Environment Variable Layering Pattern

The dotenv system uses layered files assembled in order:
1. `base.env` - Common settings (RUST_LOG, paths)
2. `test.env` - Test environment (DATABASE_URL, JWT keys)
3. `cluster.env` - Multi-instance settings (port allocation, test URLs)
4. `orchestration.env` / `worker-*.env` - Service-specific settings

For gRPC, this means:
- `orchestration.env` sets default `TASKER_ORCHESTRATION_GRPC_BIND_ADDRESS=0.0.0.0:9090`
- `rust-worker.env` sets default `TASKER_WORKER_GRPC_BIND_ADDRESS=0.0.0.0:9100`
- `cluster.env` provides `TASKER_ORCHESTRATION_GRPC_BASE_PORT=9090` and `TASKER_WORKER_RUST_GRPC_BASE_PORT=9100` for dynamic allocation
- `start-cluster.sh` calculates per-instance ports: `BASE_PORT + (instance - 1)`

**Port Allocation Pattern:**
| Protocol | Orchestration | Rust Worker | Ruby Worker | Python Worker | TS Worker |
|----------|--------------|-------------|-------------|---------------|-----------|
| REST     | 8080-8089    | 8100-8109   | 8200-8209   | 8300-8309     | 8400-8409 |
| gRPC     | 9090-9099    | 9100-9109   | 9200-9209   | 9300-9309     | 9400-9409 |

---

## Implementation Learnings (Phase 4 Session)

### SharedWorkerServices Pattern ✅

**Problem**: Similar to orchestration, the worker needed to share services between REST and gRPC APIs without duplicating construction logic.

**Solution**: Created `SharedWorkerServices` (`services/shared.rs`) mirroring the orchestration pattern:

```rust
pub struct SharedWorkerServices {
    pub security_service: Option<Arc<SecurityService>>,
    pub health_service: Arc<WorkerHealthService>,
    pub template_query_service: Arc<TemplateQueryService>,
    pub config_response: WorkerConfigResponse,
    pub startup_time: std::time::Instant,
}
```

**Bootstrap Helpers Extracted**:
1. `create_shared_services()` - Creates SharedWorkerServices once
2. `create_web_state_if_enabled()` - Creates WorkerAppState wrapping shared services
3. `start_grpc_server_if_enabled()` - Creates and spawns gRPC server

### FFI Bridge Async Handling

**Problem**: The `Worker::stop()` method became async to properly await gRPC server shutdown, but FFI bridges (Python, Ruby, TypeScript) cannot directly call async methods.

**Solution**: Use `runtime.block_on()` pattern in all FFI bridges:

```rust
// Python/Ruby/TypeScript bridges
self.runtime.block_on(async {
    worker.stop().await;
});
```

This blocks the FFI thread while waiting for async shutdown to complete, which is acceptable since shutdown is a terminal operation.

### Dead Code Cleanup (protected_routes)

**Problem**: The `protected_routes` configuration was still being loaded and parsed but was never used - TAS-176 replaced it with SecurityContext-based permission checks in handlers.

**Solution**: Complete removal across all layers:
1. Removed `ProtectedRouteConfig` and `RouteAuthConfig` structs
2. Removed `routes_map()` method and route matching functions
3. Removed dead tests that tested the unused configuration
4. Updated all base config files to remove the configuration entries
5. Regenerated all generated config files

**Key insight**: Dead code includes configuration structures, not just executable code. Config schemas that load data that's never used are technical debt.

---

## Implementation Learnings (Phase 5 Session)

### "Fail Loudly" Principle Discovery

**Problem**: Initial gRPC client conversions used `unwrap_or_default()` pervasively, creating "phantom data" - values that look valid but represent nothing real.

**Example of the problem**:
```rust
// Server returns empty response (protocol violation)
// Client fabricates a response with zeros/empty strings
let health = BasicHealthResponse {
    status: response.status.unwrap_or_default(),  // Empty string looks like "healthy"?
    uptime_seconds: response.uptime_seconds.unwrap_or(0),  // 0 means just started?
};
```

Consumers cannot distinguish "data was missing" from "data was legitimately zero/empty". This breaks the trust contract.

**Solution**: Return explicit errors for missing required fields:
```rust
let checks = response.checks.ok_or_else(|| {
    ClientError::invalid_response(
        "ReadinessResponse.checks",
        "Readiness response missing required health checks",
    )
})?;
```

**Result**: Documented as Tenet #11 in `docs/principles/tasker-core-tenets.md`.

### Proto-Rust Alignment Cascade

When updating proto schemas to match Rust domain types, changes cascade through multiple layers:

1. **Proto file** (`health.proto`): Add fields to message
2. **Proto regeneration**: `cargo build` regenerates prost types
3. **Server-side conversions** (`analytics.rs`): Update domain→proto mapping
4. **Client-side conversions** (`conversions.rs`): Update proto→domain mapping

**Key insight**: The server-side conversion (`analytics.rs`) was still using old aggregated field names (`active_tasks`, `stuck_tasks`) after proto update. Both server and client conversions must be updated together.

### Centralized Conversions Pattern

**Problem**: Conversion functions were scattered across orchestration_grpc_client.rs and worker_grpc_client.rs, making it hard to:
- Audit for phantom data patterns
- Ensure consistency between clients
- Apply the "fail loudly" principle uniformly

**Solution**: Created `tasker-client/src/grpc_clients/conversions.rs` as the single location for all proto-to-domain conversions:
- 40+ conversion functions
- Each validates required fields
- Clear naming: `proto_xxx_to_domain()`
- Helper functions for common patterns (JSON parsing, timestamps)

### Required vs Optional Field Distinction

**Key insight**: Proto fields may be optional at the wire level but required at the domain level.

```rust
// Wire: message may have empty `checks` field
// Domain: ReadinessResponse REQUIRES checks to be meaningful

// This is a protocol violation, not a legitimate None
response.checks.ok_or_else(|| ClientError::invalid_response(...))
```

The distinction is semantic, not syntactic. The domain model defines what "required" means, not the proto schema.
