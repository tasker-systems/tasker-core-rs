# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**tasker-core-rs** is a high-performance Rust implementation of workflow orchestration, designed to complement the existing Ruby on Rails **Tasker** engine at `/Users/petetaylor/projects/tasker-systems/tasker-engine/`.

**Architecture**: Delegation-based pattern where Rust handles orchestration, state management, and performance-critical operations, while Ruby/Rails handles business logic execution through FFI integration.

## Current Status (July 2025)

### ✅ PRODUCTION READY: TCP Command Architecture Complete
- **Achievement**: Full Ruby-Rust TCP integration with zero ZeroMQ dependencies
- **Test Results**: 12/12 integration tests passing in 5 seconds (vs 75+ seconds previously)  
- **Performance**: Sub-millisecond command processing, no connection pool timeouts
- **Architecture**: Generic transport system supporting TCP, Unix sockets, and future protocols

### ✅ RECENT BREAKTHROUGH: supported_tasks FFI Parameter Fixed
- **Problem Solved**: Ruby `supported_tasks` parameter was showing as null in RegisterWorker commands
- **Root Cause**: Missing parameter extraction in Rust FFI binding layer
- **Solution**: Added complete TaskHandlerInfo conversion from Ruby to Rust with proper type handling
- **Result**: Database-backed task handler registration now fully functional

### 🎯 Current Focus: Task Initialization Handler Logic
**Issue**: Tasks are created with `step_count=0` and `workflow_steps=[]` - task templates not being processed into workflow steps
**Next Steps**: Fix TaskInitializer to properly create workflow steps from registered task handler configurations

## Architecture Overview

### Core Components
- **Command System**: TCP-based command routing with async handlers
- **Worker Management**: Registration, heartbeats, health monitoring with database persistence
- **Task Orchestration**: State machines, dependency resolution, batch execution
- **FFI Integration**: Handle-based architecture with persistent Arc<> references eliminating global lookups

### Key Technical Patterns
- **Handle-Based FFI**: `Ruby → OrchestrationManager → Handle → Persistent Resources → Database`
- **Singleton CommandClient**: Process-wide TCP client with auto-reconnection
- **Database-First Registry**: Task handlers registered with complete YAML configurations
- **Generic Transport**: Protocol-agnostic executor supporting multiple transport types

## Development Guidelines

### Code Quality Standards
- **No Placeholder Code**: All implementations must be complete, no TODOs in production paths
- **Test-Driven**: Use failing integration tests to expose and fix system issues
- **Type Safety**: Full compile-time verification with proper error handling
- **SQLx Integration**: Database tests use `#[sqlx::test]` with automatic isolation

### Current Working Branch
- **Branch**: `jcoletaylor/tas-14-m2-ruby-integration-testing-completion`
- **Test Focus**: Integration test `spec/handlers/integration/order_fulfillment_integration_spec.rb`

## Key File Locations

### Core Implementation
- **Command System**: `src/execution/command.rs`, `src/execution/command_router.rs`
- **TCP Infrastructure**: `src/execution/generic_executor.rs`, `src/execution/transport.rs`
- **Task Orchestration**: `src/orchestration/task_initializer.rs`, `src/orchestration/workflow_coordinator.rs`
- **FFI Integration**: `src/ffi/shared/` (shared components), `bindings/ruby/ext/tasker_core/src/`
- **Ruby Wrappers**: `bindings/ruby/lib/tasker_core/`

### Configuration
- **Database**: `db/structure.sql`, `migrations/`
- **Config**: `config/tasker-config-development.yaml`
- **Testing**: `bindings/ruby/spec/handlers/integration/`

## Development Commands

### Rust Core
```bash
cargo build                         # Build project  
cargo test                          # Run tests
cargo clippy                        # Lint code
cargo fmt                           # Format code
```

### Ruby Extension
```bash
cd bindings/ruby
bundle exec rake compile            # Compile Ruby extension
bundle exec rspec                   # Run Ruby tests
```

### Integration Testing
```bash
cd bindings/ruby
bundle exec rspec spec/handlers/integration/order_fulfillment_integration_spec.rb:156 --format documentation
```

## Recent Achievements

### ✅ TCP Command Architecture (July 2025)
- **ZeroMQ Elimination**: Complete removal of ZeroMQ dependencies
- **Generic Transport**: Protocol-agnostic executor with TCP, Unix socket support
- **Worker Management**: Full lifecycle with registration, heartbeats, health monitoring
- **Type Safety**: Dry-struct responses with enum validation
- **Performance**: 100x improvement - millisecond operations vs infinite hangs

### ✅ Handle-Based FFI Architecture  
- **Zero Global Lookups**: Persistent Arc<> references throughout system
- **Connection Pool Sharing**: Single database pool shared across all operations
- **Singleton Pattern**: Process-wide CommandClient with auto-reconnection
- **Production Performance**: 1-6ms database operations, no pool timeouts

### ✅ Database-First Task Registry
- **supported_tasks Parameter**: Complete Ruby→Rust FFI serialization working
- **Task Handler Configurations**: Full YAML configs with step templates registered
- **Database Persistence**: Workers associated with specific task capabilities
- **Type Conversion**: Proper Ruby hash → Rust TaskHandlerInfo conversion

## Current Priorities

1. **Fix Task Initialization**: Make TaskInitializer create workflow steps from task templates
2. **Complete Integration Tests**: Get order fulfillment workflow executing end-to-end  
3. **Batch Execution**: Implement ExecuteBatch command handling for step processing
4. **Error Handling**: Ensure all command handlers return responses (prevent timeouts)

## Related Projects

- **tasker-engine/**: Production-ready Rails engine for workflow orchestration
- **tasker-blog/**: GitBook documentation with real-world engineering stories

## MCP Server Integration

This project uses Model Context Protocol (MCP) servers for enhanced development:
- **PostgreSQL MCP**: Database operations and performance analysis
- **GitHub MCP**: Repository operations and CI/CD integration  
- **Rust Documentation MCP**: Real-time Rust best practices and API guidance

**Configuration**: All MCP servers configured in `.mcp.json`