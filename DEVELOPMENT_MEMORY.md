# Development Memory - Tasker Core Rust

This document serves as institutional memory for the Tasker Core Rust project, capturing key decisions, solutions, and patterns that should be preserved for future development.

## 🧠 **Key Architectural Decisions**

### **Database Concurrency Strategy**
**Decision**: Use PostgreSQL advisory locks instead of Rust-level mutexes for schema operations
**Rationale**: 
- Database-level synchronization survives process crashes
- Works across multiple connection pools
- No shared memory requirements between test processes
- Atomic operations at the database level

**Implementation**:
```rust
const LOCK_KEY: i64 = 1234567890123456;
let lock_acquired = sqlx::query_scalar::<_, bool>(
    "SELECT pg_try_advisory_lock($1)"
).bind(LOCK_KEY).fetch_one(pool).await?;
```

### **Migration Strategy - Hybrid Approach**
**Decision**: Different migration strategies for different environments
- **Testing**: Fresh schema rebuild with concurrency control
- **Development**: Incremental migrations with version tracking  
- **Production**: Traditional migration runner

**Rationale**: Tests need isolation, production needs data preservation

### **SQLx Usage Pattern**
**Decision**: Hybrid compile-time + runtime query validation
- **Static Queries**: Use `sqlx::query!()` for compile-time validation
- **Dynamic Schema**: Use `sqlx::query()` for migration-related operations
- **Cache Management**: Clear `.sqlx` when schema changes significantly

## 🔧 **Critical Problem Solutions**

### **1. Test Concurrency Race Conditions**
**Problem**: Multiple test threads rebuilding database schema simultaneously
**Symptoms**: 
- `pg_class_relname_nsp_index` constraint violations
- `relation "tasker_schema_migrations" does not exist` errors
- Inconsistent test results between sequential and parallel execution

**Root Cause**: No synchronization between threads performing schema operations
**Solution**: Database advisory locks with wait-and-verify pattern

**Pattern to Remember**:
```rust
// One thread rebuilds, others wait
if lock_acquired {
    rebuild_schema().await?;
    release_lock().await?;
} else {
    wait_for_schema_ready().await?;
}
```

### **2. PostgreSQL Sequence Conflicts**
**Problem**: Manual inserts with explicit IDs don't advance sequences
**Symptoms**: Tests failing with primary key constraint violations on ID=1
**Solution**: Sync sequences after seed data

**Pattern to Remember**:
```sql
INSERT INTO table (id, name) VALUES (1, 'default');
SELECT setval('table_id_seq', COALESCE(MAX(id), 1)) FROM table;
```

### **3. SQLx Type Mismatches**
**Problem**: SQL functions returning varchar(64) vs text causing type conflicts
**Symptoms**: Compilation errors about mismatched types in query macros
**Solution**: 
1. Clear SQLx cache (`rm -rf .sqlx`)
2. Update SQL functions to return consistent types
3. Use runtime queries for dynamic operations

## 🏗️ **Established Patterns**

### **Model Implementation Pattern**
Every Rails model migration follows this structure:
1. **Core Struct**: Main entity with all database fields
2. **New Struct**: For creation without generated fields
3. **CRUD Methods**: `create`, `find_by_id`, `update`, `delete`
4. **Rails Scopes**: All ActiveRecord scopes as associated functions
5. **Business Logic**: Instance methods for computed properties
6. **Comprehensive Tests**: With transaction rollback and unique test data

### **Test Structure Pattern**
```rust
#[tokio::test]
async fn test_model_crud() {
    let db = DatabaseConnection::new().await.expect("Failed to connect");
    let pool = db.pool();
    
    // Create test data with unique names
    let unique_name = format!("test_{}", timestamp_or_random());
    
    // Test operations...
    
    // No cleanup needed - transaction rollback handles it
    db.close().await;
}
```

### **Migration File Pattern**
- **Naming**: `YYYYMMDDHHMMSS_description.sql`
- **Content**: Idempotent operations with `IF NOT EXISTS`
- **Seed Data**: Always sync sequences after manual inserts
- **Constraints**: Add business logic constraints, avoid over-constraining

## 🚨 **Critical Don'ts**

### **Database Operations**
❌ **Never** use Rust-level mutexes for database schema operations
❌ **Never** assume sequences are synced after manual inserts
❌ **Never** create migrations that aren't idempotent
❌ **Never** use hardcoded IDs in test data without checking conflicts

### **SQLx Usage**  
❌ **Never** mix compile-time and runtime queries for the same dynamic schema
❌ **Never** ignore SQLx cache when schema changes significantly
❌ **Never** assume query compilation succeeds across schema rebuilds

### **Testing**
❌ **Never** rely on global test state or ordering
❌ **Never** create tests that depend on specific database content
❌ **Never** assume tests run sequentially unless explicitly forced

## 🎯 **Performance Optimizations**

### **Proven Strategies**
1. **Parallel Test Execution**: 54% faster than sequential with proper synchronization
2. **Transaction Rollback**: Faster than manual cleanup for test isolation
3. **Connection Pooling**: Reuse connections across tests in same thread
4. **Advisory Locks**: Faster than recreating databases per test

### **Measurement Results**
- **Sequential Tests**: 1.24s for 67 tests
- **Parallel Tests**: 0.56s for 67 tests
- **Success Rate**: 100% with concurrency controls
- **Memory Usage**: Minimal leaks due to Rust ownership

## 🔮 **Future Considerations**

### **Next Phase Priorities**
1. **Step Handler Foundation**: Core orchestration engine
2. **FFI Bindings**: Ruby, Python, Node.js integration
3. **Performance Optimization**: Cache strategies and query optimization
4. **Production Deployment**: Monitoring and observability

### **Architectural Evolution**
- **Event System**: Pub/sub for workflow lifecycle events
- **Caching Strategy**: Redis integration for high-performance operations
- **Horizontal Scaling**: Multi-instance coordination
- **Monitoring**: OpenTelemetry integration

## 📚 **Reference Documents**
- [TESTING_ARCHITECTURE.md](./TESTING_ARCHITECTURE.md) - Detailed testing infrastructure
- [CLAUDE.md](./CLAUDE.md) - Project overview and current status
- [migrations/](./migrations/) - Database schema evolution
- [src/models/](./src/models/) - Complete model implementations

## 🎓 **Lessons for Future Developers**

1. **Database-level synchronization** is more reliable than application-level for schema operations
2. **PostgreSQL advisory locks** are perfect for atomic initialization patterns
3. **SQLx cache management** requires careful consideration in dynamic environments
4. **Test isolation** through transactions is faster and more reliable than data cleanup
5. **Parallel execution** with proper synchronization provides significant performance benefits
6. **Business logic preservation** from Rails to Rust is achievable with careful pattern matching

This memory bank should be updated as new patterns emerge and new solutions are discovered.