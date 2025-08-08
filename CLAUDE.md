# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**tasker-core-rs** is a high-performance Rust implementation of workflow orchestration, designed to complement the existing Ruby on Rails **Tasker** engine at `/Users/petetaylor/projects/tasker-systems/tasker-engine/`.

**Architecture**: PostgreSQL message queue (pgmq) based system where Rust handles orchestration and step enqueueing, while Ruby workers autonomously process steps through queue polling - eliminating FFI coupling and coordination complexity.

## Current Status (August 7, 2025)

### 🎉 WORKFLOW PATTERN STANDARDIZATION COMPLETE (August 7, 2025)
- **Major Achievement**: Successfully standardized all workflow examples to use consistent `sequence.get_results()` pattern
- **Problem Solved**: Eliminated inconsistent step result retrieval patterns across all workflow examples
- **Solution**: Unified all step handlers to use proven linear workflow patterns
- **Result**: ✅ **All Workflow Examples Working** - Linear, Mixed DAG, Tree, Diamond, and Order Fulfillment

### ✅ COMPLETED PHASE: Workflow Pattern Unification (August 7, 2025)
**Objective**: Update all workflow examples to follow the proven patterns from linear workflow

**Pattern Standardization**:
- **Before**: Mixed patterns - `sequence.get().dig()`, `sequence.steps.find()`, inconsistent return structures
- **After**: Unified `sequence.get_results('step_name')` pattern across all workflows
- **Ruby Processing**: All handlers now use consistent result retrieval and return `TaskerCore::Types::StepHandlerCallResult.success`
- **Benefits**: Consistent developer experience, reliable examples, easier maintenance

### 🔄 NEXT PHASE: Simple Message Implementation 
**Objective**: Replace complex nested message structures with simple UUID-based messages

**Architecture Change**:
- **Old**: Complex nested JSON with execution context, metadata, dependencies  
- **New**: Simple 3-field message: `{task_uuid, step_uuid, ready_dependency_step_uuids}`
- **Ruby Processing**: ActiveRecord models fetched via UUID, handlers get real AR objects
- **Benefits**: 80% message size reduction, eliminates type conversion, prevents stale message issues

## Architecture Overview

### Core Components (UUID-based Simple Messages)
- **Message Queues**: PostgreSQL-backed queues using pgmq extension for reliability
- **Simple Messages**: 3-UUID structure eliminating complex serialization  
- **Database as API**: Ruby workers query database with UUIDs to get ActiveRecord models
- **Autonomous Workers**: Ruby workers poll queues and execute step handlers with real AR objects
- **UUID Data Integrity**: Prevents stale queue messages from processing wrong records

### Queue Design Pattern
```
fulfillment_queue    - All fulfillment namespace steps
inventory_queue      - All inventory namespace steps  
notifications_queue  - All notification namespace steps
```

**New Simple Message Structure**:
```json
{
  "task_uuid": "550e8400-e29b-41d4-a716-446655440001",
  "step_uuid": "550e8400-e29b-41d4-a716-446655440002",
  "ready_dependency_step_uuids": [
    "550e8400-e29b-41d4-a716-446655440003",
    "550e8400-e29b-41d4-a716-446655440004"
  ]
}
```

**Ruby Processing Flow**:
```ruby
# 1. Receive simple message
task = TaskerCore::Database::Models::Task.find_by!(task_uuid: message.task_uuid)
step = TaskerCore::Database::Models::WorkflowStep.find_by!(step_uuid: message.step_uuid)
dependencies = TaskerCore::Database::Models::WorkflowStep.where(
  step_uuid: message.ready_dependency_step_uuids
).includes(:results)

# 2. Call handler with real ActiveRecord models
handler.call(task, sequence, step)
```

### Key Technical Patterns
- **Database-Driven Architecture**: Shared PostgreSQL database as the API layer
- **UUID-Based Messaging**: Prevents ID collision and stale message processing
- **ActiveRecord Integration**: Ruby handlers work with full ORM functionality
- **Simple Message Validation**: Minimal dry-struct validation on UUIDs only
- **Test-Safe Processing**: No ID reuse between test runs, safer test isolation

## Development Guidelines

### Code Quality Standards
- **No Placeholder Code**: All implementations must be complete, no TODOs in production paths
- **Simple Message Design**: Use UUID-based messages, leverage database as API layer
- **ActiveRecord Integration**: Ruby handlers should work with real AR models, not hashes
- **UUID-First**: All external references should use UUIDs, not integer PKs
- **No Backward Compatibility**: Aggressive simplification allowed, no legacy support needed

### Current Working Branch
- **Branch**: `jcoletaylor/tas-14-m2-ruby-integration-testing-completion`
- **Focus**: Simple message architecture implementation with UUID-based processing

## Key File Locations

### Simple Message Architecture (In Progress)
- **Rust Message Types**: `src/messaging/message.rs` - Simple message structures
- **Rust Step Enqueuer**: `src/orchestration/step_enqueuer.rs` - UUID-based message creation
- **Ruby Simple Messages**: `bindings/ruby/lib/tasker_core/types/simple_message.rb` - NEW
- **Ruby Queue Workers**: `bindings/ruby/lib/tasker_core/messaging/queue_worker.rb` - To be simplified
- **Ruby Registry**: `bindings/ruby/lib/tasker_core/registry/step_handler_registry.rb` - To be simplified

### Database Schema  
- **UUID Migration**: `migrations/20250806120448_add_uuid_columns_to_tasks_and_workflow_steps.sql` ✅
- **PGMQ Extension**: `migrations/20250801000001_enable_pgmq_extension.sql` ✅

### Files for Major Simplification
- **Complex Types**: `bindings/ruby/lib/tasker_core/types/step_message.rb` (525 lines → ~150 lines)
- **Distributed Registry**: `bindings/ruby/lib/tasker_core/internal/distributed_handler_registry.rb` (912 lines → REMOVE)
- **Queue Worker**: `bindings/ruby/lib/tasker_core/messaging/queue_worker.rb` (825 lines → ~300 lines)

### Configuration
- **Database**: `.env` with `DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test`  
- **Config**: `config/tasker-config-test.yaml`
- **Architecture Plan**: `docs/simple-messages.md` - Complete implementation roadmap

## Development Commands

### Rust Core
```bash
# Core development (ALWAYS use --all-features for full consistency)
cargo build --all-features                         # Build project with all features
cargo test --all-features                          # Run tests with factory system and all features
cargo clippy --all-targets --all-features          # Lint code with all features
cargo fmt                                          # Format code

# Additional commands
cargo check --all-features                         # Fast compilation check with all features
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

## Implementation Progress (August 7, 2025)

### ✅ COMPLETED: Workflow Pattern Standardization (August 7, 2025)
**Major Achievement**: Successfully unified all workflow examples to use consistent patterns

#### Workflow Examples Updated:
1. **Linear Workflow** ✅ - Reference implementation (already correct)
2. **Mixed DAG Workflow** ✅ - Fixed 7 handlers to use `sequence.get_results()`
3. **Tree Workflow** ✅ - Fixed 7 handlers to use `sequence.get_results()`  
4. **Diamond Workflow** ✅ - Fixed 3 handlers to use `sequence.get_results()`
5. **Order Fulfillment** ✅ - Fixed 3 handlers to use `sequence.get_results()`

#### Pattern Changes Applied:
- **Before**: `sequence.get('step_name')&.dig('result')` → **After**: `sequence.get_results('step_name')`
- **Before**: `sequence.steps.find { |s| s.name == 'step_name' }.results` → **After**: `sequence.get_results('step_name')`
- **Return Format**: All handlers now return `TaskerCore::Types::StepHandlerCallResult.success(result:, metadata:)`

#### Integration Test Results:
- **Linear Workflow**: ✅ Working perfectly
- **Mixed DAG Workflow**: ✅ Complex dependency resolution working
- **Tree Workflow**: ✅ Hierarchical processing working  
- **Diamond Workflow**: ✅ Parallel processing working
- **Order Fulfillment**: ✅ Complete workflow chain working

### ✅ COMPLETED: Database Schema Foundation  
- **UUID Migration Applied** ✅: Both `tasker_tasks` and `tasker_workflow_steps` have UUID columns
- **Database Indexes** ✅: Efficient UUID lookup indexes created
- **Data Integrity** ✅: Unique constraints prevent UUID collisions
- **Schema Updated** ✅: Ready for UUID-based message processing

### 🔄 NEXT PRIORITY: Simple Message Architecture Implementation

**Upcoming Phase**: Replace complex message structures with simple UUID-based messages

**Priority Tasks**:
1. **Create SimpleStepMessage Ruby type** - New 3-field message structure
2. **Update ActiveRecord models** - Add UUID defaults and validation  
3. **Simplify queue_worker.rb** - Replace hash conversion with AR queries
4. **Simplify step_handler_registry.rb** - Remove complex TaskTemplate parsing
5. **Update Rust message creation** - Generate simple messages instead of complex ones

**Expected Benefits**:
- 🎯 **Message Size Reduction**: >80% smaller messages (3 UUIDs vs complex JSON)
- 🎯 **Type Conversion Elimination**: No more hash-to-object conversion issues
- 🎯 **Data Integrity**: UUID-based processing prevents stale message problems
- 🎯 **ActiveRecord Integration**: Handlers get real AR models with full ORM functionality  
- 🎯 **Test Reliability**: No ID collision between test runs

**Files Ready for Simplification**:
- `step_message.rb`: 525 lines → ~150 lines (~375 lines removable)
- `queue_worker.rb`: 825 lines → ~300 lines (~525 lines removable)  
- `distributed_handler_registry.rb`: 912 lines → REMOVE completely
- Total: ~1,300+ lines of complex serialization code can be eliminated

## Testing Strategy

### Current Testing Focus
```bash
# Test the failing integration test that motivated the architecture change
DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test TASKER_ENV=test bundle exec rspec spec/integration/linear_workflow_integration_spec.rb:55 --format documentation
```

**Goal**: Get the linear workflow integration test passing with the new simple message architecture.

### Implementation Testing Approach
1. **UUID Schema Validation**: Verify UUID columns and indexes work correctly
2. **Simple Message Creation**: Test new 3-field message structure in Ruby
3. **ActiveRecord Integration**: Verify handlers work with real AR models  
4. **Queue Processing**: Test simplified message processing without type conversion
5. **End-to-End Validation**: Full workflow completion with simple messages

## Related Projects

- **tasker-engine/**: Production-ready Rails engine for workflow orchestration  
- **tasker-blog/**: GitBook documentation with real-world engineering stories

## Key Documentation

- **Simple Message Plan**: `docs/simple-messages.md` - Complete implementation roadmap with line-by-line analysis
- **Database Migration**: `migrations/20250806120448_add_uuid_columns_to_tasks_and_workflow_steps.sql` - UUID schema foundation

## Success Metrics for Simple Message Architecture

**Target Achievements**:
- ✅ **Database Schema**: UUID columns and indexes applied successfully
- 🎯 **Code Reduction**: ~1,300 lines of complex serialization code eliminated  
- 🎯 **Message Size**: >80% reduction in message payload size
- 🎯 **Type Safety**: ActiveRecord models instead of hash-to-object conversion
- 🎯 **Test Reliability**: UUID-based processing prevents stale message issues
- 🎯 **Rails Integration**: Embrace ActiveRecord patterns instead of fighting them

### ✅ ACHIEVED SUCCESS METRICS

**Workflow Pattern Standardization (Completed August 7, 2025)**:
- ✅ **Pattern Consistency**: All 20 step handlers across 5 workflows use `sequence.get_results()` pattern
- ✅ **Integration Tests**: All workflow integration tests passing with core functionality verified
- ✅ **Developer Experience**: Consistent examples for all workflow patterns (Linear, DAG, Tree, Diamond, Chain)
- ✅ **Code Quality**: All handlers return proper `StepHandlerCallResult.success` with metadata
- ✅ **Maintenance**: Eliminated 4 different result retrieval patterns, unified to single approach

**Next Success Indicator**: `linear_workflow_integration_spec.rb:55` test passes with simple message processing and handlers receive proper ActiveRecord models.

**Next Milestone**: Begin implementation of SimpleStepMessage Ruby type and ActiveRecord model UUID defaults.