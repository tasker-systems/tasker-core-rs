# TAS-176: Resource-Based Authorization with Handler Wrappers

## Summary

Move permission checking from handler methods to declarative resource+action requirements at route definition. This ensures authorization happens BEFORE body deserialization and provides a protocol-agnostic permission model reusable for REST and gRPC.

**Current flow:** JWT validation → body deserialization → handler permission check → business logic
**New flow:** JWT validation → handler wrapper permission check → body deserialization → business logic

**Key insight:** We're protecting *resources and actions*, not *routes*. Routes just declare which resource+action they need.

---

## Architecture

### Resource & Action Enums (`tasker-shared`)

New module `tasker-shared/src/types/resources.rs`:

```rust
/// Resources in the system (maps to permission prefix)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Resource {
    Tasks,      // "tasks:*"
    Steps,      // "steps:*"
    Dlq,        // "dlq:*"
    Templates,  // "templates:*"
    System,     // "system:*"
    Worker,     // "worker:*"
    Health,     // Public - no permission needed
    Metrics,    // Public - no permission needed
    Docs,       // Public - no permission needed
}

/// Actions that can be performed on resources
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Create,     // POST new resource
    Read,       // GET single resource
    List,       // GET collection
    Update,     // PUT/PATCH resource
    Delete,     // DELETE resource
    // Resource-specific actions
    Cancel,     // tasks:cancel
    Resolve,    // steps:resolve
    Stats,      // dlq:stats
    Validate,   // templates:validate
    ConfigRead, // system:config_read, worker:config_read
    AnalyticsRead, // system:analytics_read
    HandlersRead,  // system:handlers_read (legacy)
}

/// Combines Resource + Action, can resolve to Permission
#[derive(Debug, Clone, Copy)]
pub struct ResourceAction {
    pub resource: Resource,
    pub action: Action,
}

impl ResourceAction {
    pub const fn new(resource: Resource, action: Action) -> Self {
        Self { resource, action }
    }

    /// Returns None for public resources, Some(Permission) for protected
    pub fn required_permission(&self) -> Option<Permission> {
        match self.resource {
            Resource::Health | Resource::Metrics | Resource::Docs => None,
            _ => Some(self.to_permission()),
        }
    }

    /// Convert to Permission enum (panics for public resources)
    fn to_permission(&self) -> Permission {
        match (self.resource, self.action) {
            (Resource::Tasks, Action::Create) => Permission::TasksCreate,
            (Resource::Tasks, Action::Read) => Permission::TasksRead,
            (Resource::Tasks, Action::List) => Permission::TasksList,
            (Resource::Tasks, Action::Cancel) => Permission::TasksCancel,
            // ... etc for all combinations
        }
    }
}
```

### Handler Wrapper Function

New module `tasker-shared/src/web/authorize.rs`:

```rust
use axum::{extract::Request, response::Response, handler::Handler};
use std::future::Future;

/// Wraps a handler with resource+action authorization.
///
/// Usage in routes.rs:
/// ```rust
/// .route("/tasks", post(authorize(Resource::Tasks, Action::Create, handlers::create_task)))
/// ```
pub fn authorize<H, T, S>(
    resource: Resource,
    action: Action,
    handler: H,
) -> impl Handler<T, S>
where
    H: Handler<T, S>,
    S: Clone + Send + Sync + 'static,
{
    AuthorizedHandler {
        resource_action: ResourceAction::new(resource, action),
        inner: handler,
    }
}

struct AuthorizedHandler<H> {
    resource_action: ResourceAction,
    inner: H,
}

impl<H, T, S> Handler<T, S> for AuthorizedHandler<H>
where
    H: Handler<T, S>,
    S: Clone + Send + Sync + 'static,
{
    type Future = /* ... */;

    fn call(self, req: Request, state: S) -> Self::Future {
        // 1. Extract SecurityContext from request extensions
        // 2. If resource is public (Health/Metrics/Docs) → proceed
        // 3. If auth disabled → proceed
        // 4. Check has_permission(required_permission)
        // 5. If yes → call inner handler
        // 6. If no → return 403 Forbidden
    }
}
```

### No Static Registry Needed

Routes declare requirements inline - the compiler ensures all protected routes have authorization:

```rust
// If you forget authorize(), the handler still works but isn't protected
// Code review / tests catch this, not a static registry
```

---

## Files to Create

| File | Purpose |
|------|---------|
| `tasker-shared/src/types/resources.rs` | `Resource`, `Action`, `ResourceAction` enums/structs |
| `tasker-shared/src/web/authorize.rs` | `authorize()` handler wrapper function |
| `tasker-shared/src/web/mod.rs` | Web utilities module (new) |

## Files to Modify

| File | Change |
|------|--------|
| `tasker-shared/src/types/mod.rs` | Export `resources` module |
| `tasker-shared/src/lib.rs` | Export `web` module (feature-gated) |
| `tasker-orchestration/src/web/routes.rs` | Wrap handlers with `authorize()` |
| `tasker-worker/src/web/routes.rs` | Wrap handlers with `authorize()` |
| `docs/auth/permissions.md` | Document resource-based model |

---

## Route Transformations

### Orchestration `routes.rs` (Before → After)

```rust
// BEFORE
pub fn api_v1_routes() -> Router<AppState> {
    Router::new()
        .route("/tasks", post(handlers::tasks::create_task))
        .route("/tasks", get(handlers::tasks::list_tasks))
        .route("/tasks/{uuid}", get(handlers::tasks::get_task))
        .route("/tasks/{uuid}", delete(handlers::tasks::cancel_task))
        // ...
}

// AFTER
use tasker_shared::web::authorize;
use tasker_shared::types::resources::{Resource, Action};

pub fn api_v1_routes() -> Router<AppState> {
    Router::new()
        // Tasks API
        .route("/tasks", post(authorize(Resource::Tasks, Action::Create, handlers::tasks::create_task)))
        .route("/tasks", get(authorize(Resource::Tasks, Action::List, handlers::tasks::list_tasks)))
        .route("/tasks/{uuid}", get(authorize(Resource::Tasks, Action::Read, handlers::tasks::get_task)))
        .route("/tasks/{uuid}", delete(authorize(Resource::Tasks, Action::Cancel, handlers::tasks::cancel_task)))

        // Steps API
        .route("/tasks/{uuid}/workflow_steps",
            get(authorize(Resource::Steps, Action::List, handlers::steps::list_task_steps)))
        .route("/tasks/{uuid}/workflow_steps/{step_uuid}",
            get(authorize(Resource::Steps, Action::Read, handlers::steps::get_step)))
        .route("/tasks/{uuid}/workflow_steps/{step_uuid}",
            patch(authorize(Resource::Steps, Action::Resolve, handlers::steps::resolve_step_manually)))
        .route("/tasks/{uuid}/workflow_steps/{step_uuid}/audit",
            get(authorize(Resource::Steps, Action::Read, handlers::steps::get_step_audit)))

        // Templates API
        .route("/templates", get(authorize(Resource::Templates, Action::List, handlers::templates::list_templates)))
        .route("/templates/{namespace}/{name}/{version}",
            get(authorize(Resource::Templates, Action::Read, handlers::templates::get_template)))

        // Analytics API
        .route("/analytics/performance",
            get(authorize(Resource::System, Action::AnalyticsRead, handlers::analytics::get_performance_metrics)))
        .route("/analytics/bottlenecks",
            get(authorize(Resource::System, Action::AnalyticsRead, handlers::analytics::get_bottlenecks)))

        // DLQ API
        .route("/dlq", get(authorize(Resource::Dlq, Action::List, handlers::dlq::list_dlq_entries)))
        .route("/dlq/task/{task_uuid}", get(authorize(Resource::Dlq, Action::Read, handlers::dlq::get_dlq_entry)))
        .route("/dlq/entry/{dlq_entry_uuid}",
            patch(authorize(Resource::Dlq, Action::Update, handlers::dlq::update_dlq_investigation)))
        .route("/dlq/stats", get(authorize(Resource::Dlq, Action::Stats, handlers::dlq::get_dlq_stats)))
        .route("/dlq/investigation-queue",
            get(authorize(Resource::Dlq, Action::Read, handlers::dlq::get_investigation_queue)))
        .route("/dlq/staleness",
            get(authorize(Resource::Dlq, Action::Read, handlers::dlq::get_staleness_monitoring)))
}

// Health routes remain unwrapped (public)
pub fn health_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(handlers::health::basic_health))
        // ... unchanged - public routes don't need authorize()
}

// Config route gets authorization
pub fn config_routes() -> Router<AppState> {
    Router::new()
        .route("/config", get(authorize(Resource::System, Action::ConfigRead, handlers::config::get_config)))
}
```

### Worker `routes.rs` (After)

```rust
pub fn template_routes() -> Router<Arc<WorkerWebState>> {
    Router::new()
        .route("/v1/templates",
            get(authorize(Resource::Worker, Action::List, handlers::templates::list_templates)))
        .route("/v1/templates/{namespace}/{name}/{version}",
            get(authorize(Resource::Worker, Action::Read, handlers::templates::get_template)))
        .route("/v1/templates/{namespace}/{name}/{version}/validate",
            post(authorize(Resource::Templates, Action::Validate, handlers::templates::validate_template)))
}

pub fn config_routes() -> Router<Arc<WorkerWebState>> {
    Router::new()
        .route("/config", get(authorize(Resource::Worker, Action::ConfigRead, handlers::config::get_config)))
}
```

---

## No Middleware Changes Needed

The `authorize()` wrapper handles permission checking internally. The existing auth middleware continues to:
1. Validate JWT/API key
2. Inject `SecurityContext` into request extensions

The wrapper then:
1. Extracts `SecurityContext` from extensions
2. Checks `has_permission()` for the required permission
3. Either proceeds to handler or returns 403

---

## Resource → Permission Mapping

The `ResourceAction::to_permission()` method maps resource+action to existing `Permission` enum:

| Resource | Action | Permission |
|----------|--------|------------|
| Tasks | Create | `tasks:create` |
| Tasks | Read | `tasks:read` |
| Tasks | List | `tasks:list` |
| Tasks | Cancel | `tasks:cancel` |
| Steps | Read | `steps:read` |
| Steps | List | `steps:read` |
| Steps | Resolve | `steps:resolve` |
| Templates | Read | `templates:read` |
| Templates | List | `templates:read` |
| Templates | Validate | `templates:validate` |
| Dlq | Read | `dlq:read` |
| Dlq | List | `dlq:read` |
| Dlq | Update | `dlq:update` |
| Dlq | Stats | `dlq:stats` |
| System | ConfigRead | `system:config_read` |
| System | AnalyticsRead | `system:analytics_read` |
| System | HandlersRead | `system:handlers_read` |
| Worker | Read | `worker:templates_read` |
| Worker | List | `worker:templates_read` |
| Worker | ConfigRead | `worker:config_read` |
| Health | * | None (public) |
| Metrics | * | None (public) |
| Docs | * | None (public) |

---

## Verification Approach

### Compile-Time Safety

The `authorize()` wrapper is explicit - if a route doesn't have it, it's unprotected. This is visible in code review.

### Test Coverage

```rust
// In tasker-orchestration/src/web/routes.rs tests
#[cfg(test)]
mod tests {
    #[test]
    fn test_protected_routes_return_403_without_permission() {
        // Build test app, call protected route without permission
        // Assert 403 response
    }

    #[test]
    fn test_protected_routes_succeed_with_permission() {
        // Build test app, call protected route with correct permission
        // Assert success response
    }

    #[test]
    fn test_public_routes_succeed_without_auth() {
        // Call health/metrics endpoints without auth
        // Assert success
    }
}
```

---

## Migration: Removing Handler Checks

After wrappers are deployed and tested:

1. **Phase 1**: Keep both wrapper and `require_permission()` (defense in depth during rollout)
2. **Phase 2**: Remove `require_permission()` calls from handlers
3. **Phase 3**: Remove `SecurityContext` extractor from handlers that don't need subject identity

**Before (current):**
```rust
pub async fn create_task(
    State(state): State<AppState>,
    security: SecurityContext,
    Json(request): Json<TaskRequest>,
) -> ApiResult<Json<TaskCreationResponse>> {
    require_permission(&security, Permission::TasksCreate)?;
    // business logic
}
```

**After (TAS-176):**
```rust
pub async fn create_task(
    State(state): State<AppState>,
    Json(request): Json<TaskRequest>,  // No SecurityContext needed
) -> ApiResult<Json<TaskCreationResponse>> {
    // Permission already checked by authorize() wrapper
    // business logic
}
```

---

## gRPC Reuse

The same `Resource` and `Action` enums work for Tonic gRPC:

```rust
// In gRPC service implementation
impl TaskService for TaskServiceImpl {
    async fn create_task(&self, request: Request<CreateTaskRequest>) -> Result<Response<Task>, Status> {
        // Extract metadata/context
        let resource_action = ResourceAction::new(Resource::Tasks, Action::Create);

        // Check permission (same logic as REST)
        if let Some(required) = resource_action.required_permission() {
            let ctx = extract_security_context(&request)?;
            if !ctx.has_permission(&required) {
                return Err(Status::permission_denied("Missing tasks:create"));
            }
        }

        // Business logic
    }
}
```

Or with a Tonic interceptor that does the same check.

---

## Verification Steps

1. **Unit tests**: `ResourceAction::to_permission()` mapping correctness
2. **Integration tests**: Full request flow with `authorize()` wrapper
3. **Manual verification**:
   ```bash
   # Health (public) - should succeed without auth
   curl http://localhost:8080/health

   # API without auth - should 401
   curl http://localhost:8080/v1/tasks

   # API with auth but wrong permission - should 403
   curl -H "Authorization: Bearer $TOKEN_NO_PERMS" http://localhost:8080/v1/tasks

   # API with correct permission - should succeed
   curl -H "Authorization: Bearer $TOKEN_WITH_TASKS_LIST" http://localhost:8080/v1/tasks
   ```

```bash
# After implementation
cargo make test  # All existing tests pass
```
