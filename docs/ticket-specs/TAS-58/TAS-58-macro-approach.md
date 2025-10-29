# TAS-58: Macro-Based Debug Implementation Guide

## Overview

Instead of manually implementing Debug for 20+ types with PgPool or QueryBuilder fields, we've created declarative macros that generate the implementations automatically.

## Macros Available

Located in `tasker-shared/src/macros.rs`:

### 1. `debug_with_pgpool!`
For types containing a PgPool field (and possibly other Debug-implementing fields).

**Usage:**
```rust
pub struct MyService {
    pool: PgPool,
    name: String,
    count: usize,
}

crate::debug_with_pgpool!(MyService { pool: PgPool, name, count });
```

### 2. `debug_with_query_builder!`
For types containing a QueryBuilder field (and possibly other fields).

**Usage:**
```rust
pub struct MyScope {
    query: QueryBuilder<'static, Postgres>,
    enabled: bool,
}

crate::debug_with_query_builder!(MyScope { query: QueryBuilder, enabled });
```

### 3. `debug_with_db_types!`
For complex types with multiple database-related fields.

**Usage:**
```rust
pub struct MyComplexService {
    pool: PgPool,
    builder: QueryBuilder<'static, Postgres>,
    name: String,
}

crate::debug_with_db_types!(MyComplexService {
    pool => "PgPool",
    builder => "QueryBuilder";
    name
});
```

## Examples Implemented

### ✅ SqlFunctionExecutor (PgPool)
**File:** `tasker-shared/src/database/sql_functions.rs:93`

```rust
#[derive(Clone)]
pub struct SqlFunctionExecutor {
    pool: PgPool,
}

crate::debug_with_pgpool!(SqlFunctionExecutor { pool: PgPool });
```

**Output when debugging:** `SqlFunctionExecutor { pool: "PgPool" }`

### ✅ TaskScope (QueryBuilder + multiple fields)
**File:** `tasker-shared/src/scopes/task.rs:12`

```rust
pub struct TaskScope {
    query: QueryBuilder<'static, Postgres>,
    has_current_transitions_join: bool,
    has_named_tasks_join: bool,
    has_namespaces_join: bool,
    has_workflow_steps_join: bool,
    has_workflow_step_transitions_join: bool,
    has_conditions: bool,
}

crate::debug_with_query_builder!(TaskScope {
    query: QueryBuilder,
    has_current_transitions_join,
    has_named_tasks_join,
    has_namespaces_join,
    has_workflow_steps_join,
    has_workflow_step_transitions_join,
    has_conditions
});
```

**Output:** `TaskScope { query: "QueryBuilder", has_current_transitions_join: true, ... }`

## Remaining Types to Update

### PgPool Types (~15 remaining)

**tasker-shared:**
1. `FunctionRegistry` (`database/sql_functions.rs:685`) - Has SqlFunctionExecutor field
2. `TaskHandlerRegistry` (`registry/task_handler_registry.rs:108`)
3. `StepTransitiveDependenciesQuery` (`models/orchestration/step_transitive_dependencies.rs:121`)
4. `TransitionActions` (`state_machine/actions.rs:32`)
5. `StatePersistence` (`state_machine/persistence.rs:57`)
6. `TransitionPersistence` (`state_machine/persistence.rs:163`)
7. `StepStateMachine` (`state_machine/step_state_machine.rs:20`) - Also has Arc<EventPublisher>
8. `TaskStateMachine` (`state_machine/task_state_machine.rs:20`) - Also has Arc<EventPublisher>

**Apply pattern:**
```rust
crate::debug_with_pgpool!(TypeName { pool: PgPool, other_field1, other_field2 });
```

### QueryBuilder Types (~7 remaining)

**tasker-shared scopes:**
1. `WorkflowStepEdgeScope` (`scopes/edges.rs:12`)
2. `NamedTaskScope` (`scopes/named_task.rs:11`)
3. `TaskNamespaceScope` (`scopes/named_task.rs:135`)
4. `TaskTransitionScope` (`scopes/transitions.rs:13`)
5. `WorkflowStepTransitionScope` (`scopes/transitions.rs:140`)
6. `WorkflowStepScope` (`scopes/workflow_step.rs:13`)

**Apply pattern:**
```rust
crate::debug_with_query_builder!(TypeName { query: QueryBuilder, field1, field2 });
```

### Combined Types (PgPool + Arc/Other)

For types like `StepStateMachine` and `TaskStateMachine` that have both PgPool and Arc fields:

```rust
pub struct StepStateMachine {
    pool: PgPool,
    event_publisher: Arc<EventPublisher>,
}

// Option 1: Use debug_with_db_types
crate::debug_with_db_types!(StepStateMachine {
    pool => "PgPool",
    event_publisher => "EventPublisher";  // Arc<T> shows as T
});

// Option 2: Manual impl (if more complex)
impl std::fmt::Debug for StepStateMachine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StepStateMachine")
            .field("pool", &"PgPool")
            .field("event_publisher", &"EventPublisher")
            .finish()
    }
}
```

## Benefits of Macro Approach

### Before (Manual Implementation)
```rust
// Must be written for EVERY type with PgPool
impl std::fmt::Debug for SqlFunctionExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqlFunctionExecutor")
            .field("pool", &"PgPool")
            .finish()
    }
}

// Repeated 20+ times with slight variations!
```

### After (Macro)
```rust
// One line per type!
crate::debug_with_pgpool!(SqlFunctionExecutor { pool: PgPool });
```

**Advantages:**
- ✅ **Less code:** 1 line vs 7+ lines per type
- ✅ **Consistency:** All implementations follow the same pattern
- ✅ **Maintainability:** Change macro once, affects all types
- ✅ **Type safety:** Compile-time verification of field names
- ✅ **Less error-prone:** No manual typing of field names

## Implementation Checklist

- [x] Create macros in `tasker-shared/src/macros.rs`
- [x] Export macros module in `tasker-shared/src/lib.rs`
- [x] Test with `SqlFunctionExecutor` (simple PgPool case)
- [x] Test with `TaskScope` (QueryBuilder with multiple fields)
- [ ] Apply to remaining 13 PgPool types
- [ ] Apply to remaining 6 QueryBuilder types
- [ ] Verify zero Debug warnings with clippy

## Next Steps

1. Apply `debug_with_pgpool!` macro to all PgPool types listed above
2. Apply `debug_with_query_builder!` macro to all QueryBuilder scopes
3. Run `cargo clippy --all-features` to verify remaining warnings drop from 93 → ~71
   - Remaining warnings will be for complex types (Arc, atomics, channels)
4. Document any edge cases that need manual implementation

## Testing

Verify macros work:
```bash
export DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test"
cargo check --package tasker-shared --all-features
cargo clippy --package tasker-shared --all-features 2>&1 | grep "missing_debug"
```

Expected: No warnings for types using the macros.
