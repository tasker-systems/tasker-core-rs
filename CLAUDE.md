# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**tasker-core-rs** is a high-performance Rust implementation of workflow orchestration, designed to complement the existing Ruby on Rails **Tasker** engine at `/Users/petetaylor/projects/tasker-systems/tasker-engine/`.

**Architecture**: Delegation-based pattern where Rust handles orchestration, state management, and performance-critical operations, while Ruby/Rails handles business logic execution through FFI integration.

## Current Status (July 2025)

### ✅ RECENT BREAKTHROUGH: Ruby CommandListener Architecture Fixed  
- **Problem Solved**: Ruby CommandListener was using pure Ruby TCPSocket instead of delegating to Rust FFI
- **Root Cause**: Architecture mismatch between CommandClient (Rust delegation) and CommandListener (Ruby sockets)
- **Solution**: Refactored CommandListener to follow same delegation pattern as CommandClient
- **Result**: Fixed format mismatches and ExecuteBatch routing issues

### ✅ SINGLETON ARCHITECTURE: Process-Wide Component Management
- **CommandClient Singleton**: Process-wide TCP client with auto-reconnection in OrchestrationManager
- **CommandListener Singleton**: Process-wide command listener with lifecycle management
- **WorkerManager Singleton**: Process-wide worker manager with configuration consistency
- **Thread Safety**: All singletons use mutex-protected access and configuration matching

### ✅ CONFIGURATION REQUIREMENTS: No More Silent Failures
- **Fail-Fast Configuration**: Removed all hardcoded defaults (8080, etc) and require proper config
- **Clear Error Messages**: Missing command_backplane configuration throws descriptive errors
- **Test Configuration**: Embedded server uses configured ports from tasker-config-test.yaml
- **Result**: Configuration issues are caught immediately instead of failing silently

### 🎯 CURRENT ISSUE: Command Handler Timeouts (40 seconds)
**Problem**: TryTaskIfReady and InitializeTask commands timeout while RegisterWorker commands succeed
**Evidence**: Integration tests show 2/6 failures (improved from 6/6) but specific commands hang
**Analysis**: Worker registration works, suggesting TCP architecture is functional but specific handlers may be missing
**Next Steps**: Investigate command handler registration for TryTaskIfReady and InitializeTask in Rust TCP executor

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

## Session Context: Current Working Session (August 1, 2025)

### 🎯 SESSION OBJECTIVE: Complete Ruby-Rust Command Integration
**Starting Point**: Integration tests showing command routing issues and simulation in RubyCommandBridge
**User Insight**: Need to eliminate simulation and properly connect Ruby handlers to Rust command flow
**Architecture Challenge**: Magnus thread-safety constraints preventing direct Ruby callback passing

### ✅ MAJOR BREAKTHROUGH: RubyCommandBridge Architecture Complete
**Problem Solved**: RubyCommandBridge was using simulation instead of proper Ruby handler integration
**Root Cause**: Magnus `Value` cannot be safely passed across threads, requiring architectural redesign
**Solution**: Fire-and-forget pattern with immediate acknowledgment and separate Ruby processing flow

### 🔧 SESSION ACCOMPLISHMENTS
1. **Eliminated Simulation**: Removed all "simulating Ruby handler execution" code from RubyCommandBridge
2. **Solved Magnus Thread-Safety**: Designed architecture that avoids passing Ruby Values across threads
3. **Ruby CommandListener Cleanup**: Removed duplicate handler registration, simplified to Rust-only registration
4. **Fire-and-Forget Pattern**: RubyCommandBridge returns immediate acknowledgment, Ruby processes asynchronously
5. **Architecture Validation**: Integration tests running successfully with proper command routing

### 🎯 CURRENT ARCHITECTURE STATUS
- **✅ Working**: Orchestrator (8555) → Worker CommandListener (8556) → RubyCommandBridge → Ruby handlers
- **✅ Working**: Worker registration, ExecuteBatch command routing, immediate acknowledgment responses
- **✅ Working**: Ruby CommandListener delegates to Rust FFI, no more pure Ruby TCP sockets
- **✅ Working**: Process-wide singleton CommandClient and CommandListener management

### 📁 KEY FILES MODIFIED THIS SESSION
- `src/execution/command_handlers/ruby_command_bridge.rs` - Removed simulation, implemented proper acknowledgment
- `bindings/ruby/ext/tasker_core/src/command_listener.rs` - Handled Magnus thread-safety constraints
- `bindings/ruby/lib/tasker_core/execution/command_listener.rb` - Removed duplicate registration and helper methods
- Integration tests now pass command routing phase successfully

### 🎯 REMAINING WORK (In Priority Order)
1. **ReportPartialResult Flow**: Implement Ruby → Rust result reporting from BatchExecutionHandler
2. **ReportBatchCompletion Flow**: Complete batch completion signaling from Ruby back to orchestrator
3. **BatchExecutionHandler Integration**: Ensure Ruby handler actually processes commands after acknowledgment
4. **ZeroMQ Migration**: Replace remaining ZeroMQ usage in batch_step_execution_orchestrator.rb with TCP commands

## Current Priorities

1. **ReportPartialResult Flow**: Implement Ruby → Rust result reporting from BatchExecutionHandler back to orchestrator
2. **ReportBatchCompletion Flow**: Complete batch completion signaling from Ruby workers back to orchestrator  
3. **BatchExecutionHandler Integration**: Verify Ruby handler actually processes ExecuteBatch commands after RubyCommandBridge acknowledgment
4. **ZeroMQ Migration**: Replace remaining ZeroMQ usage in batch_step_execution_orchestrator.rb with TCP commands

## Testing Status

### Integration Test Results
- **File**: `spec/handlers/integration/order_fulfillment_integration_spec.rb:156`
- **Status**: Command routing phase successful, RubyCommandBridge working properly
- **✅ Working Commands**: RegisterWorker, ExecuteBatch command routing, embedded server startup
- **✅ Architecture**: Orchestrator (8555) → Worker CommandListener (8556) → RubyCommandBridge flow validated
- **Configuration**: Uses tasker-config-test.yaml with proper port assignment (8555/8556)
- **Next Phase**: Verify Ruby BatchExecutionHandler processes commands after acknowledgment

## Related Projects

- **tasker-engine/**: Production-ready Rails engine for workflow orchestration
- **tasker-blog/**: GitBook documentation with real-world engineering stories

## MCP Server Integration

This project uses Model Context Protocol (MCP) servers for enhanced development:
- **PostgreSQL MCP**: Database operations and performance analysis
- **GitHub MCP**: Repository operations and CI/CD integration  
- **Rust Documentation MCP**: Real-time Rust best practices and API guidance

**Configuration**: All MCP servers configured in `.mcp.json`