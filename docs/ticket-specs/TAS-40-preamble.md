# TAS-40 Preamble: Workspace Architecture Evolution

## Overview

The next phase of Tasker development involves a fundamental architectural evolution from a monolithic Rust codebase to a multi-workspace ecosystem that cleanly separates orchestration concerns from worker execution concerns. This document outlines the strategic direction for TAS-40, TAS-41, and TAS-42.

## Current State Challenges

### Monolithic Architecture Issues
- **Single codebase** handling both orchestration and prototype worker functionality
- **Binding languages** (Ruby) managing infrastructure concerns (threading, DB pools, queue management)
- **Tight coupling** between orchestration logic and step execution infrastructure
- **Inconsistent patterns** across different binding language implementations
- **Scalability limitations** due to shared infrastructure responsibilities

### Ruby Binding Complexity
The current Ruby bindings in `workers/ruby/` handle too many infrastructure concerns:
- Database connection pooling and management
- Queue worker thread coordination
- Message parsing and claiming logic
- State transition management
- Concurrent Ruby threading for step processing

This creates maintenance burden and prevents clean separation of business logic from infrastructure.

## Strategic Vision: Multi-Workspace Ecosystem

### Target Architecture
```
tasker-systems/
├── tasker-orchestration/     # Orchestration system (renamed from tasker-core)
├── tasker-worker-foundation/ # Worker infrastructure foundation
├── tasker-worker-rust/       # Pure Rust worker implementation
├── tasker-shared/           # Shared models, config, messaging
└── bindings/
    ├── ruby/                # Simplified business-logic-only Ruby
    ├── python/              # Future Python worker support
    └── wasm/                # Future WASM worker support
```

### Separation of Concerns

#### Orchestration System (`tasker-orchestration/`)
**Responsibilities:**
- Task lifecycle management and coordination
- Step enqueueing and dependency resolution
- Task finalization and completion determination
- Health monitoring and auto-scaling for orchestration
- Web API for task submission and status
- Integration with external systems

**Key Components:**
- `OrchestrationCore` and `OrchestrationLoopCoordinator`
- Finalization claiming and race condition prevention
- Axum web API for task management
- Configuration management and resource validation

#### Worker Foundation (`tasker-worker-foundation/`)
**Responsibilities:**
- Worker infrastructure and lifecycle management
- Step claiming, processing coordination, result persistence
- FFI bridge to binding languages (Ruby, Python, WASM)
- Worker health monitoring and auto-scaling
- Resource management (DB pools, queue connections)
- Worker-specific web API (health checks, metrics)

**Key Components:**
- `WorkerCore` and `WorkerLoopCoordinator`
- FFI event bridge system
- Step processing workflow management
- Worker-specific configuration and resource limits

#### Shared Foundation (`tasker-shared/`)
**Responsibilities:**
- Common database models and schema
- Configuration system and TOML management
- Messaging abstractions (TAS-35 queue traits)
- Common types and data structures
- Shared utilities and error handling

#### Binding Languages (Simplified)
**Ruby Responsibilities (Post-TAS-42):**
- Pure step handler business logic implementation
- Domain-specific processing and validation
- External API integrations and business rules
- Event handling via dry-events pub-sub

**Removed from Ruby:**
- Database connection management
- Queue threading and coordination
- Message parsing and state transitions
- Infrastructure resource management

## Implementation Roadmap

### TAS-40: Worker Foundation (Weeks 1-4)
**Goal:** Create robust Rust worker infrastructure foundation

**Outcomes:**
- Complete worker system mirroring orchestration patterns
- Clean FFI boundary via dry-events
- Worker-specific health monitoring and auto-scaling
- Dedicated worker web API for K8s integration
- 70%+ reduction in Ruby infrastructure code

### TAS-41: Pure Rust Worker (Weeks 5-6)
**Goal:** Prove worker foundation with native Rust implementation

**Outcomes:**
- Standalone Rust worker using foundation
- Same workflow patterns as Ruby examples
- Complete test infrastructure matching existing patterns
- Docker deployability and production readiness
- Performance baseline for 1000+ steps/second

### TAS-42: Ruby Simplification (Weeks 7-8)
**Goal:** Strip Ruby to pure business logic using worker foundation

**Outcomes:**
- Ruby codebase reduced by 70%+
- Zero infrastructure code in Ruby
- Clean event-based integration
- Maintained compatibility with existing step handlers
- Migration path for existing deployments

## Workspace Structure Details

### Cargo Workspace Configuration
```toml
# Cargo.toml (workspace root)
[workspace]
members = [
    "tasker-orchestration",
    "tasker-worker-foundation",
    "tasker-worker-rust",
    "tasker-shared"
]

[workspace.dependencies]
# Shared dependencies across all workspace members
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
# ... other common dependencies
```

### Dependency Flow
```
tasker-orchestration → tasker-shared
tasker-worker-foundation → tasker-shared
tasker-worker-rust → tasker-worker-foundation → tasker-shared
```

### Configuration Organization
```
config/
├── orchestration/           # Orchestration-specific configs
│   ├── base/
│   └── environments/
├── worker/                  # Worker-specific configs
│   ├── base/
│   └── environments/
└── shared/                  # Shared configs (database, messaging)
    ├── base/
    └── environments/
```

## Migration Strategy

### Phase 1: Workspace Creation (Week 1)

#### Detailed Implementation Sequence

##### Step 1: Create Workspace Structure (Keep Everything Compiling)
```bash
# Create workspace directories without moving files
mkdir -p tasker-shared/src
mkdir -p tasker-orchestration/src
mkdir -p tasker-worker-foundation/src

# Create placeholder Cargo.toml files
touch tasker-shared/Cargo.toml
touch tasker-orchestration/Cargo.toml
touch tasker-worker-foundation/Cargo.toml
```

##### Step 2: Create Transitional Workspace Root
```toml
# Cargo.toml (root) - Temporary dual-crate setup
[workspace]
members = [
    ".",  # Current crate temporarily remains
    "tasker-shared",
    "tasker-orchestration",
    "tasker-worker-foundation"
]
resolver = "2"

[workspace.dependencies]
# Shared dependencies
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
sqlx = { version = "0.7", features = ["postgres", "runtime-tokio-rustls", "chrono", "uuid"] }
pgmq = "0.33.1"
tracing = "0.1"
thiserror = "1.0"
anyhow = "1.0"
uuid = { version = "1.0", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
```

##### Step 3: Create tasker-shared Incrementally

**3.1: Copy Core Infrastructure (No Dependencies)**
```bash
# Copy files that have no internal dependencies
cp src/error.rs tasker-shared/src/error.rs
cp src/constants.rs tasker-shared/src/constants.rs
cp src/validation.rs tasker-shared/src/validation.rs
cp src/sql_functions.rs tasker-shared/src/sql_functions.rs
```

Create initial `tasker-shared/Cargo.toml`:
```toml
[package]
name = "tasker-shared"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { workspace = true }
serde_json = "1.0"
uuid = { workspace = true }
thiserror = { workspace = true }
```

Create initial `tasker-shared/src/lib.rs`:
```rust
pub mod error;
pub mod constants;
pub mod validation;
pub mod sql_functions;
```

Verify: `cd tasker-shared && cargo build`

**3.2: Add Logging (Depends on error)**
```bash
cp src/logging.rs tasker-shared/src/logging.rs
```
Update `tasker-shared/Cargo.toml` to add tracing dependencies
Update `tasker-shared/src/lib.rs` to add `pub mod logging;`
Fix any import paths in logging.rs
Verify: `cd tasker-shared && cargo build`

**3.3: Add Database Module**
```bash
cp -r src/database tasker-shared/src/database
```
Update dependencies and imports
Verify compilation

**3.4: Add Configuration Module**
```bash
cp -r src/config tasker-shared/src/config
```
Update dependencies and imports
Verify compilation

**3.5: Add State Machine**
```bash
cp -r src/state_machine tasker-shared/src/state_machine
```
Update dependencies and imports
Verify compilation

**3.6: Add Registry**
```bash
cp -r src/registry tasker-shared/src/registry
```
Update dependencies and imports
Verify compilation

**3.7: Add Scopes, Events, Execution**
```bash
cp -r src/scopes tasker-shared/src/scopes
cp -r src/events tasker-shared/src/events
cp -r src/execution tasker-shared/src/execution
```
Update dependencies and imports
Verify compilation

**3.8: Add Models, Types, Messaging (Most Complex)**
```bash
cp -r src/models tasker-shared/src/models
cp -r src/types tasker-shared/src/types
cp -r src/messaging tasker-shared/src/messaging
```
Update dependencies and imports
Verify compilation

##### Step 4: Update Root Crate to Use tasker-shared

**4.1: Add tasker-shared as dependency**
```toml
# In root Cargo.toml [dependencies]
tasker-shared = { path = "tasker-shared" }
```

**4.2: Replace modules with re-exports**
```rust
// In src/lib.rs, progressively replace:
// pub mod error;
// with:
pub use tasker_shared::error;
```

**4.3: Delete original files after confirming re-export works**
```bash
# After confirming each module works via tasker-shared:
rm src/error.rs
rm src/constants.rs
# etc...
```

Verify after each deletion: `cargo build && cargo test`

##### Step 5: Create tasker-orchestration

**5.1: Move orchestration-specific code**
```bash
# Create orchestration structure
mkdir -p tasker-orchestration/src/orchestration
mkdir -p tasker-orchestration/src/web
mkdir -p tasker-orchestration/src/resilience

# Move orchestration modules
mv src/orchestration/* tasker-orchestration/src/orchestration/
mv src/web/* tasker-orchestration/src/web/
mv src/resilience/* tasker-orchestration/src/resilience/
```

**5.2: Create tasker-orchestration/Cargo.toml**
```toml
[package]
name = "tasker-orchestration"
version = "0.1.0"
edition = "2021"

[dependencies]
tasker-shared = { path = "../tasker-shared" }
tokio = { workspace = true }
axum = "0.7"
# ... other orchestration-specific deps
```

**5.3: Update imports in moved files**
- Change `crate::` imports to `tasker_shared::`
- Fix internal orchestration imports

##### Step 6: Move Tests

**6.1: Categorize tests**
```bash
# Shared functionality tests
mv tests/*_shared_test.rs tasker-shared/tests/

# Orchestration tests
mv tests/*_orchestration_test.rs tasker-orchestration/tests/
mv tests/web_*.rs tasker-orchestration/tests/

# Integration tests that span both
# Keep temporarily in root, will refactor later
```

**6.2: Update test imports**
- Fix all imports to use new crate structure
- Verify each test suite independently

##### Step 7: Update Binaries and Examples

**7.1: Move orchestration binaries**
```bash
mv src/bin/tasker-server.rs tasker-orchestration/src/bin/
```

**7.2: Update binary imports**
- Change to use `tasker_orchestration` and `tasker_shared`

##### Step 8: Final Cleanup

**8.1: Remove root src/ orchestration code**
```bash
# After confirming everything works via workspaces:
rm -rf src/orchestration
rm -rf src/web
rm -rf src/resilience
```

**8.2: Update root Cargo.toml**
```toml
# Remove "." from workspace members
[workspace]
members = [
    "tasker-shared",
    "tasker-orchestration",
    "tasker-worker-foundation"
]
```

**8.3: Convert root to virtual workspace**
- Remove [package] section from root Cargo.toml
- Remove src/ directory entirely
- Keep only workspace configuration

##### Step 9: Verify Complete Migration

**9.1: Build all workspaces**
```bash
cargo build --workspace
```

**9.2: Run all tests**
```bash
cargo test --workspace
```

**9.3: Check all examples**
```bash
cargo check --workspace --examples
```

**9.4: Verify binaries**
```bash
cargo build --workspace --bins
```

##### Step 10: Update CI and Documentation

**10.1: Update .github/workflows**
- Change build commands to use `--workspace`
- Update test commands

**10.2: Update README.md**
- Document new workspace structure
- Update build instructions

**10.3: Update sqlx offline data**
```bash
cd tasker-orchestration
cargo sqlx prepare
cd ../tasker-shared
cargo sqlx prepare
```

This incremental approach ensures:
1. **Everything compiles at each step** - No broken intermediate states
2. **Git history preserved** - Using `git mv` for moves
3. **Easy rollback** - Can revert at any step if issues arise
4. **Clear progress tracking** - Each substep is independently verifiable
5. **Minimal disruption** - Can pause/resume at any point

### Phase 2: Worker Foundation (Weeks 2-4)
1. **Implement core worker infrastructure** in `tasker-worker-foundation/`
2. **Create FFI bridge system** for binding language integration
3. **Build worker health monitoring** and auto-scaling
4. **Develop worker web API** for health checks and metrics
5. **Integration testing** with existing Ruby bindings

### Phase 3: Rust Worker (Weeks 5-6)
1. **Create `tasker-worker-rust/` workspace** using foundation
2. **Implement Rust step handlers** matching Ruby workflow patterns
3. **Build comprehensive test suite** replicating existing test coverage
4. **Docker integration** and deployment configuration
5. **Performance benchmarking** and optimization

### Phase 4: Ruby Simplification (Weeks 7-8)
1. **Remove infrastructure code** from Ruby bindings
2. **Implement event-based integration** with worker foundation
3. **Migrate existing step handlers** to simplified pattern
4. **Comprehensive testing** of simplified Ruby integration
5. **Documentation and migration guides**

## Benefits of This Architecture

### Operational Benefits
- **Independent scaling** of orchestration vs worker components
- **Language flexibility** for worker implementations
- **Resource isolation** between orchestration and execution
- **Simplified deployment** with clear service boundaries

### Development Benefits
- **Reduced complexity** in binding language implementations
- **Consistent infrastructure** across all supported languages
- **Better testability** with clear component boundaries
- **Faster iteration** on business logic without infrastructure concerns

### Performance Benefits
- **Optimized resource usage** with dedicated pools and limits
- **Reduced memory overhead** by eliminating duplicate infrastructure
- **Better concurrent execution** with purpose-built worker coordination
- **Scalable architecture** supporting 1000+ steps/second from day one

## Success Criteria

### Quantitative Metrics
- ✅ Ruby codebase reduced by 70%+ (infrastructure removal)
- ✅ Worker system handles 1000+ steps/second from initial release
- ✅ Zero race conditions in step claiming and processing
- ✅ Sub-5-second graceful shutdown for both orchestration and workers
- ✅ < 10% FFI overhead compared to native Ruby execution

### Qualitative Outcomes
- ✅ Clean separation between orchestration and worker concerns
- ✅ Simple, maintainable binding language implementations
- ✅ Production-ready Docker deployments for all components
- ✅ Comprehensive monitoring and observability
- ✅ Clear migration path for existing deployments

## Risk Mitigation

### Technical Risks
- **Workspace complexity:** Start with simple structure, evolve incrementally
- **FFI overhead:** Benchmark early, optimize critical paths
- **Configuration management:** Leverage existing TAS-34 patterns

### Migration Risks
- **Breaking changes:** Maintain compatibility during transition
- **Performance regression:** Comprehensive benchmarking at each phase
- **Operational complexity:** Clear documentation and deployment guides

This preamble sets the foundation for the detailed implementation specifications in TAS-40, TAS-41, and TAS-42, ensuring we have a clear strategic vision before diving into implementation details.
