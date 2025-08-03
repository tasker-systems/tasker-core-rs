# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**tasker-core-rs** is a high-performance Rust implementation of workflow orchestration, designed to complement the existing Ruby on Rails **Tasker** engine at `/Users/petetaylor/projects/tasker-systems/tasker-engine/`.

**Architecture**: PostgreSQL message queue (pgmq) based system where Rust handles orchestration and step enqueueing, while Ruby workers autonomously process steps through queue polling - eliminating FFI coupling and coordination complexity.

## Current Status (August 3, 2025)

### 🎉 MAJOR ARCHITECTURAL PIVOT: TCP → pgmq Success!
- **Strategic Decision**: Pivoted from complex TCP command system to PostgreSQL message queue architecture
- **Problem Solved**: Eliminated imperative disguised as event-driven, central planning overhead, Rust<->Ruby thread issues
- **Solution**: Simple queue-based processing that returns to Rails Tasker philosophy
- **Result**: ✅ **Phases 1-4 Complete** - pgmq architecture fully implemented! ✅ **Phase 5.2 Largely Complete** - Individual step enqueueing with metadata flow!

### ✅ PHASE 5.2 COMPLETED: Individual Step Enqueueing with Metadata Flow (August 3, 2025)
- **Enhanced StepMessage** ✅: Complete execution context with (task, sequence, step) where sequence contains dependency results
- **Immediate Delete Pattern** ✅: Queue workers delete messages immediately, no retry logic duplication  
- **Dependency Chain Results** ✅: `StepExecutionContext.dependencies` provides convenient `sequence.get(step_name)` access
- **Ruby Type System** ✅: Complete dry-struct types matching Rust structures with wrapper classes
- **Handler Interface Fixed** ✅: Workers now call `handler.call(task, sequence, step)` and treat any return as success
- **Orchestration Metadata Integration** ✅: `OrchestrationResultProcessor` enhanced with `BackoffCalculator` integration
- **Intelligent Backoff Processing** ✅: HTTP headers, error context, and backoff hints flow to orchestration decisions

### 🏗️ PHASE 5.2 ARCHITECTURAL BENEFITS ACHIEVED
- **🚀 "Worker Executes, Orchestration Coordinates"**: Complete separation of concerns
- **🔄 Individual Step Processing**: No batch coordination complexity, fault isolation per step
- **📊 Metadata Flow**: Rich orchestration metadata from handlers to intelligent backoff decisions
- **📋 Enhanced Dependency Chain**: (task, sequence, step) pattern with full dependency results
- **🎯 Immediate Feedback**: Step results processed as they complete
- **🧹 Simplified Workers**: No retry state management, stateless debugging

## Architecture Overview

### Core Components (pgmq-based)
- **Message Queues**: PostgreSQL-backed queues using pgmq extension for reliability
- **Step Enqueueing**: Rust publishes step messages to namespace-specific queues
- **Autonomous Workers**: Ruby workers poll queues and execute step handlers independently
- **SQL Functions**: Status queries and analytics through database functions
- **Type System**: dry-struct validation for messages, results, and metadata

### Queue Design Pattern
```
fulfillment_queue    - All fulfillment namespace steps
inventory_queue      - All inventory namespace steps  
notifications_queue  - All notification namespace steps
```

**Message Structure**:
```json
{
  "step_id": 12345,
  "task_id": 67890, 
  "namespace": "fulfillment",
  "step_name": "validate_order",
  "step_payload": { /* step execution data */ },
  "metadata": {
    "enqueued_at": "2025-08-01T12:00:00Z",
    "retry_count": 0,
    "max_retries": 3
  }
}
```

### Key Technical Patterns
- **Queue-First Architecture**: All coordination through PostgreSQL message queues
- **Autonomous Processing**: Workers operate independently without registration
- **Database Integration**: Shared database access for Rust orchestration and Ruby execution
- **Type-Safe Messages**: Full validation using dry-struct/dry-types system
- **Fire-and-Forget**: Immediate message acknowledgment with async processing

## Development Guidelines

### Code Quality Standards
- **No Placeholder Code**: All implementations must be complete, no TODOs in production paths
- **Queue-Driven Design**: All inter-component communication through pgmq
- **Type Safety**: Full compile-time verification with dry-struct validation
- **Autonomous Components**: No central coordination, workers poll independently

### Current Working Branch
- **Branch**: `jcoletaylor/tas-14-m2-ruby-integration-testing-completion`
- **Focus**: Phase 5.2 completion - Individual step enqueueing with metadata flow (LARGELY COMPLETE)

## Key File Locations

### pgmq Architecture (Phase 1 Complete)
- **Rust Integration**: `src/messaging/pgmq_client.rs`, `src/messaging/message.rs`
- **Ruby Implementation**: `bindings/ruby/lib/tasker_core/messaging/pgmq_client.rb`
- **Queue Workers**: `bindings/ruby/lib/tasker_core/messaging/queue_worker.rb`
- **SQL Functions**: `bindings/ruby/lib/tasker_core/database/sql_functions.rb`
- **Type System**: `bindings/ruby/lib/tasker_core/types/step_message.rb`
- **Integration Tests**: `bindings/ruby/spec/integration/pgmq_architecture_spec.rb`

### Core Business Logic (Preserved)
- **Task Orchestration**: `src/orchestration/task_initializer.rs`, `src/orchestration/workflow_coordinator.rs`  
- **Database Models**: `src/models/` (unchanged)
- **Step Handlers**: `bindings/ruby/lib/tasker_core/step_handler/` (unchanged)
- **Ruby Business Logic**: `bindings/ruby/spec/handlers/examples/` (preserved)

### Configuration
- **Database**: `migrations/20250801000001_enable_pgmq_extension.sql`
- **Config**: `config/tasker-config-test.yaml`
- **Environment**: `.env` with `DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test`

## Development Commands

### Rust Core
```bash
cargo build                         # Build project  
cargo test                          # Run tests
cargo clippy                        # Lint code
cargo fmt                           # Format code
```

### Ruby Extension & pgmq Tests
```bash
cd bindings/ruby
bundle install                      # Install gems (including pg gem)
bundle exec rake compile            # Compile Ruby extension
DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test bundle exec rspec spec/integration/pgmq_architecture_spec.rb --format documentation
```

### Database Setup
```bash
cd /Users/petetaylor/projects/tasker-systems/tasker-core-rs
DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test cargo sqlx migrate run
```

## Current Status: Phase 1 Complete, Ready for Phase 2

### ✅ PHASE 1 ACHIEVEMENTS
- **pgmq Foundation Working**: Basic queue operations tested and validated
- **Ruby Architecture Transformed**: Pure Ruby workers using pg gem, no FFI coupling
- **Infrastructure Simplified**: Removed complex TCP command system (36 files)
- **Type System Complete**: Full dry-struct validation for all message types
- **Integration Tests Passing**: Comprehensive test suite validates end-to-end functionality

### 🎯 NEXT PHASE: Step Enqueueing (Phase 2)
**Objective**: Replace BatchExecutionSender with queue-based step publishing

**Priority Tasks**:
1. **Adapt workflow_coordinator.rs**: Replace TCP commands with pgmq step enqueueing
2. **Create step message serialization**: Convert step execution data to queue messages  
3. **Implement enqueue_ready_steps()**: Queue-based step discovery and publishing
4. **Update task initialization**: Trigger initial step enqueueing after task creation
5. **Orchestrator polling loop**: Check task progress and enqueue newly ready steps

**Key Files to Modify**:
- `src/orchestration/workflow_coordinator.rs` - Replace batch execution with queue enqueueing
- `src/execution/command_handlers/batch_execution_sender.rs` - Convert to queue-based publishing
- `src/orchestration/viable_step_discovery.rs` - Integrate with queue enqueueing
- `bindings/ruby/lib/tasker_core/execution/batch_execution_handler.rb` - Process queue messages

### 🔮 FUTURE PHASES
- **Phase 3**: Queue-based worker integration (Ruby workers processing queue messages)
- **Phase 4**: Result aggregation (task completion tracking via database)
- **Phase 5**: Migration cleanup (complete removal of old TCP infrastructure)

## Testing Strategy

### pgmq Integration Tests ✅
```bash
# Test basic queue operations
DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test bundle exec rspec spec/integration/pgmq_architecture_spec.rb:29 --format documentation

# Results: ✅ PASSING
# PGMQ Architecture Integration
#   Phase 1: Basic Queue Operations
#     can create and manage queues ✓
```

**Test Coverage**: 15 comprehensive integration tests covering:
- Queue creation, deletion, purging
- Message sending, reading, deleting, archiving
- Step message processing with dry-struct validation
- Queue worker framework with autonomous processing
- SQL function integration for status queries
- End-to-end message lifecycle validation

### Business Logic Tests (Preserved)
- Existing step handler tests remain unchanged
- Integration tests for order fulfillment workflow preserved
- Core business logic validation continues working

## Related Projects

- **tasker-engine/**: Production-ready Rails engine for workflow orchestration  
- **tasker-blog/**: GitBook documentation with real-world engineering stories

## MCP Server Integration

This project uses Model Context Protocol (MCP) servers for enhanced development:
- **PostgreSQL MCP**: Database operations and pgmq management
- **GitHub MCP**: Repository operations and CI/CD integration
- **Rust Documentation MCP**: Real-time Rust best practices and API guidance

**Configuration**: All MCP servers configured in `.mcp.json`

## Key Documentation

- **Architecture Roadmap**: `docs/roadmap/pgmq-pivot.md` - Comprehensive pivot analysis and implementation plan
- **Phase 1 Results**: All pgmq foundation components complete and tested
- **Migration Analysis**: File-by-file analysis of TCP → pgmq transformation completed

## Success Metrics Achieved

- ✅ **Architectural Simplicity**: Eliminated complex TCP coordination (36 files removed)
- ✅ **FFI Decoupling**: Pure Ruby implementation using standard libraries
- ✅ **Autonomous Processing**: Workers operate independently without registration
- ✅ **Database Integration**: Shared PostgreSQL access with proper separation of concerns
- ✅ **Type Safety**: Complete dry-struct validation system
- ✅ **Test Coverage**: Comprehensive integration tests validate architecture
- ✅ **Rails Philosophy**: Return to proven simplicity of original Rails Tasker

**Next Milestone**: Complete Phase 2 step enqueueing to enable full workflow orchestration through queues.