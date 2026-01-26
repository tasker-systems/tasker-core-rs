# TAS-76: Axum Handler Refactoring and Tonic gRPC Foundations

## Overview

Extract business logic from orchestration REST handlers into services following the TAS-168 analytics pattern (`Handler -> Service -> QueryService -> Database`). This enables future gRPC endpoints to share the same business logic.

## Current State Analysis

### Orchestration (tasker-orchestration) - 25 endpoints

**Heavy Logic (needs extraction):**
- `tasks.rs` (716 lines, 4 endpoints) - Input validation, backpressure checking, response assembly, error classification
- `steps.rs` (691 lines, 4 endpoints) - State machine routing, manual resolution, audit history
- `health.rs` (583 lines, 5 endpoints) - Multiple health check implementations

**Already Well-Delegated (reference patterns):**
- `analytics.rs` - Uses `AnalyticsService -> AnalyticsQueryService` pattern
- `dlq.rs` - Delegated to model layer
- `registry.rs`, `config.rs` - Minimal logic

### Worker (tasker-worker) - 12 endpoints

Already follows TAS-77 service-layer pattern - handlers are thin wrappers to `HealthService`, `MetricsService`, `TemplateQueryService`, `ConfigQueryService`.

### Auth/Permissions

- `SecurityContext` is Axum-specific (`FromRequestParts` impl)
- `require_permission()` duplicated in orchestration and worker
- Need framework-agnostic extraction for gRPC compatibility

---

## Implementation Plan

### Phase 1: Auth Service Extraction (Foundation)

Extract framework-agnostic auth/permission services to `tasker-shared`.

**Create:**
```
tasker-shared/src/auth/
    mod.rs
    authentication_service.rs   # Wraps SecurityService with transport-agnostic API
    permission_service.rs       # Consolidates require_permission() logic
    errors.rs                   # AuthError, PermissionError types
```

**Modify:**
- `tasker-shared/src/types/security.rs` - Feature-gate Axum's `FromRequestParts` impl
- `tasker-orchestration/src/web/middleware/auth.rs` - Delegate to AuthenticationService
- `tasker-orchestration/src/web/middleware/permission.rs` - Delegate to PermissionService
- `tasker-worker/src/web/middleware/auth.rs` - Use shared PermissionService

**Key Design:**
```rust
// tasker-shared/src/auth/authentication_service.rs
impl AuthenticationService {
    pub async fn authenticate_bearer(&self, token: &str) -> Result<SecurityContext, AuthError>;
    pub fn authenticate_api_key(&self, key: &str) -> Result<SecurityContext, AuthError>;
}

// tasker-shared/src/auth/permission_service.rs
impl PermissionService {
    pub fn require_permission(ctx: &SecurityContext, perm: Permission) -> Result<(), PermissionError>;
}
```

### Phase 2: Task Service Extraction (Highest Priority)

Extract task business logic from `tasks.rs` (716 lines -> ~50 lines).

**Create:**
```
tasker-orchestration/src/services/
    task_query_service.rs   # Database queries, execution context assembly
    task_service.rs         # Business logic, error classification, response building
```

**TaskQueryService responsibilities:**
- `get_task_with_context(uuid: Uuid) -> Result<TaskWithContext, TaskQueryError>`
- `list_tasks_with_context(query: TaskListQuery) -> Result<PaginatedTasksWithContext, TaskQueryError>`
- Pure database queries using `SqlFunctionExecutor`

**TaskService responsibilities:**
- `create_task(request: TaskRequest) -> Result<TaskCreationResponse, TaskError>`
- `get_task(uuid: Uuid) -> Result<TaskResponse, TaskError>`
- `list_tasks(query: TaskListQuery) -> Result<TaskListResponse, TaskError>`
- `cancel_task(uuid: Uuid) -> Result<TaskResponse, TaskError>`
- Input validation, backpressure checking, error classification

**Handler after refactoring:**
```rust
pub async fn create_task(
    State(state): State<AppState>,
    security: SecurityContext,
    Json(request): Json<TaskRequest>,
) -> ApiResult<Json<TaskCreationResponse>> {
    require_permission(&security, Permission::TasksCreate)?;
    state.task_service.create_task(request).await.map(Json).map_err(Into::into)
}
```

### Phase 3: Step Service Extraction

Extract step business logic from `steps.rs`.

**Create:**
```
tasker-orchestration/src/services/
    step_query_service.rs   # Database queries for steps
    step_service.rs         # State machine routing, manual resolution
```

**StepService responsibilities:**
- `list_task_steps(task_uuid: Uuid) -> Result<Vec<StepResponse>, StepError>`
- `get_step(task_uuid: Uuid, step_uuid: Uuid) -> Result<StepResponse, StepError>`
- `resolve_step_manually(task_uuid: Uuid, step_uuid: Uuid, action: StepManualAction) -> Result<StepResponse, StepError>`
- `get_step_audit(task_uuid: Uuid, step_uuid: Uuid) -> Result<Vec<StepAuditResponse>, StepError>`

### Phase 4: Health Service Extraction

Extract health check logic from `health.rs` following worker's `HealthService` pattern.

**Create:**
```
tasker-orchestration/src/services/health/
    mod.rs
    service.rs              # Main health service
    checks.rs               # Individual health check implementations
```

**OrchestrationHealthService responsibilities:**
- `basic_health() -> BasicHealthResponse`
- `readiness() -> Result<DetailedHealthResponse, DetailedHealthResponse>`
- `liveness() -> BasicHealthResponse`
- `detailed_health() -> DetailedHealthResponse`

### Phase 5: Update AppState and mod.rs

**Modify `tasker-orchestration/src/services/mod.rs`:**
```rust
mod analytics_query_service;
mod analytics_service;
mod task_query_service;      // NEW
mod task_service;            // NEW
mod step_query_service;      // NEW
mod step_service;            // NEW
pub mod health;              // NEW

pub use analytics_query_service::AnalyticsQueryService;
pub use analytics_service::AnalyticsService;
pub use task_query_service::TaskQueryService;
pub use task_service::TaskService;
pub use step_query_service::StepQueryService;
pub use step_service::StepService;
pub use health::OrchestrationHealthService;
```

**Modify `tasker-orchestration/src/web/state.rs`:**
```rust
pub struct AppState {
    // Existing fields...
    pub task_service: TaskService,
    pub step_service: StepService,
    pub health_service: OrchestrationHealthService,
}
```

---

## Critical Files

| File | Purpose |
|------|---------|
| `tasker-orchestration/src/services/analytics_service.rs` | Reference pattern (250 lines) |
| `tasker-orchestration/src/web/handlers/tasks.rs` | Primary refactoring target (716 lines) |
| `tasker-orchestration/src/web/handlers/steps.rs` | Secondary target (691 lines) |
| `tasker-orchestration/src/web/handlers/health.rs` | Third target (583 lines) |
| `tasker-shared/src/types/security.rs` | Auth types to refactor |
| `tasker-orchestration/src/web/middleware/permission.rs` | Permission logic to consolidate |
| `tasker-worker/src/worker/services/health/service.rs` | Reference for HealthService pattern (706 lines) |

---

## Implementation Order

| Phase | Priority | Status | Dependencies |
|-------|----------|--------|--------------|
| 1. Auth Service | High | **DONE** | None |
| 2. Task Service | High | **DONE** | Phase 1 |
| 3. Step Service | Medium | **DONE** | Phase 1 |
| 4. Health Service | Medium | **DONE** | Phase 1 |
| 5. AppState Updates | High | **DONE** | Phases 2-4 |

**Progress Summary:**
- Phase 1: `require_permission` consolidated in `tasker-shared/src/services/permission_service.rs`
- Phase 2: `TaskService` and `TaskQueryService` created in `tasker-orchestration/src/services/`
- Phase 3: `StepService` and `StepQueryService` created in `tasker-orchestration/src/services/`
- Phase 4: `HealthService` created in `tasker-orchestration/src/services/health/`
- Phase 5: All services wired into `AppState`; handlers refactored to use services

**Handler Line Count Reductions:**
- `tasks.rs`: 716 → 219 lines (69% reduction)
- `steps.rs`: 691 → 259 lines (63% reduction)
- `health.rs`: 583 → 92 lines (84% reduction)

**All phases complete!**

---

## Testing Strategy

### Service-Level Tests (sqlx::test)
- All service tests use `#[sqlx::test]` with real database connections
- Follow existing patterns in `tests/integration/` and `tests/e2e/`
- Real database pools, not mocks - the compiler already validates type correctness

### Route/Handler Tests
- Follow auth service test patterns: create Axum test instances
- Test route correctness, permission checking, request/response shaping
- Located in service-specific test modules or `tests/web/`

### Integration Tests
- Rely on existing `tests/integration/` infrastructure
- No changes to existing test patterns

### E2E Tests
- Rely on existing `tests/e2e/` infrastructure
- Existing tests validate API behavior end-to-end

### Verification Commands
```bash
# Build with all features
cargo build --all-features

# Run clippy
cargo clippy --all-targets --all-features

# Run unit tests (DB + messaging only)
cargo make test-rust-unit

# Run E2E tests (requires services)
cargo make test-rust-e2e
```

---

## Success Criteria

1. All orchestration handlers reduced to <50 lines
2. Services testable independently of HTTP layer
3. `require_permission()` unified in `tasker-shared`
4. No API contract changes
5. All existing tests pass
6. Same business logic available for future gRPC endpoints

---

## Future: gRPC Foundation (Not in this PR)

Once services are extracted, adding gRPC becomes straightforward:
```
Axum Handler ─────┐
                  ├──> TaskService ──> TaskQueryService ──> Database
Tonic Service ────┘
```

The service layer will be the same for both REST and gRPC.
