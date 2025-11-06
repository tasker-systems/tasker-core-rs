# Protected Routes Configuration - Ergonomic Syntax

## Problem: Hard-to-Maintain Route Configuration

The original approach embedded route patterns in TOML section names as quoted strings:

```toml
[orchestration.web.auth.protected_routes."GET /v1/analytics/bottlenecks"]
auth_type = "bearer"
required = false

[orchestration.web.auth.protected_routes."PATCH /v1/tasks/{task_uuid}/workflow_steps/{step_uuid}"]
auth_type = "bearer"
required = true

[orchestration.web.auth.protected_routes."DELETE /v1/tasks/{task_uuid}"]
auth_type = "bearer"
required = true
```

**Issues:**
- HTTP method and path concatenated in section name
- Quoted strings in brackets are awkward to read/edit
- Difficult to visually scan for specific routes
- Hard to grep/search for specific methods or paths
- No separation between route pattern and auth config

## Solution: TOML Array of Tables

Using TOML's array of tables syntax for a more ergonomic declaration:

```toml
[[orchestration.web.auth.protected_routes]]
method = "GET"
path = "/v1/analytics/bottlenecks"
auth_type = "bearer"
required = false

[[orchestration.web.auth.protected_routes]]
method = "PATCH"
path = "/v1/tasks/{task_uuid}/workflow_steps/{step_uuid}"
auth_type = "bearer"
required = true

[[orchestration.web.auth.protected_routes]]
method = "DELETE"
path = "/v1/tasks/{task_uuid}"
auth_type = "bearer"
required = true
```

**Benefits:**
- Clear separation of HTTP method and path
- Standard TOML array syntax (no awkward quoted section names)
- Easy to scan, search, and maintain
- Method and path are separate fields (easy to filter/grep)
- Each route is a clear, self-contained entry

## Implementation: Load-Time Transformation

The configuration uses a **Vec at load time** for ergonomic TOML, with **HashMap conversion for runtime** lookups:

### TOML-Friendly Type (Load Time)
```rust
pub struct AuthConfig {
    // ... other fields ...

    /// Route-specific authentication configuration
    /// Uses TOML array of tables for ergonomic route declarations.
    /// At load time, converted to HashMap for efficient runtime lookups.
    #[serde(default)]
    pub protected_routes: Vec<ProtectedRouteConfig>,
}

pub struct ProtectedRouteConfig {
    /// HTTP method (GET, POST, PUT, DELETE, PATCH, etc.)
    pub method: String,

    /// Route path pattern (supports parameters like /v1/tasks/{task_uuid})
    pub path: String,

    /// Type of authentication required ("bearer", "api_key")
    pub auth_type: String,

    /// Whether authentication is required for this route
    pub required: bool,
}
```

### Runtime Conversion (HashMap for Fast Lookups)
```rust
impl AuthConfig {
    /// Convert protected routes list to HashMap for efficient runtime lookups
    pub fn routes_map(&self) -> HashMap<String, RouteAuthConfig> {
        self.protected_routes
            .iter()
            .map(|route| {
                let key = format!("{} {}", route.method, route.path);
                let config = RouteAuthConfig {
                    auth_type: route.auth_type.clone(),
                    required: route.required,
                };
                (key, config)
            })
            .collect()
    }
}
```

### Usage Pattern

```rust
// Load configuration (Vec from TOML)
let config = TaskerConfig::load("config.toml")?;

// Convert to HashMap for runtime lookups (once, at startup)
let auth = &config.orchestration.web.auth;
let routes_map = auth.routes_map();

// Fast runtime lookups
if let Some(route_config) = routes_map.get("DELETE /v1/tasks/{task_uuid}") {
    if route_config.required {
        // Enforce authentication
    }
}
```

## Migration Notes

**No Breaking Changes**: This is purely a load-time transformation.

**Backward Compatibility**: The original `config/web.rs` types remain unchanged. The conversion happens in `tasker_v2_proposal.rs` using `routes_map()`.

**Zero Runtime Cost**: Conversion happens once at startup. Runtime lookups use efficient HashMap.

## Example: Adding a New Protected Route

### Old Way (Awkward)
```toml
[orchestration.web.auth.protected_routes."POST /v1/admin/bulk-operations"]
auth_type = "bearer"
required = true
```

### New Way (Ergonomic)
```toml
[[orchestration.web.auth.protected_routes]]
method = "POST"
path = "/v1/admin/bulk-operations"
auth_type = "bearer"
required = true
```

**Improved maintainability:**
- Easy to copy/paste and modify
- Clear field structure
- Searchable with standard tools: `grep "method = \"POST\""`
- IDE-friendly with better syntax highlighting

## Testing

Comprehensive test coverage verifies:
- TOML deserialization with array of tables syntax
- Vec-based configuration loads correctly
- `routes_map()` conversion produces correct HashMap
- All route configurations are preserved
- Validation passes for all fields

See: `tasker-shared/src/config/tasker/tasker_v2_proposal.rs::test_deserialize_complete_test_toml()`
