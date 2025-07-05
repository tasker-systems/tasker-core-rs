# Test Organization Pattern - tasker-core-rs

## Philosophy: Tests Belong in `tests/` Directory

This project follows a strict test organization pattern that separates all non-doctest tests from production source code. This approach provides significant benefits for maintainability, discoverability, and CI performance.

## Core Principle

**All tests except doctests must be in the `tests/` directory.**

- ✅ **Doctests**: Remain in source files for documentation purposes
- ✅ **Unit Tests**: Move to `tests/` directory with proper module structure  
- ✅ **Integration Tests**: Move to `tests/` directory with SQLx native testing
- ❌ **`#[cfg(test)]` modules**: Not allowed in `src/` files

## Directory Structure

```
tests/
├── lib.rs                    # Main test module coordinator
├── config.rs                 # Configuration tests
├── database/                 # Database-specific tests
│   ├── mod.rs
│   └── sql_functions.rs
├── models/                   # Model tests with SQLx isolation
│   ├── mod.rs
│   ├── insights/             # Analytics and performance models
│   │   ├── mod.rs
│   │   ├── analytics_metrics.rs
│   │   ├── slowest_steps.rs
│   │   ├── slowest_tasks.rs
│   │   └── system_health_counts.rs
│   ├── annotation_type.rs
│   ├── dependent_system.rs
│   ├── dependent_system_object_map.rs
│   ├── named_step.rs
│   ├── named_task.rs
│   ├── named_tasks_named_step.rs
│   ├── step_dag_relationship.rs
│   ├── step_readiness_status.rs
│   ├── task.rs
│   ├── task_annotation.rs
│   ├── task_execution_context.rs
│   ├── task_namespace.rs
│   ├── task_transition.rs
│   ├── workflow_step.rs
│   └── workflow_step_transition.rs
├── query_builder/            # Query construction tests
│   ├── mod.rs
│   ├── builder.rs
│   ├── conditions.rs
│   ├── joins.rs
│   ├── pagination.rs
│   └── scopes.rs
└── state_machine/            # State management tests
    ├── mod.rs
    ├── actions.rs
    ├── errors.rs
    ├── events.rs
    ├── guards.rs
    ├── persistence.rs
    ├── states.rs
    ├── step_state_machine.rs
    └── task_state_machine.rs
```

## Benefits

### 1. **Enhanced Discoverability**
- Tests are easy to find without searching through source files
- Logical organization mirrors source structure
- Clear separation between production and test code

### 2. **Faster Development**
- No need to scroll through test code when working on production features
- Cleaner source files focus on business logic
- Reduced cognitive load when reading source code

### 3. **Superior CI Performance**
- Parallel test execution with SQLx database isolation
- Each test gets its own clean database instance
- No test interference or race conditions

### 4. **Maintainable Test Architecture**
- Consistent import patterns: `use tasker_core::module::*;`
- Standardized test structure across all modules
- Easy to add new tests following established patterns

### 5. **Production Code Quality**
- Zero test pollution in source files
- Smaller compilation units for production builds
- Clear API boundaries between modules

## Testing Patterns

### SQLx Native Testing for Database Tests
```rust
#[sqlx::test]
async fn test_model_crud(pool: PgPool) -> sqlx::Result<()> {
    // Each test gets its own isolated database
    let model = Model::create(&pool, data).await?;
    assert_eq!(model.field, expected);
    Ok(())
}
```

### Standard Unit Tests for Logic
```rust
#[test]
fn test_business_logic() {
    let result = calculate_something(input);
    assert_eq!(result, expected);
}
```

### Import Pattern
```rust
// In tests/models/example.rs
use tasker_core::models::example::{Example, NewExample};
use sqlx::PgPool;
```

## Migration History

This pattern was established through a comprehensive migration completed in July 2025:

- **28 test modules** migrated from `src/` to `tests/`
- **149 total tests** successfully reorganized
- **Zero `#[cfg(test)]` modules** remaining in source code
- **Perfect test isolation** achieved with SQLx native testing

## Enforcement

This pattern is enforced through:

1. **Code Review**: All new tests must follow this structure
2. **Documentation**: Clear examples in this guide
3. **CI Checks**: Future tooling may verify zero `#[cfg(test)]` in `src/`
4. **Team Agreement**: Established as project standard

## Exceptions

The only acceptable tests in source files are:

- **Doctests**: For API documentation and examples
- **Private helper function tests**: Only when testing requires access to private fields/methods

## Future Considerations

- Consider adding automated checks to prevent `#[cfg(test)]` in `src/`
- Explore test discovery tooling for enhanced navigation
- Maintain this pattern as the project scales

## Conclusion

This test organization pattern significantly improves code quality, developer experience, and CI performance. It should be maintained as a core architectural principle of the tasker-core-rs project.