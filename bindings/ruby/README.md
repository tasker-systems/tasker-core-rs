# Tasker Core Ruby Bindings

Ruby FFI bindings for the high-performance Tasker Core Rust orchestration engine.

## Status: Foundation Complete ✅

The foundational structure has been successfully implemented and tested:

- ✅ **Monorepo Integration**: Successfully integrated into the main tasker-core-rs repository
- ✅ **Cargo Workspace**: Properly configured as a workspace member
- ✅ **Magnus FFI Setup**: Basic Ruby module initialization working
- ✅ **Rust Compilation**: All Rust code compiles successfully
- ✅ **Build System**: CI/CD workflows configured for cross-platform testing

## Current Implementation

This is a **foundational version** that establishes the structure:

```rust
// Ruby module available:
TaskerCore::RUST_VERSION    # "0.1.0"
TaskerCore::STATUS          # "foundation"
TaskerCore::FEATURES        # "module_init"

// Error classes defined:
TaskerCore::Error
TaskerCore::OrchestrationError  
TaskerCore::DatabaseError
TaskerCore::FFIError
```

## Development Commands

```bash
# Install dependencies
bundle install

# Compile the Rust extension (requires Ruby dev environment)
rake compile

# Run tests
rake spec

# Full development setup
rake setup
```

## Next Phase: Full Implementation

The next development phase will implement:

1. **Complete WorkflowCoordinator API**
2. **Ruby ↔ Rust type conversions**
3. **Framework adapter for Rails integration**
4. **Performance benchmarks and validation**

## Architecture

This gem follows the **delegation-based architecture**:

```
Rails Engine ↔ tasker-core-rb (FFI) ↔ tasker-core-rs (Performance Core)
```

- **Rails**: Business logic and step execution
- **Rust**: High-performance orchestration and dependency resolution
- **Ruby Bindings**: Safe FFI bridge between the two

## Performance Targets

- **10-100x faster** dependency resolution vs PostgreSQL functions
- **<1ms FFI overhead** per orchestration call
- **>10k events/sec** cross-language event processing

## Requirements

- **Ruby**: 3.0+ with development headers
- **Rust**: 1.70+ with magnus dependencies
- **PostgreSQL**: 12+ for database operations

## Contributing

This is part of the larger tasker-systems monorepo. See the main project documentation for development guidelines and contribution instructions.

---

🦀 **Built with Rust + Magnus for maximum performance and safety**