# Permissions

Permission-based access control using a `resource:action` vocabulary with wildcard support.

---

## Permission Vocabulary

17 permissions organized by resource:

### Tasks

| Permission | Description | Endpoints |
|------------|-------------|-----------|
| `tasks:create` | Create new tasks | `POST /v1/tasks` |
| `tasks:read` | Read task details | `GET /v1/tasks/{uuid}` |
| `tasks:list` | List tasks | `GET /v1/tasks` |
| `tasks:cancel` | Cancel running tasks | `DELETE /v1/tasks/{uuid}` |
| `tasks:context_read` | Read task context data | `GET /v1/tasks/{uuid}/context` |

### Steps

| Permission | Description | Endpoints |
|------------|-------------|-----------|
| `steps:read` | Read workflow step details | `GET /v1/tasks/{uuid}/workflow_steps`, `GET /v1/tasks/{uuid}/workflow_steps/{step_uuid}`, `GET /v1/tasks/{uuid}/workflow_steps/{step_uuid}/audit` |
| `steps:resolve` | Manually resolve steps | `PATCH /v1/tasks/{uuid}/workflow_steps/{step_uuid}` |

### Dead Letter Queue

| Permission | Description | Endpoints |
|------------|-------------|-----------|
| `dlq:read` | Read DLQ entries | `GET /v1/dlq`, `GET /v1/dlq/task/{task_uuid}`, `GET /v1/dlq/investigation-queue`, `GET /v1/dlq/staleness` |
| `dlq:update` | Update DLQ investigations | `PATCH /v1/dlq/entry/{dlq_entry_uuid}` |
| `dlq:stats` | View DLQ statistics | `GET /v1/dlq/stats` |

### Templates

| Permission | Description | Endpoints |
|------------|-------------|-----------|
| `templates:read` | Read task templates | Orchestration: `GET /v1/templates`, `GET /v1/templates/{namespace}/{name}/{version}` |
| `templates:validate` | Validate templates | Worker: `POST /v1/templates/{namespace}/{name}/{version}/validate` |

### System (Orchestration)

| Permission | Description | Endpoints |
|------------|-------------|-----------|
| `system:config_read` | Read system configuration | `GET /config` |
| `system:handlers_read` | Read handler registry | `GET /v1/handlers`, `GET /v1/handlers/{namespace}`, `GET /v1/handlers/{namespace}/{name}` |
| `system:analytics_read` | Read analytics data | `GET /v1/analytics/performance`, `GET /v1/analytics/bottlenecks` |

### Worker

| Permission | Description | Endpoints |
|------------|-------------|-----------|
| `worker:config_read` | Read worker configuration | Worker: `GET /config` |
| `worker:templates_read` | Read worker templates | Worker: `GET /v1/templates`, `GET /v1/templates/{namespace}/{name}/{version}` |

---

## Wildcards

Resource-level wildcards allow broad access within a resource domain:

| Pattern | Matches |
|---------|---------|
| `tasks:*` | All task permissions |
| `steps:*` | All step permissions |
| `dlq:*` | All DLQ permissions |
| `templates:*` | All template permissions |
| `system:*` | All system permissions |
| `worker:*` | All worker permissions |

**Note:** Global wildcards (`*`) are NOT supported. Use explicit resource wildcards for broad access (e.g., `tasks:*`, `system:*`). This follows AWS IAM-style resource-level granularity.

Wildcard matching is implemented in `permission_matches()`:
- `resource:*` → matches if required permission's resource component equals the prefix
- Exact string → matches if strings are identical

---

## Role Patterns

Common permission sets for different service roles:

### Read-Only Operator

```json
["tasks:read", "tasks:list", "steps:read", "dlq:read", "dlq:stats"]
```

Suitable for dashboards, monitoring services, and read-only admin UIs.

### Task Submitter

```json
["tasks:create", "tasks:read", "tasks:list"]
```

Services that submit work to Tasker and track their submissions.

### Ops Admin

```json
["tasks:*", "steps:*", "dlq:*", "system:*"]
```

Full operational access including step resolution, DLQ investigation, and system observability.

### Worker Service

```json
["worker:config_read", "worker:templates_read"]
```

Worker processes that need to read their configuration and available templates.

### Full Access (Admin)

```json
["tasks:*", "steps:*", "dlq:*", "templates:*", "system:*", "worker:*"]
```

Full access to all resources via resource wildcards. Use sparingly.

---

## Strict Validation

When `strict_validation = true` (default), tokens containing permission strings not in the vocabulary are rejected with 401:

```
Unknown permissions: custom:action, tasks:delete
```

Set `strict_validation = false` if your identity provider includes additional scopes that are not part of Tasker's vocabulary. Use `log_unknown_permissions = true` to still log unrecognized permissions for monitoring.

---

## Permission Check Implementation

### TAS-176: Resource-Based Authorization

Permissions are enforced declaratively at the route level using `authorize()` wrappers. This ensures authorization happens **before** body deserialization:

```rust
// In routes.rs
use tasker_shared::web::authorize;
use tasker_shared::types::resources::{Resource, Action};

Router::new()
    .route("/tasks", post(authorize(Resource::Tasks, Action::Create, create_task)))
    .route("/tasks", get(authorize(Resource::Tasks, Action::List, list_tasks)))
    .route("/tasks/{uuid}", get(authorize(Resource::Tasks, Action::Read, get_task)))
```

The `authorize()` wrapper:
1. Extracts `SecurityContext` from request extensions (set by auth middleware)
2. If resource is public (Health/Metrics/Docs) → proceeds to handler
3. If auth disabled (`AuthMethod::Disabled`) → proceeds to handler
4. Checks `has_permission(required)` → if yes, proceeds; if no, returns 403

### Resource → Permission Mapping

The `ResourceAction` type maps resource+action combinations to permissions:

| Resource | Action | Permission |
|----------|--------|------------|
| Tasks | Create | `tasks:create` |
| Tasks | Read | `tasks:read` |
| Tasks | List | `tasks:list` |
| Tasks | Cancel | `tasks:cancel` |
| Tasks | ContextRead | `tasks:context_read` |
| Steps | Read/List | `steps:read` |
| Steps | Resolve | `steps:resolve` |
| Dlq | Read/List | `dlq:read` |
| Dlq | Update | `dlq:update` |
| Dlq | Stats | `dlq:stats` |
| Templates | Read/List | `templates:read` |
| Templates | Validate | `templates:validate` |
| System | ConfigRead | `system:config_read` |
| System | HandlersRead | `system:handlers_read` |
| System | AnalyticsRead | `system:analytics_read` |
| Worker | ConfigRead | `worker:config_read` |
| Worker | Read/List | `worker:templates_read` |

### Public Resources

These resources don't require authentication:
- `Resource::Health` - Health check endpoints
- `Resource::Metrics` - Prometheus metrics
- `Resource::Docs` - OpenAPI/Swagger documentation

### Legacy Handler-Level Check (Still Available)

For cases where you need permission checks inside handler logic:

```rust
use tasker_shared::services::require_permission;
use tasker_shared::types::Permission;

fn my_handler(ctx: SecurityContext) -> Result<(), ApiError> {
    require_permission(&ctx, Permission::TasksCreate)?;
    // ... handler logic
}
```

Source: `tasker-shared/src/web/authorize.rs`, `tasker-shared/src/types/resources.rs`

---

## OpenAPI Documentation

### Permission Extensions (TAS-176)

Each protected endpoint in the OpenAPI spec includes an `x-required-permission` extension that documents the exact permission required:

```json
{
  "paths": {
    "/v1/tasks": {
      "post": {
        "security": [
          { "bearer_auth": [] },
          { "api_key_auth": [] }
        ],
        "x-required-permission": "tasks:create",
        ...
      }
    }
  }
}
```

### Why Extensions Instead of OAuth2 Scopes?

OpenAPI 3.x only formally supports scopes for OAuth2 and OpenID Connect security schemes—not for HTTP Bearer or API Key authentication. Since Tasker uses JWT Bearer tokens with JWKS validation (not OAuth2 flows), we use vendor extensions (`x-required-permission`) to document permissions in a standards-compliant way.

This approach:
- Is OpenAPI compliant (tools ignore unknown `x-` fields gracefully)
- Doesn't misrepresent our authentication mechanism
- Is machine-readable for SDK generators and tooling
- Is visible in generated documentation

### Viewing Permissions in Swagger UI

Each operation's description includes a **Required Permission** line:

```
**Required Permission:** `tasks:create`
```

This provides human-readable permission information directly in the Swagger UI.

### Programmatic Access

To extract permission requirements from the OpenAPI spec:

```python
import json

spec = json.load(open("orchestration-openapi.json"))
for path, methods in spec["paths"].items():
    for method, operation in methods.items():
        if "x-required-permission" in operation:
            print(f"{method.upper()} {path}: {operation['x-required-permission']}")
```

---

## CLI: List Permissions

```bash
cargo run --bin tasker-cli -- auth show-permissions
```

Outputs all 17 permissions with their resource grouping.
