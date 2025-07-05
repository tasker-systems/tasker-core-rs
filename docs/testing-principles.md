# Testing Principles - tasker-core-rs

## Core Testing Philosophy

This document establishes the testing principles for tasker-core-rs that all developers should follow.

## 🎯 **Primary Principle**

> **All tests except doctests must be in the `tests/` directory**

## Test Categories

### ✅ **Doctests** - Stay in `src/`
- **Purpose**: Document and test public API examples
- **Location**: Embedded in `src/` files within `///` comments
- **Usage**: Demonstrate correct usage patterns for developers
- **Example**:
```rust
/// Creates a new task with the given parameters
/// 
/// # Example
/// ```rust
/// use tasker_core::models::task::{Task, NewTask};
/// 
/// let new_task = NewTask {
///     named_task_id: 1,
///     context: Some(serde_json::json!({"order_id": 123})),
///     // ... other fields
/// };
/// ```
pub async fn create(pool: &PgPool, new_task: NewTask) -> Result<Task, sqlx::Error> {
    // Implementation
}
```

### ❌→✅ **Unit/Integration Tests** - Move to `tests/`
- **Purpose**: Test implementation logic and behavior
- **Current Location**: `#[cfg(test)]` modules in `src/` files ❌
- **Target Location**: Dedicated files in `tests/` directory ✅
- **Benefits**: Better isolation, faster builds, parallel execution

## Directory Structure

```
tests/
├── models/           # Model behavior tests
│   ├── insights/     # Insights model tests
│   ├── task.rs       # Task model tests
│   └── ...
├── state_machine/    # State machine logic tests
├── query_builder/    # Query builder tests  
├── database/         # Database function tests
└── common/           # Shared test utilities
```

## Migration Status

### ✅ **Completed**
- 4 model tests migrated to `tests/models/`
- Established SQLx native testing patterns
- Created comprehensive migration plan

### ❌ **Remaining** 
- **27 `#[cfg(test)]` modules** in `src/` need migration
- See `docs/test-migration-analysis.md` for detailed plan

## Developer Guidelines

### For New Tests
1. **Always create tests in `tests/` directory**
2. **Use SQLx native testing** for database tests: `#[sqlx::test]`
3. **Use standard testing** for pure logic: `#[test]`
4. **Follow import pattern**: `use tasker_core::module::*;`

### For Existing Code
1. **Do not add `#[cfg(test)]` modules** to `src/` files
2. **Migrate existing tests** when touching related code
3. **Prefer enhancing `tests/` files** over creating `src/` tests

### Test File Naming
- Mirror the `src/` structure in `tests/`
- Use same filename: `src/models/task.rs` → `tests/models/task.rs`
- Group related tests in single files

## Quality Standards

### Every Test Must
- [ ] Execute in isolation (no shared state)
- [ ] Pass in parallel execution
- [ ] Have descriptive test names
- [ ] Clean up automatically (SQLx handles this)

### Database Tests Must
- [ ] Use `#[sqlx::test]` attribute
- [ ] Accept `pool: PgPool` parameter
- [ ] Return `sqlx::Result<()>`
- [ ] Use `?` operator instead of `.expect()`

## Example Migration

### Before (❌ in src/)
```rust
// src/models/task.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DatabaseConnection;
    
    #[tokio::test]
    async fn test_task_crud() {
        let db = DatabaseConnection::new().await.expect("DB");
        let pool = db.pool();
        // test logic
        db.close().await;
    }
}
```

### After (✅ in tests/)
```rust
// tests/models/task.rs
use sqlx::PgPool;
use tasker_core::models::task::{Task, NewTask};

#[sqlx::test]
async fn test_task_crud(pool: PgPool) -> sqlx::Result<()> {
    // test logic - automatic cleanup!
    Ok(())
}
```

## Benefits

1. **🚀 Faster Builds**: Tests not compiled in release builds
2. **🔒 Better Isolation**: Each test gets fresh database
3. **⚡ Parallel Execution**: Tests run safely in parallel
4. **🎯 Clear Organization**: Easy to find and maintain tests
5. **✅ CI Stability**: Eliminates database conflicts
6. **📚 Better Documentation**: Clear separation of concerns

## References

- **Detailed Migration Plan**: `docs/test-migration-analysis.md`
- **Project Status**: See "Testing Architecture" section in `CLAUDE.md`
- **SQLx Testing Guide**: https://docs.rs/sqlx/latest/sqlx/attr.test.html

---

**Remember**: This is an early-stage architecture decision that will save significant time and complexity as the project grows. Following these principles ensures a maintainable, scalable testing strategy.