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
| `templates:read` | Read task templates | Worker: `GET /templates/*` |
| `templates:validate` | Validate templates | Worker: `POST /templates/validate` |

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
| `worker:templates_read` | Read worker templates | Worker: `GET /templates/*` |

---

## Wildcards

| Pattern | Matches |
|---------|---------|
| `*` | All permissions (superuser) |
| `tasks:*` | All task permissions |
| `steps:*` | All step permissions |
| `dlq:*` | All DLQ permissions |
| `templates:*` | All template permissions |
| `system:*` | All system permissions |
| `worker:*` | All worker permissions |

Wildcard matching is implemented in `permission_matches()`:
- `*` → always true
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

### Superuser

```json
["*"]
```

Full access to all resources. Use sparingly.

---

## Strict Validation

When `strict_validation = true` (default), tokens containing permission strings not in the vocabulary are rejected with 401:

```
Unknown permissions: custom:action, tasks:delete
```

Set `strict_validation = false` if your identity provider includes additional scopes that are not part of Tasker's vocabulary. Use `log_unknown_permissions = true` to still log unrecognized permissions for monitoring.

---

## Permission Check Implementation

Permissions are enforced at the handler level (not middleware). Each handler calls:

```rust
require_permission(&security_context, Permission::TasksCreate)?;
```

This returns:
- `Ok(())` if `AuthMethod::Disabled` (auth off) or permission matches
- `Err(ApiError::authorization_error(...))` → HTTP 403 with message indicating the missing permission

Source: `tasker-orchestration/src/web/middleware/permission.rs`, `tasker-worker/src/web/middleware/auth.rs`

---

## CLI: List Permissions

```bash
cargo run --bin tasker-cli -- auth show-permissions
```

Outputs all 17 permissions with their resource grouping.
