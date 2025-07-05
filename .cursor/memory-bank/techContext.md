# Technical Context: Tasker Core Rust

## Technology Stack

### Core Technologies

#### Rust Ecosystem
- **Rust Edition**: 2021 with latest stable toolchain (1.88.0+)
- **Crate Type**: `cdylib` (for FFI) + `rlib` (for Rust consumers)
- **Target Platforms**: Linux, macOS, Windows (with primary focus on Linux/macOS)

#### Async Runtime
- **Tokio**: `1.0` with full features for async/await operations
- **async-trait**: `0.1` for trait object compatibility
- **futures**: `0.3` for future combinators and utilities

#### Database Layer
- **SQLx**: `0.8` with PostgreSQL, UUID, Chrono, and JSON support
- **PostgreSQL**: Primary database with type-safe compile-time verified queries
- **UUID**: `1.0` with v4 generation and serde support
- **Connection**: `postgresql://tasker:tasker@localhost/tasker_rust_development`

### Serialization & Data Handling

#### Core Serialization
- **serde**: `1.0` with derive macros for JSON/MessagePack serialization
- **serde_json**: `1.0` for JSON handling (Rails compatibility)
- **chrono**: `0.4` with serde support for timestamp handling

#### Time & Date
- **NaiveDateTime**: Used throughout for PostgreSQL timestamp compatibility
- **UTC**: Consistent timezone handling across the system

### Error Handling & Logging

#### Error Management
- **thiserror**: `2.0` for structured error types with derive macros
- **anyhow**: `1.0` for error context and chaining

#### Observability
- **tracing**: `0.1` for structured logging and instrumentation
- **tracing-subscriber**: `0.3` with env-filter and JSON features
- **OpenTelemetry**: `0.27` with OTLP export for distributed tracing

### Concurrency & State Management

#### Thread-Safe Collections
- **parking_lot**: `0.12` for high-performance mutexes and RwLocks
- **dashmap**: `6.0` for concurrent hash maps
- **crossbeam**: `0.8` for lock-free data structures and channels

#### State Machines
- **smlang**: `0.7` for compile-time state machine generation
- Custom state machine implementations for complex workflow logic

### Configuration Management
- **config**: `0.14` for hierarchical configuration loading
- Environment variable support with `.env` file loading
- YAML/TOML configuration file support

## FFI Integration

### Ruby Integration (Primary)
- **magnus**: `0.7` for Ruby bindings and Rails integration
- Ruby class wrapping with `#[magnus::wrap]`
- Memory-safe Ruby object handling
- Integration with Rails ActiveRecord patterns

### Python Integration
- **PyO3**: `0.22` with extension-module features
- Python class generation with `#[pyclass]`
- NumPy compatibility for data science workflows
- Async Python support for non-blocking operations

### WebAssembly Support
- **wasm-bindgen**: `0.2` for JavaScript/WebAssembly bindings
- **js-sys**: `0.3` for JavaScript API bindings
- **web-sys**: `0.3` for Web API access
- Future consideration for browser-based workflow visualization

## Development & Testing

### Testing Framework
- **tokio-test**: `0.4` for async test utilities and time manipulation
- **proptest**: `1.0` for property-based testing of DAG operations
- **mockall**: `0.12` for mock object generation and testing
- **insta**: `1.34` for snapshot testing of serialization formats
- **tempfile**: `3.0` for temporary file handling in tests

### Performance Testing
- **criterion**: `0.5` for statistical benchmarking
- **fastrand**: `2.0` for fast random number generation in tests
- Benchmarking infrastructure for 10-100x performance validation

### Database Testing
- **sqlx-test**: Transactional testing with automatic rollback
- Test database isolation and parallel test execution
- Property-based testing for complex DAG scenarios

## Build Configuration

### Release Profile
```toml
[profile.release]
opt-level = 3          # Maximum optimization
lto = true             # Link-time optimization
codegen-units = 1      # Single codegen unit for better optimization
panic = "abort"        # Smaller binary size, faster execution
```

### Development Profile
```toml
[profile.dev]
opt-level = 0          # Fast compilation
debug = true           # Full debug information
```

### Feature Flags
- `default = ["postgres"]` - PostgreSQL support enabled by default
- `ruby-ffi` - Enable Ruby FFI bindings (magnus)
- `python-ffi` - Enable Python FFI bindings (PyO3)
- `wasm-ffi` - Enable WebAssembly bindings
- `benchmarks` - Enable Criterion benchmarking

## Development Environment

### Required Tools
- **Rust**: Latest stable toolchain via rustup
- **PostgreSQL**: 12+ with development headers
- **SQLx CLI**: For database migrations and schema management
- **Ruby**: 3.0+ with magnus gem for FFI development
- **Python**: 3.8+ with PyO3 for Python bindings

### Database Setup
```bash
# Database configuration
createdb tasker_rust_development
psql tasker_rust_development < migrations/20250701023135_initial_schema.sql
sqlx migrate run
```

### Development Commands
```bash
# Standard Rust workflow
cargo build                    # Build the project
cargo test                     # Run test suite
cargo run                      # Execute main binary
cargo bench                    # Run performance benchmarks
cargo clippy                   # Lint checking
cargo fmt                      # Code formatting

# Database operations
sqlx migrate add <name>         # Create new migration
sqlx migrate run               # Apply migrations
sqlx prepare                   # Generate query metadata
```

## Project Structure

### Source Organization
```
src/
├── lib.rs              # Main library entry point
├── config.rs           # Configuration management
├── error.rs            # Error type definitions
├── models/             # Database entity models
│   ├── mod.rs          # Module exports
│   ├── task.rs         # Task model (381 lines)
│   ├── workflow_step.rs # WorkflowStep model (485 lines)
│   ├── named_task.rs   # NamedTask model (405 lines)
│   ├── named_step.rs   # NamedStep model (273 lines)
│   ├── workflow_step_edge.rs # Edge relationships (202 lines)
│   ├── task_namespace.rs # Namespace model (239 lines)
│   └── transitions.rs  # State transition auditing (231 lines)
├── orchestration/      # Core orchestration engine
│   ├── mod.rs          # Module exports
│   ├── coordinator.rs  # Main orchestration coordinator
│   ├── step_executor.rs # Step execution logic
│   ├── task_finalizer.rs # Task completion logic
│   ├── viable_step_discovery.rs # Dependency resolution
│   └── backoff_calculator.rs # Retry timing logic
├── state_machine/      # State management
├── events/            # Event system
├── registry/          # Registry systems
├── database/          # Database layer
├── ffi/              # FFI bindings
└── query_builder/    # Query building utilities
```

### Test Organization
```
tests/
├── common/            # Shared test utilities
│   ├── mod.rs         # Test module exports
│   ├── test_db.rs     # Database test setup
│   ├── builders.rs    # Test data builders
│   ├── mock_delegate.rs # Mock implementations
│   └── strategies.rs  # Property test strategies
├── database_test.rs   # Database integration tests
├── model_integration_tests.rs # Model integration tests
└── property_based_tests.rs # Property-based DAG tests
```

### Benchmark Organization
```
benches/
└── orchestration_benchmarks.rs # Performance benchmarks
```

## External Dependencies

### Rails Tasker Engine
- **Location**: `/Users/petetaylor/projects/tasker/`
- **Schema Reference**: `spec/dummy/db/structure.sql`
- **Models Reference**: `app/models/`
- **Core Logic**: `lib/tasker/`

### Database Schema
- **Primary Key Pattern**: `bigint` for main entities, `integer` for lookup tables
- **Timestamp Pattern**: `timestamp(6) without time zone` with `created_at`/`updated_at`
- **JSON Fields**: `jsonb` for structured data (context, tags, configuration)
- **Foreign Keys**: Proper referential integrity with cascading deletes

### Configuration Sources
1. **Environment Variables**: `DATABASE_URL`, `TASKER_*` prefixed variables
2. **Configuration Files**: YAML/TOML support for complex configuration
3. **Default Values**: Sensible defaults for development environment
4. **Runtime Configuration**: Dynamic configuration updates via API

## Performance Considerations

### Memory Management
- **Arc/Rc**: Reference counting for shared ownership
- **Box**: Heap allocation for large structures
- **Cow**: Copy-on-write for efficient string handling
- **Zero-copy**: Minimize allocations in hot paths

### Database Optimization
- **Connection Pooling**: 20 connection pool for concurrent operations
- **Prepared Statements**: SQLx compile-time query preparation
- **Batch Operations**: Bulk inserts/updates for efficiency
- **Index Strategy**: Proper indexing for dependency resolution queries

### Concurrency Strategy
- **Async/Await**: Non-blocking I/O throughout
- **Thread Pool**: Tokio's work-stealing thread pool
- **Lock-Free**: DashMap and crossbeam for high-contention scenarios
- **Backpressure**: Proper flow control for event processing
