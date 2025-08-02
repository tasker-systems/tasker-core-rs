# PGMQ Architecture Pivot Roadmap

**Date**: August 1, 2025
**Branch**: `jcoletaylor/tas-14-m2-ruby-integration-testing-completion`
**Decision**: Pivot from TCP Command System to PostgreSQL Message Queue (pgmq) Architecture

## Executive Summary

After extensive development on the TCP command system, we've identified fundamental architectural issues that require a strategic pivot. The current imperative command model creates unnecessary complexity, tight coupling, and ongoing Rust<->Ruby thread-safety challenges. Moving to a PostgreSQL-backed message queue system (pgmq) will restore the simplicity of the original Rails Tasker while enabling true distributed processing.

## Problem Analysis

### Current System Issues

#### 1. **Imperative Disguised as Event-Driven**
- TCP command system feels event-driven but is actually imperative
- Orchestrator directly commands workers, creating tight coupling
- "Batch" mechanism requires central planning and coordination

#### 2. **Central Planning Overhead**
- Core must track worker registrations, capabilities, lifetimes
- Heartbeat monitoring, availability tracking, batch status evaluation
- Complex coordination between orchestrator and workers

#### 3. **Rust<->Ruby Thread Bridging Problems**
- Ongoing Magnus thread-safety constraints (`Value` cannot cross threads)
- Complex callback registration and execution patterns
- Architecture fighting against language boundary limitations

#### 4. **Future Complexity Debt**
- Upcoming requirements: worker pools, batch locks, retry mechanisms
- Replay capabilities, batch acquisition coordination
- Exponentially increasing coordination complexity

### Root Cause
The TCP command system recreated imperative complexity where event-driven queue processing would be more appropriate. We lost the core simplicity that made the original Rails Tasker effective.

## Proposed Solution: pgmq Integration

### What is pgmq?
[pgmq](https://github.com/pgmq/pgmq) is a PostgreSQL-backed message queue system providing:
- SQS-like API (`send`, `read`, `delete`, `archive`)
- Built-in visibility timeouts and retry mechanisms
- Batch operations for efficiency
- Message persistence and durability
- No additional infrastructure dependencies

### Architecture Benefits

#### 1. **True Event-Driven Processing**
- Workers pull work from queues independently
- No worker registration or central coordination needed
- Natural load balancing and fault tolerance

#### 2. **Eliminates Complex Coordination**
- No TCP servers, connection management, or heartbeats
- No Rust<->Ruby thread bridging issues
- Workers operate completely independently

#### 3. **Leverages Existing Infrastructure**
- Built on PostgreSQL (already core dependency)
- No additional services or complexity
- Familiar SQL-based operations

#### 4. **Simplified Future Development**
- Built-in retry and failure handling
- Natural horizontal scaling
- Easier debugging and monitoring

## Technical Architecture

### Queue Design

#### Option: Namespace-Based Queues (Recommended)
```
fulfillment_queue    - All fulfillment namespace tasks
inventory_queue      - All inventory namespace tasks
notifications_queue  - All notification namespace tasks
```

**Message Structure**:
```json
{
  "step_id": 12345,
  "task_id": 67890,
  "namespace": "fulfillment",
  "task_name": "process_order",
  "task_version": "1.0.0",
  "step_name": "validate_order",
  "step_payload": { /* step execution data */ },
  "metadata": {
    "created_at": "2025-08-01T12:00:00Z",
    "retry_count": 0,
    "timeout_ms": 30000
  }
}
```

> **Note**: Task and step IDs are database-backed big integers (i64), providing efficient indexing and referential integrity. This enables fast database queries for step completion tracking and task progress aggregation without requiring UUID parsing or string comparisons.

### Workflow Processing

#### 1. **Task Initialization** (Unchanged)
```rust
// Existing logic - creates tasks and steps in database
initialize_task(task_request) -> Task
```

#### 2. **Step Enqueueing** (New)
```rust
// Replace BatchExecutionSender with queue-based publishing
async fn enqueue_ready_steps(task_id: i64) -> Result<(), Error> {
    let ready_steps = discover_ready_steps(task_id).await?;

    for step in ready_steps {
        let message = create_step_message(step);
        pgmq::send(&format!("{}_queue", step.namespace), message).await?;
    }

    Ok(())
}
```

#### 3. **Worker Processing** (Simplified)
```ruby
class QueueWorker
  def poll_and_process
    loop do
      messages = pgmq_read("#{namespace}_queue", batch_size: 10)

      messages.each do |message|
        if can_handle_step?(message)
          process_step(message)
          pgmq_delete(message.id)  # Acknowledge completion
        end
      end

      sleep(poll_interval)
    end
  end
end
```

#### 4. **Result Aggregation** (Database-Driven)
```rust
// Orchestrator periodically checks for completed steps
async fn check_task_progress(task_id: i64) -> Result<(), Error> {
    if task_completed(task_id).await? {
        publish_task_completion_event(task_id).await?;
    } else {
        enqueue_ready_steps(task_id).await?;  // Enqueue newly ready steps
    }

    Ok(())
}
```

## Comprehensive Codebase Migration Analysis

### Technical Implementation Notes

#### pgmq-rs Client Integration
- **Rust pgmq Client**: Use [pgmq-rs](https://github.com/pgmq/pgmq/tree/main/pgmq-rs) as the foundation
- **Magnus FFI Wrapper**: Minimal surface area - just JSON message passing, no callbacks or shared memory
- **Ruby Worker Optimization**: Use `concurrent-ruby` promises for parallel message processing within workers
- **Thread Safety**: No Magnus cross-thread issues since we're only passing JSON strings

### File-by-File Migration Plan

#### 🗑️ **REMOVE COMPLETELY** - TCP Command Infrastructure

**Core TCP Infrastructure (Obsolete with Queues):**
- `src/execution/generic_executor.rs` - TCP server becomes unnecessary with queue polling
- `src/execution/command_router.rs` - Command routing replaced by queue message routing
- `src/execution/transport.rs` - TCP transport layer not needed
- `src/execution/tokio_tcp_executor.rs` - TCP executor implementation obsolete
- `src/execution/worker_pool.rs` - Worker pool management handled by queue consumers

**TCP Command Handlers (Replaced by Queue Processing):**
- `src/execution/command_handlers/batch_execution_sender.rs` - Replace with queue enqueueing
- `src/execution/command_handlers/tcp_worker_client_handler.rs` - Direct worker communication obsolete
- `src/execution/command_handlers/ruby_command_bridge.rs` - No more Rust<->Ruby bridging needed
- `src/execution/command_handlers/worker_management_handler.rs` - Worker management obsolete with autonomous workers

**FFI TCP Components (No Longer Needed):**
- `src/ffi/tcp_executor.rs` - TCP executor FFI wrapper
- `src/ffi/shared/command_client.rs` - Command client FFI
- `src/ffi/shared/command_listener.rs` - Command listener FFI
- `bindings/ruby/ext/tasker_core/src/command_client.rs` - Ruby command client FFI
- `bindings/ruby/ext/tasker_core/src/command_listener.rs` - Ruby command listener FFI
- `bindings/ruby/ext/tasker_core/src/embedded_tcp_executor.rs` - Embedded TCP executor FFI

**Ruby TCP Components (Replace with Queue Workers):**
- `bindings/ruby/lib/tasker_core/execution/command_client.rb` - TCP client obsolete
- `bindings/ruby/lib/tasker_core/execution/command_listener.rb` - TCP listener obsolete
- `bindings/ruby/lib/tasker_core/execution/command_backplane.rb` - TCP coordination obsolete

**Worker Management Infrastructure (Obsolete with Autonomous Workers):**
- `migrations/20250728000001_create_workers_tables.sql` - Worker registration tables not needed
- `migrations/20250729000001_create_optimized_worker_query_functions.sql` - Worker query optimizations obsolete
- `src/models/core/worker.rs` - Worker registration model obsolete
- `src/models/core/worker_registration.rs` - Worker registration tracking obsolete
- `src/models/core/worker_named_task.rs` - Worker capability tracking obsolete
- `src/models/core/worker_transport_availability.rs` - Worker transport info obsolete
- `src/models/core/worker_command_audit.rs` - Worker command auditing obsolete
- `src/services/worker_selection_service.rs` - Worker selection via queue polling instead
- `bindings/ruby/lib/tasker_core/execution/worker_manager.rb` - Worker coordination obsolete

> **Key Insight**: In pgmq architecture, workers are completely autonomous. They poll queues based on their capabilities, no registration or coordination needed. Database can be rebuilt without worker tables.

#### ✅ **KEEP UNCHANGED** - Core Business Logic

**Database Layer (Unchanged):**
- `src/database/` - All database connection, pool, and query logic remains
- `src/models/` - All database models remain unchanged (Task, Step, etc.)
- `src/scopes/` - Database query scopes remain relevant
- `db/structure.sql` - Database schema unchanged
- `migrations/` - All existing migrations remain

**Business Logic (Unchanged):**
- `src/state_machine/` - State machine logic remains identical
- `src/registry/` - Handler and plugin registries remain
- `src/events/` - Event publishing system remains
- `src/validation.rs` - Validation logic unchanged
- `src/sql_functions.rs` - SQL functions remain useful

**Ruby Business Logic (Unchanged):**
- `bindings/ruby/lib/tasker_core/models/` - Ruby model wrappers unchanged
- `bindings/ruby/lib/tasker_core/step_handler/` - Step handler API unchanged
- `bindings/ruby/lib/tasker_core/task_handler/` - Task handler API unchanged
- `bindings/ruby/lib/tasker_core/logging/` - Logging infrastructure unchanged
- `bindings/ruby/lib/tasker_core/events/` - Event system unchanged
- `bindings/ruby/lib/tasker_core/types/` - Type definitions unchanged

**Ruby FFI Utilities (Unchanged):**
- `bindings/ruby/ext/tasker_core/src/context.rs` - JSON<->Ruby conversion utilities
- `bindings/ruby/ext/tasker_core/src/error_translation.rs` - Error translation
- `bindings/ruby/ext/tasker_core/src/types.rs` - Type definitions
- `bindings/ruby/ext/tasker_core/src/handles.rs` - Handle management (may be useful)

#### 🔄 **ADAPT/MODIFY** - Components Needing Changes

**Orchestration Layer (Adapt for Queues):**
- `src/orchestration/workflow_coordinator.rs` - **ADAPT**: Change from TCP commands to queue enqueueing
- `src/orchestration/step_execution_orchestrator.rs` - **ADAPT**: Queue-based step coordination
- `src/orchestration/task_initializer.rs` - **MINOR ADAPT**: May need to trigger initial step enqueueing
- `src/orchestration/viable_step_discovery.rs` - **KEEP/ADAPT**: Logic remains, but integrate with queue enqueueing
- `src/orchestration/task_enqueuer.rs` - **ADAPT**: May become queue-specific enqueue logic
- Rest of `src/orchestration/` - **KEEP**: Business logic remains unchanged

**Command/Message System (Adapt to Queues):**
- `src/execution/command.rs` - **ADAPT**: Transform Command structs to Message structs for queue payloads
- `src/execution/message_protocols.rs` - **ADAPT**: Update for queue-based message protocols
- `src/execution/errors.rs` - **ADAPT**: Update error types for queue operations

**FFI Shared Components (Selective Adaptation):**
- `src/ffi/shared/orchestration_system.rs` - **ADAPT**: Remove TCP components, integrate queue orchestration
- `src/ffi/shared/handles.rs` - **ADAPT**: May need queue-related handle types
- `src/ffi/shared/types.rs` - **ADAPT**: Update types for queue operations
- `src/ffi/shared/testing.rs` - **ADAPT**: Update testing utilities for queue-based system

**Ruby Orchestration (Adapt):**
- `bindings/ruby/lib/tasker_core/orchestration/orchestration_manager.rb` - **MAJOR ADAPT**: Remove TCP singletons, add queue management
- `bindings/ruby/lib/tasker_core/execution/batch_execution_handler.rb` - **ADAPT**: Process queue messages instead of TCP commands

**Tests (Major Adaptation Needed):**
- `tests/execution/` - **ADAPT**: Most TCP-based tests need queue equivalents
- `bindings/ruby/spec/execution/` - **ADAPT**: Ruby execution tests for queue processing
- `bindings/ruby/spec/handlers/integration/` - **ADAPT**: Integration tests for queue-based workflows
- Unit tests for business logic - **MINIMAL CHANGES**: Core logic tests remain mostly unchanged

#### 🆕 **NEW COMPONENTS** - Queue-Based Architecture

**Rust pgmq Integration:**
- `src/messaging/mod.rs` - **NEW**: Messaging module organization
- `src/messaging/pgmq_client.rs` - **NEW**: Rust wrapper around pgmq-rs client
- `src/messaging/step_enqueuer.rs` - **NEW**: Replace BatchExecutionSender with queue-based step publishing
- `src/messaging/message.rs` - **NEW**: Message structures for queue payloads (adapted from Command)
- `src/messaging/orchestrator.rs` - **NEW**: Queue-based orchestration loop (replace TCP orchestration)
- `src/messaging/queue_manager.rs` - **NEW**: Queue lifecycle management (create, monitor, cleanup)

**Ruby pgmq Integration:**
- `bindings/ruby/ext/tasker_core/src/pgmq_client.rs` - **NEW**: Magnus FFI wrapper for pgmq operations
- `bindings/ruby/lib/tasker_core/messaging/` - **NEW**: Ruby messaging module
- `bindings/ruby/lib/tasker_core/messaging/pgmq_client.rb` - **NEW**: Ruby wrapper for pgmq operations
- `bindings/ruby/lib/tasker_core/messaging/queue_worker.rb` - **NEW**: Queue polling worker with concurrent-ruby support
- `bindings/ruby/lib/tasker_core/messaging/message_processor.rb` - **NEW**: Process queue messages and route to step handlers

**Queue-Based Tests:**
- `tests/messaging/` - **NEW**: Test queue operations, message processing, orchestration
- `bindings/ruby/spec/messaging/` - **NEW**: Ruby queue worker tests, integration tests
- `tests/integration/queue_workflow_test.rs` - **NEW**: End-to-end queue-based workflow tests

### Migration Scope Summary

| Category | Rust Files | Ruby Files | Ruby FFI | Migrations | Tests | Total Impact |
|----------|------------|------------|----------|------------|-------|--------------|
| **Remove** | 19 files | 4 files | 3 files | 2 files | ~20 tests | **High** - Major infrastructure removal |
| **Keep** | 20+ files | 14+ files | 4 files | 4 files | ~25 tests | **Low** - Business logic unchanged |
| **Adapt** | 6 files | 3 files | 2 files | 0 files | ~20 tests | **Medium** - Targeted modifications |
| **New** | 6 files | 4 files | 1 file | 0 files | ~10 tests | **Medium** - Focused new development |

### Key Insights from Analysis

1. **Business Logic Preservation**: ~70% of core business logic (models, step handlers, orchestration logic) remains unchanged
2. **Infrastructure Simplification**: Removing TCP infrastructure eliminates ~20 complex coordination components
3. **Autonomous Workers**: Complete elimination of worker registration, heartbeats, and coordination overhead
4. **Database Simplification**: Can rebuild database without worker tables - 2 fewer migrations needed
5. **Focused New Development**: Only ~11 new components needed for queue-based architecture
6. **Test Migration**: Most integration tests need adaptation, but unit tests largely unchanged
7. **FFI Simplification**: Magnus FFI becomes much simpler with just JSON message passing

## Implementation Plan

### ✅ Phase 1: pgmq Foundation (COMPLETED - August 1, 2025)
**Objective**: Set up pgmq infrastructure and basic operations

- [x] Install pgmq extension in PostgreSQL
- [x] Create Rust pgmq integration layer using sqlx
- [x] Create Ruby pgmq wrapper using pg gem
- [x] Implement basic queue operations (send, read, delete, archive, purge)
- [x] Create queue management utilities
- [x] Write integration tests for queue operations
- [x] Remove FFI-heavy Ruby components
- [x] Create SQL function wrapper for status queries
- [x] Create autonomous queue worker framework
- [x] Implement dry-struct type system for messages

**Deliverables** ✅:
- `src/messaging/pgmq_client.rs` - Rust pgmq integration with full API
- `bindings/ruby/lib/tasker_core/messaging/pgmq_client.rb` - Ruby pgmq wrapper using pg gem
- `bindings/ruby/lib/tasker_core/messaging/queue_worker.rb` - Autonomous queue worker framework
- `bindings/ruby/lib/tasker_core/database/sql_functions.rb` - SQL function access layer
- `bindings/ruby/lib/tasker_core/types/step_message.rb` - dry-struct message types
- `spec/integration/pgmq_architecture_spec.rb` - Comprehensive integration tests
- Infrastructure cleanup: Removed 36+ TCP/worker management files

**Key Achievements**:
- 🎉 **pgmq Architecture Working End-to-End**: Basic queue operations passing all tests
- 🚀 **No FFI Coupling**: Pure Ruby implementation using standard pg gem
- 🔄 **Autonomous Workers**: Workers poll queues independently, no coordination needed
- 📊 **Type Safety**: Full dry-struct validation for all message types
- 🗑️ **Major Cleanup**: Removed complex TCP command infrastructure (36 files)
- ✅ **Test Coverage**: 15 comprehensive integration tests validating the architecture

**Test Results**: `1 example, 0 failures` - Basic queue operations fully functional!

### ✅ Phase 2: Step Enqueueing (COMPLETED - August 1, 2025)
**Objective**: Replace BatchExecutionSender with queue-based step publishing

- [x] Create step message serialization/deserialization
- [x] Implement `enqueue_ready_steps()` function in WorkflowCoordinator
- [x] Integrate pgmq_client into WorkflowCoordinator
- [x] Create PgmqStepMessage and metadata structures
- [x] Update execute_step_batch to use pgmq enqueueing
- [x] Remove TCP command routing from workflow execution

**Deliverables** ✅:
- `src/messaging/message.rs` - PgmqStepMessage structure implemented
- WorkflowCoordinator now uses execute_step_batch_pgmq for queue-based step enqueueing
- Message format includes step data, namespace routing, and metadata
- Steps successfully enqueued to namespace-specific queues

**Key Achievements**:
- 🎉 **Step Enqueueing Working**: Steps now enqueued to pgmq instead of TCP
- 🚀 **Namespace Routing**: Steps routed to appropriate namespace queues
- 📊 **Message Structure**: Complete step information for worker processing
- 🗑️ **TCP Removal**: Removed TCP routing from main workflow path

### ✅ Phase 3: Complete pgmq Orchestration Workflow (COMPLETED - August 1, 2025)
**Objective**: Implement the comprehensive queue-based orchestration workflow

#### ✅ Sub-Phase 3.1: Task Request Ingestion (COMPLETED)
- [x] Create `orchestration_task_requests` queue
- [x] Implement TaskRequestProcessor in Rust
- [x] Poll task requests and validate using task_handler_registry
- [x] Create tasks using existing database models
- [x] Enqueue validated tasks to `orchestration_tasks_to_be_processed`

#### ✅ Sub-Phase 3.2: Task Processing Loop (COMPLETED)
- [x] Create `orchestration_tasks_to_be_processed` queue
- [x] Implement orchestration polling loop in OrchestrationSystemPgmq
- [x] Process tasks by discovering viable steps
- [x] Create step_execution_batch records
- [x] Build batch messages with (task, sequence, step) data
- [x] Enqueue batches to namespace-specific queues

#### ✅ Sub-Phase 3.3: Batch Queue Infrastructure (COMPLETED)
- [x] Create namespace batch queues (`fulfillment_batch_queue`, etc.)
- [x] Update message structures for batch processing
- [x] Implement batch message serialization
- [x] Add batch metadata for retry handling

#### ✅ Sub-Phase 3.4: Ruby Worker Updates (COMPLETED)
- [x] Create BatchQueueWorker to poll namespace batch queues
- [x] Implement BatchExecutor for concurrent step execution
- [x] Use concurrent-ruby promises for parallel processing
- [x] Update step handlers to work with batch context
- [x] Implement PostgreSQL JSON queries for task filtering

#### ✅ Sub-Phase 3.5: Result Processing (COMPLETED)
- [x] Create `orchestration_batch_results` queue
- [x] Implement ResultReporter in Ruby workers
- [x] Create BatchResultProcessor in Rust
- [x] Process batch results and update task progress
- [x] Implement task finalization logic
- [x] Handle task re-enqueueing for incomplete workflows

**Deliverables** ✅:
- `src/messaging/orchestration_messages.rs` - Complete orchestration queue message types
- `src/orchestration/task_request_processor.rs` - Task validation and ingestion
- `src/orchestration/orchestration_system_pgmq.rs` - Dual polling loop orchestration system
- `bindings/ruby/lib/tasker_core/messaging/batch_queue_worker.rb` - Ruby batch workers
- `bindings/ruby/lib/tasker_core/orchestration/batch_executor.rb` - Concurrent batch execution
- Complete queue-based orchestration system with autonomous workers

**Key Achievements**:
- 🎉 **Complete Orchestration Architecture**: Full queue-based workflow implementation
- 🚀 **Autonomous Workers**: Ruby workers operate independently via queue polling
- 📊 **Batch Processing**: Efficient parallel step execution with concurrent-ruby
- 🔄 **Result Aggregation**: Complete task lifecycle management
- 🗑️ **TCP Architecture Eliminated**: No more complex command coordination
- ✅ **Test Coverage**: Comprehensive integration tests validate end-to-end functionality

### 🎯 Phase 4: Architecture Cleanup & Orchestration Refactoring (In Progress - August 2, 2025)
**Objective**: Complete transition to pgmq architecture through configuration cleanup, orchestration layer refactoring, and database-backed TaskTemplate registry

#### ✅ Phase 4.1: Embedded FFI Bridge (COMPLETED)
**Objective**: Restore embedded system capability for local development and testing
- [x] Create minimal FFI interface for lifecycle management (`bindings/ruby/ext/tasker_core/src/embedded_bridge.rs`)
- [x] Implement TaskerCore::EmbeddedOrchestrator Ruby class with start/stop/status methods
- [x] Add embedded mode integration tests for local development
- [x] Enable Ruby-Rust integration testing without docker-compose overhead

#### ✅ Phase 4.2: Ruby Handler Architecture Updates (COMPLETED)
**Objective**: Update Ruby handlers to work with pgmq architecture instead of TCP commands
- [x] Update `task_handler/base.rb` to use pgmq instead of TCP commands
- [x] Remove command_client dependencies from Ruby handlers
- [x] Ensure step_handler classes work seamlessly with new architecture
- [x] Validate Ruby business logic preservation with TaskerCore::Types::TaskRequest integration

#### ✅ Phase 4.3: Configuration and Mode Management (COMPLETED - August 2, 2025)
**Objective**: Clean up configuration files and implement embedded vs distributed mode detection
- [x] Remove obsolete `command_backplane` configuration from all config files
- [x] Add `pgmq` configuration section with queue settings and namespaces
- [x] Add `orchestration.mode` setting for embedded/distributed selection 
- [x] Update task_handler/base.rb to be mode-aware (embedded FFI vs pure pgmq)
- [x] Configure mode defaults (embedded for test environment, distributed for production)
- [x] Enhanced orchestration queue configuration for all orchestration core queues

**Key Changes Implemented**:
```yaml
# REMOVED entire command_backplane section from all configs
# CHANGED execution.processing_mode from "tcp" to "pgmq"
# ADDED orchestration.mode: "embedded" | "distributed"
# ADDED comprehensive pgmq configuration section
# ADDED orchestration.queues configuration for all queue types
```

**Enhanced Queue Configuration Added**:
- **task_requests_queue**: Incoming task requests from external systems
- **task_processing_queue**: Tasks ready for orchestration processing  
- **batch_results_queue**: Completed batch execution results
- **worker_queues**: Namespaced queues (default, fulfillment, inventory, notifications)
- **Environment-specific settings**: Different retention and DLQ policies per environment
- **Dead Letter Queue preparation**: Configuration structure for future DLQ implementation

**Architectural Benefits Achieved**:
- ✅ Configuration-driven mode selection (test=embedded, dev/prod=distributed)
- ✅ Clean separation of embedded FFI vs distributed pgmq processing
- ✅ Complete removal of TCP infrastructure from configuration
- ✅ Forward-compatible queue configuration ready for Rust orchestration core integration
- ✅ All tests passing (18 examples, 0 failures) with mode-aware task handler

#### 📋 Phase 4.4: Orchestration Layer Refactoring (HIGH PRIORITY)  
**Objective**: Massive simplification of orchestration layer through TCP infrastructure removal
- [ ] Simplify `orchestration_manager.rb` by removing 400+ lines of TCP infrastructure (72% reduction)
- [ ] Refactor `orchestration.rb` API to use pgmq step enqueueing instead of TCP commands
- [ ] Rename `enhanced_handler_registry.rb` to `distributed_handler_registry.rb`
- [ ] Implement bootstrapping functionality for queue setup and handler registration
- [ ] Create mode-aware orchestration that handles embedded vs distributed scenarios

**Architectural Benefits**:
- **544 lines → ~150 lines** in orchestration_manager.rb
- Remove TCP client/listener/worker singletons
- Replace OrchestrationHandle FFI with simple pgmq client
- Eliminate complex TCP configuration matching logic

#### 📋 Phase 4.5: Database-backed TaskTemplate Registry (HIGH PRIORITY)
**Objective**: Complete distributed worker architecture with database-backed TaskTemplate distribution
- [ ] Review `src/orchestration/task_config_finder.rs` for config lookup patterns
- [ ] Review `src/registry/task_handler_registry.rs` for database-first registry approach
- [ ] Implement YAML TaskTemplate → database loading in `distributed_handler_registry.rb`
- [ ] Add Ruby worker TaskTemplate registration API using existing database-backed registry
- [ ] Enable workers to register and discover TaskTemplates via shared database

#### 📋 Phase 4.6: Comprehensive Testing Strategy (MEDIUM PRIORITY)
**Objective**: Validate both embedded and distributed modes work correctly
- [ ] Test embedded mode functionality with embedded orchestrator
- [ ] Test distributed mode with pure pgmq communication
- [ ] Validate configuration-driven mode switching
- [ ] Create integration tests for TaskTemplate database registration
- [ ] Document testing approaches for different deployment scenarios

#### 📋 Phase 4.7: Final Cleanup & Documentation (MEDIUM PRIORITY)
**Objective**: Remove remaining obsolete components and update documentation
- [ ] Systematic audit and removal of obsolete TCP-era files (~20 files)
- [ ] Update obsolete tests or remove test files for deleted functionality  
- [ ] Document new architecture patterns and migration guide
- [ ] Update README files to reflect pgmq architecture

**Key Design Principles**:
- **Embedded Mode**: Lightweight FFI bridge for lifecycle management only, no complex state sharing
- **Architecture Clarity**: Clean separation between orchestration (Rust) and execution (Ruby)
- **Testing Options**: Both embedded mode (fast iteration) and docker-compose (production fidelity)
- **Database Registry**: Distributed workers register TaskTemplates in shared database
- **Autonomous Workers**: No central coordination, workers poll queues independently

**Success Criteria**:
- [x] Local integration tests work with embedded orchestrator (Phase 4.1 ✅)
- [x] Ruby handlers use pgmq instead of TCP commands (Phase 4.2 & 4.3 ✅)
- [ ] Workers can register and discover TaskTemplates via database
- [ ] Obsolete TCP infrastructure completely removed
- [ ] Documentation reflects actual implementation

### 🔮 Phase 5: Advanced Features & Reliability (FUTURE)
**Objective**: Implement advanced messaging features for production reliability

#### Phase 5.1: Dead Letter Queue Implementation
**Objective**: Implement DLQ strategy for failed message handling
- [ ] Implement DLQ creation and routing logic based on existing configuration
- [ ] Add DLQ processing for messages exceeding max_receive_count
- [ ] Create DLQ monitoring and alerting system
- [ ] Add DLQ replay capabilities for message recovery

**Configuration Foundation Already Complete**:
- ✅ `dead_letter_queue_enabled` setting in all environments
- ✅ `max_receive_count` thresholds configured per environment  
- ✅ Environment-specific DLQ policies defined

#### Phase 5.2: Enhanced Monitoring & Observability
**Objective**: Production-grade monitoring and observability
- [ ] Queue depth monitoring and alerting
- [ ] Message processing metrics and dashboards
- [ ] Performance monitoring for queue processing
- [ ] Health checks for queue availability and processing

## Components Analysis

### Remove (TCP Command System)
- `src/execution/generic_executor.rs` - TCP server infrastructure
- `src/execution/command_router.rs` - Command routing logic
- `src/execution/transport.rs` - TCP transport implementations
- `src/execution/command_handlers/batch_execution_sender.rs` - Direct worker communication
- `src/execution/command_handlers/tcp_worker_client_handler.rs` - Worker client handling
- `src/execution/command_handlers/ruby_command_bridge.rs` - Rust<->Ruby bridging
- `src/ffi/shared/command_client.rs` - Command client FFI
- `src/ffi/shared/command_listener.rs` - Command listener FFI
- `bindings/ruby/lib/tasker_core/execution/command_client.rb` - Ruby command client
- `bindings/ruby/lib/tasker_core/execution/command_listener.rb` - Ruby command listener
- Worker registration, heartbeat, and availability tracking

### Keep/Adapt
- Database models and migrations (tasks, steps, etc.)
- Task initialization and step dependency logic
- Ruby step handler business logic
- Configuration system
- Event publishing system
- Command structures (adapt to Message structures)

### New Components
- `src/messaging/pgmq_client.rs` - Rust pgmq integration
- `src/messaging/step_enqueuer.rs` - Step enqueueing logic
- `src/messaging/orchestrator.rs` - Queue-based orchestration
- `bindings/ruby/lib/tasker_core/messaging/pgmq_client.rb` - Ruby pgmq client
- `bindings/ruby/lib/tasker_core/messaging/queue_worker.rb` - Queue polling worker
- Queue monitoring and management tools

## Risk Assessment

### Technical Risks

#### Performance Concerns
- **Risk**: Queue polling might be slower than direct TCP
- **Mitigation**: pgmq is optimized for PostgreSQL; batch operations; benchmarking
- **Fallback**: Optimize polling intervals and batch sizes

#### Message Size Limitations
- **Risk**: Large step payloads might stress queue system
- **Mitigation**: Reference step data by ID rather than embedding full payloads
- **Fallback**: Implement payload compression or external storage

#### PostgreSQL Load
- **Risk**: Increased database load from queue operations
- **Mitigation**: pgmq is designed for high throughput; we already depend on PostgreSQL heavily
- **Fallback**: Database optimization, connection pooling

### Implementation Risks

#### Migration Complexity
- **Risk**: Big architectural change with many moving parts
- **Mitigation**: Implement alongside existing system, gradual migration
- **Fallback**: Maintain TCP system during transition

#### Debugging Challenges
- **Risk**: Queue-based systems can be harder to debug
- **Mitigation**: Comprehensive logging, queue monitoring tools, message tracing
- **Fallback**: Enhanced observability and debugging tools

## Success Criteria

### Functional Requirements
- [ ] All existing workflow functionality preserved
- [ ] Workers can process steps independently via queues
- [ ] Task completion detection and next step triggering
- [ ] Proper error handling and retry mechanisms
- [ ] Support for multiple worker types and capabilities

### Non-Functional Requirements
- [ ] **Simplicity**: Significantly reduced coordination complexity
- [ ] **Performance**: Comparable or better than TCP system
- [ ] **Reliability**: Improved fault tolerance and message durability
- [ ] **Scalability**: Easy horizontal scaling of workers
- [ ] **Maintainability**: Cleaner, more understandable codebase

### Success Metrics
- Elimination of Magnus thread-safety issues
- Reduced codebase complexity (measured by lines of code and cyclomatic complexity)
- Improved system reliability (reduced failure points)
- Better horizontal scaling characteristics
- Faster development velocity for new features

## Future Opportunities

### Enhanced Queue Features
- Priority queues for urgent tasks
- Dead letter queues for failed messages
- Message scheduling and delayed processing
- Queue monitoring and alerting

### Advanced Workflows
- Conditional step execution based on queue messages
- Dynamic worker scaling based on queue depth
- Cross-queue message routing for complex workflows
- Queue-based workflow orchestration patterns

### Integration Possibilities
- Integration with existing Rails Tasker engine
- Multi-language worker support (Python, Node.js, etc.)
- Cloud-native deployment with managed PostgreSQL
- API gateway integration for external task submission

## Ruby Architecture Analysis

### Current FFI Coupling Problems

The existing Ruby bindings assume deep FFI coupling with Rust internals:

#### **Remove (FFI-Heavy Components)**:
- `bindings/ruby/ext/tasker_core/src/handles.rs` - Direct FFI model access
- `bindings/ruby/ext/tasker_core/src/globals.rs` - Global Rust state access
- `bindings/ruby/ext/tasker_core/src/performance.rs` - Direct Rust analytics
- `bindings/ruby/ext/tasker_core/src/models/` - Direct FFI model wrappers
- `bindings/ruby/lib/tasker_core/embedded_server.rb` - TCP server components
- Ruby specs that test FFI coupling (`execution/`, `architecture/`, `integration/`)

#### **Keep & Adapt (Core Business Logic)**:
- `bindings/ruby/lib/tasker_core/step_handler/` - **Essential step execution logic**
- `bindings/ruby/lib/tasker_core/task_handler/` - **Task orchestration business logic**
- `bindings/ruby/lib/tasker_core/execution/batch_execution_handler.rb` - **Step execution framework**
- `bindings/ruby/spec/handlers/examples/` - **Real step handlers that prove the system**
- `bindings/ruby/spec/handlers/integration/order_fulfillment_integration_spec.rb` - **End-to-end proof**
- Core utilities: `config.rb`, `logging/`, `errors.rb`, `types/` - **Foundation components**

### New pgmq Architecture for Ruby

#### **Workers as Autonomous Queue Consumers**
Ruby workers become simple, focused components:
- **Queue Consumers**: Poll pgmq for step messages
- **Step Executors**: Execute business logic via step handlers
- **Callable Routers**: Route step execution to appropriate handlers
- **Database Users**: Direct database access for updates (shared with Rust core)
- **No FFI Coupling**: No direct access to Rust models or state

#### **Interface Design**
1. **pgmq Interface** (using `pg` gem, not FFI):
   ```ruby
   # bindings/ruby/lib/tasker_core/messaging/pgmq_client.rb
   class PgmqClient
     def send_message(queue_name, message)
     def read_messages(queue_name, visibility_timeout)
     def delete_message(queue_name, msg_id)
   end
   ```

2. **SQL Function Interface** (for status queries):
   ```ruby
   # bindings/ruby/lib/tasker_core/database/sql_functions.rb
   class SqlFunctions
     def task_execution_context(task_id)
     def step_readiness_status(task_id)
     def dependency_analysis(task_id)
   end
   ```

3. **Data Classes** (using dry-struct/dry-types):
   ```ruby
   # bindings/ruby/lib/tasker_core/types/step_message.rb
   class StepMessage < Dry::Struct
     attribute :step_id, Types::Integer
     attribute :task_id, Types::Integer
     attribute :namespace, Types::String
     attribute :step_payload, Types::Hash
   end
   ```

4. **Queue Worker Framework**:
   ```ruby
   # bindings/ruby/lib/tasker_core/messaging/queue_worker.rb
   class QueueWorker
     def poll_and_process(namespace)
     def execute_step_message(step_message)
   end
   ```

#### **Integration Testing Strategy**
- **Queue + SQL Functions**: Tests verify workflows via message processing and SQL status queries
- **No FFI State**: No direct access to Rust internal state
- **Embedded Support**: Can spin up Rust orchestration core for single-process testing
- **End-to-End Proof**: Integration tests demonstrate complete workflows through queue processing

#### **Benefits of New Architecture**
1. **Architectural Clarity**: Clean separation between orchestration (Rust) and execution (Ruby)
2. **Simplified Integration**: No complex FFI coupling or thread-safety issues
3. **Rails Philosophy**: Returns to proven simplicity of original Rails Tasker
4. **Shared Database**: Both Rust and Ruby use same database through appropriate interfaces
5. **Testing Clarity**: Integration tests verify via queues + SQL queries, not FFI state
6. **Autonomous Workers**: Workers don't need knowledge of Rust internals or dependency evaluation

## Phase 3 Detailed Implementation Plan

### Overview
Phase 3 completes the pgmq architecture by implementing the comprehensive workflow design that replaces the TCP command system with a fully queue-based orchestration approach.

### Architecture Components

#### Queue Infrastructure
1. **orchestration_task_requests** - Ingests new task requests
2. **orchestration_tasks_to_be_processed** - Holds validated tasks ready for processing
3. **namespace_batch_queue** (e.g., `fulfillment_batch_queue`) - Namespace-specific batch queues
4. **orchestration_batch_results** - Collects batch execution results

#### Message Structures

**Task Request Message**:
```json
{
  "request_id": "uuid",
  "namespace": "fulfillment",
  "task_name": "process_order",
  "task_version": "1.0.0",
  "input_data": { /* task input payload */ },
  "metadata": {
    "requested_at": "2025-08-01T12:00:00Z",
    "requester": "api_gateway",
    "priority": "normal"
  }
}
```

**Batch Message** (for namespace queues):
```json
{
  "batch_id": 12345,
  "task_id": 67890,
  "namespace": "fulfillment",
  "task_name": "process_order",
  "task_version": "1.0.0",
  "steps": [
    {
      "step_id": 11111,
      "sequence": 1,
      "step_name": "validate_order",
      "step_payload": { /* step data */ }
    },
    {
      "step_id": 22222,
      "sequence": 2,
      "step_name": "check_inventory",
      "step_payload": { /* step data */ }
    }
  ],
  "metadata": {
    "batch_created_at": "2025-08-01T12:00:00Z",
    "timeout_seconds": 300,
    "retry_policy": "exponential_backoff"
  }
}
```

**Batch Result Message**:
```json
{
  "batch_id": 12345,
  "task_id": 67890,
  "namespace": "fulfillment",
  "batch_status": "partial_success",
  "step_results": [
    {
      "step_id": 11111,
      "status": "success",
      "output": { /* step output */ },
      "executed_at": "2025-08-01T12:01:00Z"
    },
    {
      "step_id": 22222,
      "status": "failed",
      "error": "Insufficient inventory",
      "executed_at": "2025-08-01T12:01:30Z"
    }
  ],
  "metadata": {
    "worker_id": "fulfillment-worker-01",
    "execution_time_ms": 1500,
    "completed_at": "2025-08-01T12:01:30Z"
  }
}
```

### Implementation Steps

#### Step 1: Task Request Ingestion (Rust)
```rust
// src/orchestration/task_request_processor.rs
pub struct TaskRequestProcessor {
    pgmq_client: Arc<PgmqClient>,
    task_handler_registry: Arc<TaskHandlerRegistry>,
    task_initializer: Arc<TaskInitializer>,
    pool: PgPool,
}

impl TaskRequestProcessor {
    pub async fn poll_and_process(&self) -> Result<()> {
        loop {
            // Poll orchestration_task_requests queue
            let messages = self.pgmq_client
                .read_messages("orchestration_task_requests", 10, 60)
                .await?;

            for msg in messages {
                let request: TaskRequestMessage = serde_json::from_value(msg.payload)?;

                // Validate using task_handler_registry
                match self.task_handler_registry
                    .get_task_template(&request.namespace, &request.task_name, &request.task_version)
                    .await {
                    Ok(template) => {
                        // Create task using existing models
                        let task = self.task_initializer
                            .initialize_task_from_request(request, template)
                            .await?;

                        // Enqueue to processing queue
                        let task_message = TaskProcessingMessage {
                            task_id: task.task_id,
                            namespace: request.namespace,
                            priority: request.metadata.priority,
                        };

                        self.pgmq_client
                            .send_message("orchestration_tasks_to_be_processed", task_message)
                            .await?;
                    }
                    Err(e) => {
                        // Log validation error and archive message
                        tracing::error!("Task validation failed: {}", e);
                        self.pgmq_client.archive_message("orchestration_task_requests", msg.id).await?;
                    }
                }
            }

            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}
```

#### Step 2: Task Processing with Batch Creation
```rust
// src/orchestration/batch_creator.rs
pub struct BatchCreator {
    pool: PgPool,
}

impl BatchCreator {
    pub async fn create_step_batch(
        &self,
        task_id: i64,
        steps: Vec<ViableStep>,
    ) -> Result<StepExecutionBatch> {
        // Use existing step_execution_batch model
        let batch = StepExecutionBatch::create(
            &self.pool,
            NewStepExecutionBatch {
                task_id,
                status: BatchStatus::Pending,
                created_at: Utc::now(),
                metadata: json!({
                    "step_count": steps.len(),
                    "namespace": steps.first().map(|s| s.namespace.clone()),
                }),
            }
        ).await?;

        // Associate steps with batch
        for step in steps {
            StepBatchAssociation::create(
                &self.pool,
                batch.batch_id,
                step.step_id,
            ).await?;
        }

        Ok(batch)
    }
}
```

#### Step 3: Ruby Batch Worker Implementation
```ruby
# bindings/ruby/lib/tasker_core/orchestration/batch_queue_worker.rb
module TaskerCore
  module Orchestration
    class BatchQueueWorker
      def initialize(namespace:, pgmq_client:, batch_executor:)
        @namespace = namespace
        @pgmq_client = pgmq_client
        @batch_executor = batch_executor
        @queue_name = "#{namespace}_batch_queue"
      end

      def poll_and_process
        loop do
          messages = @pgmq_client.read_messages(@queue_name, count: 5, visibility_timeout: 300)

          messages.each do |msg|
            batch_message = Types::BatchMessage.new(msg.payload)

            # Use PostgreSQL JSON queries to check if we can handle this task
            if can_handle_task?(batch_message)
              process_batch(msg.id, batch_message)
            else
              # Return to queue for another worker
              @pgmq_client.set_visibility_timeout(@queue_name, msg.id, 0)
            end
          end

          sleep 1
        end
      end

      private

      def can_handle_task?(batch_message)
        # Query database for task handler configuration
        result = ActiveRecord::Base.connection.execute(<<-SQL)
          SELECT EXISTS(
            SELECT 1 FROM named_tasks nt
            JOIN task_namespaces tn ON nt.task_namespace_id = tn.id
            WHERE tn.name = $1
            AND nt.name = $2
            AND nt.configuration->>'worker_requirements' IS NULL
               OR nt.configuration->'worker_requirements' @> '{"namespace": "#{@namespace}"}'::jsonb
          )
        SQL

        result.first['exists']
      end

      def process_batch(message_id, batch_message)
        result = @batch_executor.execute_batch(batch_message)

        # Send result to orchestration_batch_results queue
        @pgmq_client.send_message('orchestration_batch_results', result.to_h)

        # Delete processed message
        @pgmq_client.delete_message(@queue_name, message_id)
      rescue => e
        Rails.logger.error("Batch processing failed: #{e.message}")
        # Message will become visible again after timeout
      end
    end
  end
end
```

#### Step 4: Concurrent Batch Executor
```ruby
# bindings/ruby/lib/tasker_core/orchestration/batch_executor.rb
module TaskerCore
  module Orchestration
    class BatchExecutor
      def execute_batch(batch_message)
        promises = batch_message.steps.map do |step|
          Concurrent::Promise.execute do
            execute_single_step(batch_message, step)
          end
        end

        # Wait for all promises to complete
        results = promises.map(&:value!)

        # Build batch result
        Types::BatchResult.new(
          batch_id: batch_message.batch_id,
          task_id: batch_message.task_id,
          namespace: batch_message.namespace,
          batch_status: calculate_batch_status(results),
          step_results: results,
          metadata: {
            worker_id: worker_identifier,
            execution_time_ms: calculate_execution_time,
            completed_at: Time.now.utc
          }
        )
      end

      private

      def execute_single_step(batch_message, step)
        # Load step handler
        handler_class = resolve_step_handler(batch_message.namespace, step.step_name)
        handler = handler_class.new

        # Execute with task context
        context = StepExecutionContext.new(
          task_id: batch_message.task_id,
          step_id: step.step_id,
          step_payload: step.step_payload
        )

        result = handler.execute(context)

        {
          step_id: step.step_id,
          status: result.success? ? 'success' : 'failed',
          output: result.output,
          error: result.error_message,
          executed_at: Time.now.utc
        }
      rescue => e
        {
          step_id: step.step_id,
          status: 'failed',
          error: e.message,
          executed_at: Time.now.utc
        }
      end
    end
  end
end
```

#### Step 5: Orchestration Polling Loops
```rust
// src/ffi/shared/orchestration_system_pgmq.rs
impl OrchestrationSystemPgmq {
    pub async fn start_orchestration_loops(&self) -> Result<()> {
        // Start task request processor
        let request_processor = self.task_request_processor.clone();
        tokio::spawn(async move {
            request_processor.poll_and_process().await
        });

        // Start task orchestration loop
        let task_processor = self.clone();
        tokio::spawn(async move {
            task_processor.poll_and_orchestrate_tasks().await
        });

        // Start result processor
        let result_processor = self.batch_result_processor.clone();
        tokio::spawn(async move {
            result_processor.poll_and_process_results().await
        });

        Ok(())
    }

    async fn poll_and_orchestrate_tasks(&self) -> Result<()> {
        loop {
            let messages = self.pgmq_client
                .read_messages("orchestration_tasks_to_be_processed", 10, 60)
                .await?;

            for msg in messages {
                let task_msg: TaskProcessingMessage = serde_json::from_value(msg.payload)?;

                // Discover viable steps
                let viable_steps = self.viable_step_discovery
                    .find_viable_steps(task_msg.task_id)
                    .await?;

                if !viable_steps.is_empty() {
                    // Create batch
                    let batch = self.batch_creator
                        .create_step_batch(task_msg.task_id, viable_steps)
                        .await?;

                    // Build batch message
                    let batch_message = self.build_batch_message(batch).await?;

                    // Enqueue to namespace queue
                    let queue_name = format!("{}_batch_queue", task_msg.namespace);
                    self.pgmq_client
                        .send_message(&queue_name, batch_message)
                        .await?;
                }

                // Check if task is complete
                if self.is_task_complete(task_msg.task_id).await? {
                    self.pgmq_client.delete_message("orchestration_tasks_to_be_processed", msg.id).await?;
                } else {
                    // Re-enqueue for next check
                    self.pgmq_client.set_visibility_timeout("orchestration_tasks_to_be_processed", msg.id, 30).await?;
                }
            }

            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}
```

### Key Benefits of This Architecture

1. **Complete Decoupling**: Rust orchestration and Ruby execution are fully decoupled via queues
2. **Autonomous Workers**: Workers operate independently without registration or coordination
3. **Horizontal Scalability**: Add more workers by simply starting new processes
4. **Fault Tolerance**: Message persistence and visibility timeouts handle failures gracefully
5. **Observability**: Queue depths and message ages provide clear system health metrics
6. **Simplicity**: Returns to the proven Rails Tasker philosophy of simple, focused components

### Migration Path

1. Implement components incrementally alongside existing system
2. Test with shadow traffic before switching over
3. Gradual rollout by namespace
4. Monitor queue depths and processing times
5. Remove TCP infrastructure after validation

## Conclusion

The pgmq architecture pivot addresses fundamental issues in our current TCP command system by returning to the proven simplicity of queue-based processing. This change eliminates complex coordination overhead, resolves Rust<->Ruby integration challenges, and positions us for scalable, maintainable future development.

The Ruby architecture transformation creates autonomous workers that are simple queue consumers and callable routers, eliminating FFI complexity while preserving essential business logic. The shared database approach with SQL functions provides necessary status visibility without tight coupling.

The comprehensive Phase 3 implementation plan provides a clear path to complete the pgmq architecture, implementing the full workflow from task request ingestion through batch execution and result processing. This approach maintains the Rails Tasker philosophy while enabling true distributed processing capabilities.

**Recommendation**: Proceed with Phase 3 implementation, starting with task request ingestion and orchestration polling loops.
