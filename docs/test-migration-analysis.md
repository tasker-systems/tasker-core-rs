# Test Migration Analysis - tasker-core-rs

## Overview

This document provides a comprehensive analysis of all tests in the tasker-core-rs codebase and establishes a migration plan to move all non-doctest tests to the `tests/` directory. This follows modern Rust best practices and aligns with our SQLx native testing architecture.

## Current Test Distribution

### Summary
- **27 #[cfg(test)] modules in src/** ❌ Needs migration
- **21 test files in tests/** ✅ Correctly located  
- **41 doctests in src/** ✅ Correctly located (should remain)

### Testing Philosophy
- **Doctests**: Remain in src/ files for documentation purposes ✅
- **Unit/Integration Tests**: All migrate to tests/ directory ❌→✅
- **Test Organization**: Mirror src/ structure in tests/ directory

## Detailed Migration Plan

### Phase 1: Model Tests (HIGH PRIORITY)
**Target**: Complete migration of all model tests to `tests/models/`

#### Already Migrated ✅
- `annotation_type.rs` ✅ 
- `dependent_system.rs` ✅
- `dependent_system_object_map.rs` ✅
- `task_transition.rs` ✅
- `step_dag_relationship.rs` ✅
- `step_readiness_status.rs` ✅
- `task_execution_context.rs` ✅

#### Needs Migration ❌
**Core Models:**
1. `tests/models/task.rs` ← `src/models/task.rs::#[cfg(test)]`
2. `tests/models/workflow_step.rs` ← `src/models/workflow_step.rs::#[cfg(test)]`
3. `tests/models/workflow_step_transition.rs` ← `src/models/workflow_step_transition.rs::#[cfg(test)]`
4. `tests/models/task_annotation.rs` ← `src/models/task_annotation.rs::#[cfg(test)]`
5. `tests/models/named_tasks_named_step.rs` ← `src/models/named_tasks_named_step.rs::#[cfg(test)]`
6. `tests/models/named_step.rs` ← `src/models/named_step.rs::#[cfg(test)]`
7. `tests/models/named_task.rs` ← `src/models/named_task.rs::#[cfg(test)]`
8. `tests/models/task_namespace.rs` ← `src/models/task_namespace.rs::#[cfg(test)]`

**Insights Models:**
9. `tests/models/insights/analytics_metrics.rs` ← `src/models/insights/analytics_metrics.rs::#[cfg(test)]`
10. `tests/models/insights/system_health_counts.rs` ← `src/models/insights/system_health_counts.rs::#[cfg(test)]`
11. `tests/models/insights/slowest_steps.rs` ← `src/models/insights/slowest_steps.rs::#[cfg(test)]`
12. `tests/models/insights/slowest_tasks.rs` ← `src/models/insights/slowest_tasks.rs::#[cfg(test)]`

### Phase 2: Database Tests (HIGH PRIORITY)
**Target**: Move database-related tests to `tests/database/`

1. `tests/database/sql_functions.rs` ← `src/database/sql_functions.rs::#[cfg(test)]`

### Phase 3: State Machine Tests (MEDIUM PRIORITY)  
**Target**: Move state machine tests to `tests/state_machine/`

1. `tests/state_machine/task_state_machine.rs` ← `src/state_machine/task_state_machine.rs::#[cfg(test)]`
2. `tests/state_machine/step_state_machine.rs` ← `src/state_machine/step_state_machine.rs::#[cfg(test)]`
3. `tests/state_machine/states.rs` ← `src/state_machine/states.rs::#[cfg(test)]`
4. `tests/state_machine/guards.rs` ← `src/state_machine/guards.rs::#[cfg(test)]`
5. `tests/state_machine/persistence.rs` ← `src/state_machine/persistence.rs::#[cfg(test)]`
6. `tests/state_machine/events.rs` ← `src/state_machine/events.rs::#[cfg(test)]`
7. `tests/state_machine/errors.rs` ← `src/state_machine/errors.rs::#[cfg(test)]`
8. `tests/state_machine/actions.rs` ← `src/state_machine/actions.rs::#[cfg(test)]`

### Phase 4: Query Builder Tests (MEDIUM PRIORITY)
**Target**: Move query builder tests to `tests/query_builder/`

1. `tests/query_builder/builder.rs` ← `src/query_builder/builder.rs::#[cfg(test)]`
2. `tests/query_builder/conditions.rs` ← `src/query_builder/conditions.rs::#[cfg(test)]`
3. `tests/query_builder/joins.rs` ← `src/query_builder/joins.rs::#[cfg(test)]`
4. `tests/query_builder/pagination.rs` ← `src/query_builder/pagination.rs::#[cfg(test)]`
5. `tests/query_builder/scopes.rs` ← `src/query_builder/scopes.rs::#[cfg(test)]`

### Phase 5: Core Library Tests (LOW PRIORITY)
**Target**: Move core library tests to appropriate locations

1. `tests/lib.rs` ← `src/lib.rs::#[cfg(test)]` (single test function)

## Migration Process

### For Each Test Module:

1. **Create test file** in appropriate tests/ subdirectory
2. **Copy test module content** from src/ #[cfg(test)] block  
3. **Update imports** to use `use tasker_core::module::*;` syntax
4. **Convert test types** to SQLx native testing where appropriate:
   - `#[tokio::test]` → `#[sqlx::test]` for database tests
   - Keep `#[test]` for pure unit tests
5. **Run tests** to verify functionality: `cargo test`
6. **Remove #[cfg(test)]** module from src/ file
7. **Verify no regression** with full test suite

### Import Pattern Example:
```rust
// In tests/models/task.rs
use sqlx::PgPool;
use tasker_core::models::task::{Task, NewTask};
use tasker_core::models::task_namespace::{TaskNamespace, NewTaskNamespace};

#[sqlx::test]
async fn test_task_crud(pool: PgPool) -> sqlx::Result<()> {
    // Test implementation
}
```

## Quality Assurance

### Before Each Migration:
- [ ] Identify all test functions in source #[cfg(test)] module
- [ ] Note any special dependencies or setup requirements
- [ ] Check for existing tests/ file to merge with

### After Each Migration:
- [ ] All tests pass: `cargo test`
- [ ] No compilation warnings
- [ ] Test coverage maintained (no lost functionality)
- [ ] Source file #[cfg(test)] module removed

### Final Validation:
- [ ] Zero #[cfg(test)] modules remain in src/
- [ ] All tests execute in parallel without conflicts
- [ ] Build time improved (no test compilation in release builds)
- [ ] CI pipeline success

## Benefits of Migration

1. **Improved Test Isolation**: SQLx native testing provides automatic database isolation
2. **Faster Builds**: Tests not compiled in production builds
3. **Better Organization**: Clear separation between implementation and tests
4. **Parallel Execution**: Tests can run safely in parallel
5. **CI Stability**: Eliminates database-related CI failures
6. **Developer Experience**: Easier to find and maintain tests

## Post-Migration Architecture

```
tests/
├── common/           # Shared test utilities ✅
├── database/         # Database and SQL function tests
├── models/           # Model tests (partially complete)
│   └── insights/     # Insights model tests  
├── query_builder/    # Query builder tests
├── state_machine/    # State machine tests
├── database_test.rs  # Integration tests ✅
└── property_based_tests.rs ✅

src/
├── models/           # Clean model implementations (no #[cfg(test)])
├── query_builder/    # Clean query builder (no #[cfg(test)])
├── state_machine/    # Clean state machine (no #[cfg(test)])
└── database/         # Clean database code (no #[cfg(test)])
```

## Timeline Estimate

- **Phase 1 (Models)**: 2-3 hours (12 model test files)
- **Phase 2 (Database)**: 30 minutes (1 database test file)  
- **Phase 3 (State Machine)**: 1-2 hours (8 state machine test files)
- **Phase 4 (Query Builder)**: 1 hour (5 query builder test files)
- **Phase 5 (Core)**: 15 minutes (1 lib test)

**Total Estimated Time**: 4-6 hours for complete migration

This migration will establish a clean, maintainable testing architecture that follows Rust best practices and supports the project's growth.