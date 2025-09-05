# Simplified Message Architecture Plan

## Current Problem

We're overcomplicating the orchestration-to-execution message flow by trying to serialize entire execution contexts and avoid database access on the Ruby side. This leads to:

- Complex serialization/deserialization between Rust and Ruby
- Type conversion issues with hash-to-object transformations
- Large message payloads with duplicated database data
- Fragile dry-struct type system integration
- Handler compatibility issues (expecting ActiveRecord-like objects)

## Solution: Database-Backed Simple Messages

Since both Rust orchestration and Ruby execution share the same database, we can leverage this as our "API layer" and dramatically simplify messages.

### New Message Structure

**Current Complex Message:**
```json
{
  "step_id": 12345,
  "task_id": 67890,
  "namespace": "fulfillment",
  "step_name": "validate_order",
  "step_payload": { /* complex data */ },
  "execution_context": {
    "task": { /* full task data */ },
    "dependencies": [ /* dependency results */ ],
    "step": { /* full step configuration */ }
  },
  "metadata": { /* timing, retry info */ }
}
```

**New Simple Message (UUID-based for data integrity):**
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

### Why UUIDs Instead of Integer IDs?

**Problem with Integer IDs:**
- Between tests, database tables are truncated but pgmq queues are not cleaned up
- Stale queue messages with integer IDs could accidentally process wrong tasks/steps
- Auto-incrementing IDs get reused after table truncation
- Tight coupling between database primary keys and queue message integrity

**UUID Solution:**
- Globally unique identifiers prevent ID collision across test runs
- Stale queue messages with non-existent UUIDs are safely ignored
- No accidental processing of wrong tasks due to ID reuse
- Better data integrity in production environments

### Workflow Changes

#### Rust Orchestration Side
1. **Claims tasks** from `tasker_ready_tasks` view (unchanged)
2. **Discovers ready steps** for claimed tasks (unchanged)
3. **Determines dependencies**: Query database to find completed dependency steps
4. **Enqueues simple message**: Just 3 integers instead of complex nested structure

#### Ruby Execution Side
1. **Receives simple message** from namespace queue
2. **Fetches data via ActiveRecord using UUIDs**:
   ```ruby
   task = TaskerCore::Database::Models::Task.find_by!(task_uuid: message.task_uuid)
   step = TaskerCore::Database::Models::WorkflowStep.find_by!(step_uuid: message.step_uuid)
   dependencies = TaskerCore::Database::Models::WorkflowStep.where(
     step_uuid: message.ready_dependency_step_uuids
   ).includes(:results) # Efficient loading of step results
   ```
3. **Creates sequence object**: Wrap dependencies in handler-expected interface
4. **Calls handler**: `handler.call(task, sequence, step)` with real ActiveRecord models
5. **Error handling**: Gracefully handle missing records (stale queue messages)

## Benefits

1. **Dramatically simpler messages**: 3 UUIDs vs complex nested structures (>80% size reduction)
2. **No serialization complexity**: Database handles all data persistence
3. **Real ActiveRecord models**: Handlers get full ORM functionality (associations, validations, etc.)
4. **Single source of truth**: Database contains authoritative data
5. **Better performance**: Smaller messages, efficient database queries
6. **Eliminates type conversion**: No more hash-to-object transformation issues
7. **Leverages Rails patterns**: Uses ActiveRecord as intended
8. **Data integrity**: UUIDs prevent stale queue messages from processing wrong records
9. **Test reliability**: No ID collision between test runs, safer test isolation
10. **Production safety**: No accidental processing during data migrations or cleanup

## Database Schema Changes

### Required Migrations

Before implementing the new message structure, we need to add UUID columns to our core tables.

#### Migration 1: Add UUID to Tasks Table
```sql
-- migrations/add_task_uuid_to_tasks.sql
-- Add UUID column to tasks table
ALTER TABLE tasker_tasks ADD COLUMN task_uuid UUID DEFAULT gen_random_uuid();

-- Backfill existing records (they'll get default UUIDs)
UPDATE tasker_tasks SET task_uuid = gen_random_uuid() WHERE task_uuid IS NULL;

-- Make it NOT NULL and add unique constraint
ALTER TABLE tasker_tasks ALTER COLUMN task_uuid SET NOT NULL;
ALTER TABLE tasker_tasks ADD CONSTRAINT tasker_tasks_task_uuid_unique UNIQUE (task_uuid);

-- Add index for fast UUID lookups
CREATE INDEX index_tasker_tasks_on_task_uuid ON tasker_tasks (task_uuid);
```

#### Migration 2: Add UUID to Workflow Steps Table
```sql
-- migrations/add_step_uuid_to_workflow_steps.sql
-- Add UUID column to workflow_steps table
ALTER TABLE tasker_workflow_steps ADD COLUMN step_uuid UUID DEFAULT gen_random_uuid();

-- Backfill existing records
UPDATE tasker_workflow_steps SET step_uuid = gen_random_uuid() WHERE step_uuid IS NULL;

-- Make it NOT NULL and add unique constraint
ALTER TABLE tasker_workflow_steps ALTER COLUMN step_uuid SET NOT NULL;
ALTER TABLE tasker_workflow_steps ADD CONSTRAINT tasker_workflow_steps_step_uuid_unique UNIQUE (step_uuid);

-- Add index for fast UUID lookups
CREATE INDEX index_tasker_workflow_steps_on_step_uuid ON tasker_workflow_steps (step_uuid);
```

#### ActiveRecord Model Updates
```ruby
# In TaskerCore::Database::Models::Task
class Task < ApplicationRecord
  # Ensure new records get UUIDs (fallback if DB default fails)
  before_create :ensure_task_uuid

  private

  def ensure_task_uuid
    self.task_uuid ||= SecureRandom.uuid
  end
end

# In TaskerCore::Database::Models::WorkflowStep
class WorkflowStep < ApplicationRecord
  # Ensure new records get UUIDs (fallback if DB default fails)
  before_create :ensure_step_uuid

  private

  def ensure_step_uuid
    self.step_uuid ||= SecureRandom.uuid
  end
end
```

### Schema Benefits
- **Maintains existing relationships**: Integer PKs and FKs remain unchanged
- **Adds external identity**: UUIDs for queue messages and external references
- **Performance**: UUID indexes are fast for lookups, minimal overhead
- **Data integrity**: No stale queue message issues
- **Test reliability**: Each test run has unique identifiers

## Implementation Phases

### Phase 0: Database Schema Updates
- [ ] Create and run UUID migration for tasks table
- [ ] Create and run UUID migration for workflow_steps table
- [ ] Update ActiveRecord models with UUID defaults
- [ ] Verify UUID generation works for new records

### Phase 1: Define New Message Structure
- [ ] Create `SimpleStepMessage` type in Ruby
- [ ] Update `pgmq_client.rb` to handle new format
- [ ] Keep backward compatibility with old messages

### Phase 2: Update Ruby Processing
- [ ] Modify `queue_worker.rb` to process simple messages
- [ ] Add ActiveRecord queries for task, step, dependencies
- [ ] Create `StepSequence` wrapper for dependencies
- [ ] Add error handling for missing records
- [ ] Update handler calling convention

### Phase 3: Update Rust Enqueueing
- [ ] Identify Rust step enqueueing code
- [ ] Modify to create simple messages
- [ ] Ensure dependency calculation works correctly
- [ ] Remove complex context serialization

### Phase 4: Testing & Validation
- [ ] Update integration tests for new message format
- [ ] Verify step handlers work with ActiveRecord models
- [ ] Performance testing for database query impact
- [ ] End-to-end workflow testing

### Phase 5: Cleanup
- [ ] Remove old complex message processing code
- [ ] Clean up unused dry-struct types
- [ ] Update documentation

## Comprehensive Code Analysis: What Can Be Removed vs. Modified

### Files to Remove Completely (No Backward Compatibility)

#### Ruby Files for Complete Removal
- `workers/ruby/lib/tasker_core/internal/distributed_handler_registry.rb` (912 lines)
  - Complex callable/Proc/Lambda registration system - NOT NEEDED
  - TaskTemplate YAML loading can be moved to simpler utilities
  - Overly complex bootstrap and discovery logic
  - 80%+ of this file can be removed

### Files to Simplify Dramatically (>70% Reduction)

#### Ruby Files for Major Simplification
- **`workers/ruby/lib/tasker_core/types/step_message.rb` (525 lines → ~150 lines)**
  - **REMOVE (~375 lines):**
    - Lines 23-122: `StepDependencyResult`, `DependencyChain` - dependencies fetched from DB
    - Lines 125-158: `StepExecutionContext` - complex execution context
    - Lines 161-196: `StepMessageMetadata` - complex retry/timeout metadata
    - Lines 200-324: `StepMessage` - complex nested message structure
    - Lines 454-523: `QueueMessage`, `QueueMessageData` - complex queue parsing
  - **KEEP (~150 lines):**
    - Lines 12-21: Basic types (`StepId`, `TaskId`, etc.) - still useful for validation
    - Lines 327-355: `StepExecutionStatus` - result status still needed
    - Lines 358-366: `StepExecutionError` - error handling still needed
    - Lines 369-450: `StepResult` - result reporting still needed
  - **ADD (~25 lines):** New `SimpleStepMessage` type

- **`workers/ruby/lib/tasker_core/messaging/queue_worker.rb` (825 lines → ~300 lines)**
  - **REMOVE (~525 lines):**
    - Lines 789-821: `create_task_object_from_context`, `create_step_object_from_context` - no more hash conversion
    - Lines 634-704: Complex metadata processing and orchestration extraction
    - Lines 522-632: `execute_step_handler_with_metadata` with type conversion complexity
    - Lines 201-242: `can_extract_execution_context?` - validation no longer needed
    - Lines 547-550: Complex object creation from execution context
    - Lines 415-457: `process_queue_message` - deprecated sequential processing
  - **KEEP (~200 lines):**
    - Basic worker lifecycle (start/stop/polling)
    - Configuration loading
    - Error handling and logging
    - Result sending to orchestration
  - **MODIFY (~100 lines):** New simple message processing with ActiveRecord queries

- **`workers/ruby/lib/tasker_core/registry/step_handler_registry.rb` (271 lines → ~100 lines)**
  - **REMOVE (~171 lines):**
    - Lines 160-225: Complex `get_task_template_for_task` with caching and TaskTemplate parsing
    - Lines 227-237: `find_step_template` within parsed TaskTemplate
    - Lines 25-38: Complex `CacheEntry` and `ResolvedStepHandler` dry-structs
    - Complex database query chains to resolve handler classes from TaskTemplate configurations
  - **KEEP (~50 lines):**
    - Basic registry interface
    - Handler class resolution via constantize
    - Cache statistics
  - **ADD (~50 lines):** Simple UUID-based WorkflowStep lookup for handler class resolution

### Files to Modify (Targeted Changes)

#### Ruby Files for Modification
- **`workers/ruby/lib/tasker_core/registry.rb` (60 lines)**
  - Update facade methods to work with simplified registry
  - Remove complex registry method delegates

- **Integration test files**
  - Update to work with new simple message format
  - Remove complex message structure assertions
  - Add UUID-based test data setup

### New Files to Create

#### Ruby Files to Add
- **`workers/ruby/lib/tasker_core/types/simple_message.rb` (~50 lines)**
  ```ruby
  class SimpleStepMessage < Dry::Struct
    attribute :task_uuid, Types::String
    attribute :step_uuid, Types::String
    attribute :ready_dependency_step_uuids, Types::Array.of(Types::String)
  end
  ```

- **`workers/ruby/lib/tasker_core/execution/step_sequence.rb` (~100 lines)**
  ```ruby
  class StepSequence
    # Wrapper for dependency steps loaded from database
    # Provides handler-expected interface for dependencies
  end
  ```

### Rust Side Changes Required

#### Files to Modify
- **`src/messaging/message.rs`**
  - Replace complex `StepMessage` struct with simple 3-UUID message
  - Remove `StepExecutionContext`, `StepMessageMetadata` structs
  - Add `SimpleStepMessage` struct

- **`src/orchestration/step_enqueuer.rs`**
  - Modify step enqueueing to create simple messages
  - Replace complex context building with UUID collection
  - Remove dependency result serialization

- **`src/orchestration/orchestration_loop.rs`**
  - Update to enqueue simple messages instead of complex ones
  - Simplify dependency resolution to UUID collection

#### Estimated Rust Reduction
- ~200-300 lines removed from complex message creation
- ~100-150 lines of context serialization logic removed
- Simpler dependency resolution logic

## Benefits Summary

### Code Reduction
- **Ruby side**: ~1,000 lines removed (~70% reduction in message processing complexity)
- **Rust side**: ~300 lines removed from message creation complexity
- **Total**: ~1,300 lines of complex serialization/deserialization code eliminated

### Architectural Benefits
- **Single source of truth**: Database becomes the API layer
- **Real ActiveRecord models**: Handlers get full ORM functionality
- **Eliminates type conversion**: No more hash-to-object transformation
- **Message size reduction**: >80% smaller messages (3 UUIDs vs complex nested JSON)
- **UUID data integrity**: No more stale queue messages processing wrong records
- **Rails philosophy**: Embraces ActiveRecord patterns instead of fighting them
- **Test reliability**: No ID collision between test runs, safer isolation

### Performance Benefits
- **Smaller messages**: Faster queue operations, less network overhead
- **Database optimization**: Efficient UUID indexes, proper SQL queries
- **Reduced memory**: No complex object graphs in messages
- **Faster parsing**: Simple JSON vs. complex nested structures

### New Ruby Components

#### Simple Message Type
```ruby
class SimpleStepMessage < Dry::Struct
  attribute :task_uuid, Types::String # UUID as string
  attribute :step_uuid, Types::String # UUID as string
  attribute :ready_dependency_step_uuids, Types::Array.of(Types::String) # Array of UUID strings
end
```

#### Step Sequence Wrapper
```ruby
class StepSequence
  def initialize(dependency_steps)
    @dependency_steps = dependency_steps
  end

  def dependencies
    @dependency_steps
  end

  def each(&block)
    @dependency_steps.each(&block)
  end

  # Additional convenience methods as needed
end
```

#### Updated Queue Worker Logic
```ruby
def process_simple_message(simple_message)
  # Fetch real ActiveRecord models using UUIDs
  task = TaskerCore::Database::Models::Task.find_by!(task_uuid: simple_message.task_uuid)
  step = TaskerCore::Database::Models::WorkflowStep.find_by!(step_uuid: simple_message.step_uuid)
  dependency_steps = TaskerCore::Database::Models::WorkflowStep.where(
    step_uuid: simple_message.ready_dependency_step_uuids
  ).includes(:results) # Or whatever association holds step results

  # Create sequence wrapper
  sequence = StepSequence.new(dependency_steps)

  # Call handler with real ActiveRecord models
  handler.call(task, sequence, step)
rescue ActiveRecord::RecordNotFound => e
  # Handle missing records gracefully (stale queue messages)
  logger.warn "Step processing failed - record not found: #{e.message}"
  logger.info "This is likely a stale queue message - safely ignoring"
  create_stale_message_result(simple_message, e)
end
```

## Risk Mitigation

### Database Load
- **Risk**: Additional queries per message processing
- **Mitigation**:
  - Ensure proper indexes on `task_id`, `workflow_step_id`
  - Use ActiveRecord includes/joins for efficient loading
  - Database queries are simpler than current object conversion

### Race Conditions
- **Risk**: Task/step deleted between enqueue and processing
- **Mitigation**: Graceful error handling, proper logging, maybe retry logic

### Handler Compatibility
- **Risk**: Handlers expecting specific object interfaces
- **Mitigation**: ActiveRecord models provide richer interface than current hashes

## Success Metrics

1. **Linear workflow integration test passes** without type conversion errors
2. **Message payload size** reduced by >80%
3. **Queue worker processing** simplified by removing complex type conversion
4. **Step handlers** receive proper ActiveRecord models with full functionality
5. **End-to-end workflow completion** works reliably

This approach aligns with Rails/ActiveRecord patterns and leverages the shared database as a strength rather than fighting against it.
