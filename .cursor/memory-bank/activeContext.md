# Active Context: Tasker Core Rust

## Current Work Focus

### Branch: `making-models`
Currently working on **Phase 1: Foundation Layer** with focus on database model schema alignment.

### Immediate Priority: Schema Alignment
The database models exist but have significant mismatches with the PostgreSQL schema that need to be fixed before proceeding with orchestration logic.

## Recent Changes & Discoveries

### Architecture Evolution
- **Major Shift**: Moved from monolithic replacement to **step handler foundation architecture**
- **Control Flow**: Rust core IS the step handler base that frameworks subclass
- **Integration Pattern**: Framework code extends Rust step handler with `process()` and `process_results()` hooks
- **Universal Foundation**: Same orchestration core works across Rails, Python FastAPI, Node.js Express, etc.

### Corrected Control Flow Understanding
```
┌─────────────┐    ┌─────────────────────────────────────┐    ┌─────────────────┐
│   Queue     │───▶│           Rust Core                 │───▶│ Re-enqueue      │
│ (Framework) │    │ ┌─────────────────────────────────┐ │    │ (Framework)     │
└─────────────┘    │ │     Step Handler Foundation     │ │    └─────────────────┘
                   │ │  • handle() logic               │ │             ▲
                   │ │  • backoff calculations         │ │             │
                   │ │  • retry analysis               │ │             │
                   │ │  • step output processing       │ │             │
                   │ │  • task finalization            │ │             │
                   │ └─────────────────────────────────┘ │             │
                   │              │                      │             │
                   │              ▼                      │             │
                   │ ┌─────────────────────────────────┐ │             │
                   │ │   Framework Step Handler        │ │             │
                   │ │   (Rails/Python/Node subclass)  │ │             │
                   │ │  • process() - user logic       │ │             │
                   │ │  • process_results() - post     │ │─────────────┘
                   │ └─────────────────────────────────┘ │
                   └─────────────────────────────────────┘
```

### Schema Analysis Completed
Comprehensive analysis in `docs/SCHEMA_ANALYSIS.md` revealed critical mismatches:

#### Task Model Issues
- Missing fields: `complete`, `requested_at`, `initiator`, `source_system`, `reason`, `bypass_steps`, `tags`, `identity_hash`
- Extra fields: `state`, `most_recent_error_message`, `most_recent_error_backtrace`
- Field naming: Rust uses different names than SQL schema

#### WorkflowStep Model Issues
- Missing fields: `retryable`, `in_process`, `processed`, `processed_at`, `last_attempted_at`, `backoff_request_seconds`, `skippable`
- Naming differences: `context` vs `inputs`, `output` vs `results`, `retry_count` vs `attempts`

#### WorkflowStepEdge Model Issues
- Missing primary key: `id` field
- Missing edge metadata: `name` field

### Current Model Status
- ✅ **Task**: Schema aligned (381 lines) - matches PostgreSQL exactly
- ❌ **WorkflowStep**: Needs alignment (485 lines)
- ❌ **WorkflowStepEdge**: Missing fields (202 lines)
- ❌ **NamedTask**: Version type mismatch (405 lines)
- ❌ **NamedStep**: Incomplete implementation (273 lines)
- ✅ **TaskNamespace**: Properly implemented (239 lines)
- ✅ **Transitions**: State audit trail complete (231 lines)

## Next Steps (Immediate)

### 1. Fix WorkflowStepEdge Model (Highest Priority)
```rust
// Current (missing fields)
pub struct WorkflowStepEdge {
    pub from_step_id: i64,
    pub to_step_id: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// Needs to become
pub struct WorkflowStepEdge {
    pub id: i64,                    // PRIMARY KEY
    pub from_step_id: i64,
    pub to_step_id: i64,
    pub name: String,               // Edge name/label
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}
```

### 2. Fix WorkflowStep Model
Align field names and add missing boolean flags:
- Add `retryable`, `in_process`, `processed`, `skippable` boolean fields
- Rename `context` → `inputs`, `output` → `results`, `retry_count` → `attempts`
- Add `processed_at`, `last_attempted_at` timestamp fields
- Add `backoff_request_seconds` integer field

### 3. Fix NamedTask Version Field
Change `version: i32` to `version: String` to match PostgreSQL `character varying(16)`

### 4. Complete NamedStep Model
Add missing `dependent_system_id` field and proper foreign key relationships

## Active Decisions & Considerations

### Step Handler Foundation Strategy
- **Rust Core**: Implements complete step handler foundation with all lifecycle logic
- **Framework Subclasses**: Extend Rust base with `process()` and `process_results()` hooks
- **Queue Abstraction**: Rust decides re-enqueue, framework handles actual queuing via dependency injection
- **Universal Pattern**: Same core works across Rails, Python, Node.js with wrapper code

### Schema Alignment Strategy
- **Exact Match**: Rust models must match PostgreSQL schema exactly
- **No Computed Fields**: Remove computed fields like `state` that should be derived
- **Proper Types**: Use `NaiveDateTime` for PostgreSQL timestamps, `String` for varchar fields
- **Nullable Fields**: Proper `Option<T>` for nullable database columns

### FFI Integration Strategy
- **Step Handler Base**: Rust step handler exposed via FFI with subclass hooks
- **Framework Wrappers**: Rails/Python/Node classes extend Rust step handler
- **Queue Injection**: Framework-specific queue implementations injected into Rust core
- **Universal API**: Same step handler interface across all language bindings

### Testing Strategy During Alignment
- **Transactional Tests**: Use `#[sqlx::test]` for database test isolation
- **Schema Validation**: Test that Rust models serialize/deserialize correctly with database
- **Step Handler Testing**: Test subclass pattern with mock framework implementations
- **Property Testing**: Validate DAG operations during model changes

## Current Challenges

### 1. Step Handler Architecture Implementation
Need to design the FFI interface that allows frameworks to subclass the Rust step handler while maintaining performance and safety.

### 2. Queue Abstraction Design
Must create dependency injection pattern for queue systems that works across different frameworks and queue backends.

### 3. Schema Complexity
The PostgreSQL schema has evolved significantly from initial Rust models. Need careful analysis of each field to ensure proper mapping.

### 4. Universal Framework Support
Need to design step handler foundation that works seamlessly across Rails, Python FastAPI, Node.js Express with consistent behavior.

## Environment Status

### Git Status
- **Branch**: `making-models` (up to date with origin)
- **Modified Files**: `src/config.rs` (minor changes)
- **Untracked**: `.cursor/` directory (memory bank initialization)

### Database Status
- **Connection**: `postgresql://tasker:tasker@localhost/tasker_rust_development`
- **Migrations**: Applied through `20250701120000_align_with_rails_schema.sql`
- **Schema**: Matches Rails production schema exactly

### Development Environment
- **Rust**: 1.88.0+ stable toolchain
- **PostgreSQL**: Running locally with test database
- **SQLx**: CLI tools available for migrations
- **Testing**: Full test suite with property-based testing setup

## Coordination with Rails Engine

### Reference Implementation
- **Location**: `/Users/petetaylor/projects/tasker/`
- **Schema Source**: `spec/dummy/db/structure.sql`
- **Step Handler Reference**: `lib/tasker/step_handler.rb` and related files
- **Core Logic**: `lib/tasker/orchestration/`

### Integration Points
- **Database**: Shared PostgreSQL schema and tables
- **Step Handler Pattern**: Rails step handlers will subclass Rust foundation
- **Queue Systems**: Rails queue implementation injected into Rust core
- **Events**: Compatible event system (56+ lifecycle events)

## Success Metrics for Current Phase

### Model Alignment (Phase 1)
- [ ] All models match PostgreSQL schema exactly
- [ ] Full CRUD operations working for all models
- [ ] Comprehensive test coverage with transactional tests
- [ ] FFI serialization formats validated with snapshot tests
- [ ] Property-based tests for DAG operations passing

### Step Handler Foundation Design
- [ ] Rust step handler base class designed with FFI interface
- [ ] Subclass pattern working with `process()` and `process_results()` hooks
- [ ] Queue abstraction pattern designed for dependency injection
- [ ] Framework wrapper proof-of-concept working

### Performance Baseline
- [ ] Benchmarking infrastructure established
- [ ] Baseline measurements for step handler performance
- [ ] Memory usage profiling for long-running operations
- [ ] Concurrent operation safety validated

### Integration Readiness
- [ ] Ruby FFI interface for step handler subclassing working
- [ ] Queue injection pattern tested with Sidekiq
- [ ] Event system compatibility validated
- [ ] Configuration system Rails-compatible
