# TAS-96: Ruby Worker API Alignment

**Parent**: [TAS-92](./README.md)
**Linear**: [TAS-96](https://linear.app/tasker-systems/issue/TAS-96)
**Branch**: `jcoletaylor/tas-96-ruby-worker-api-alignment`
**Priority**: Medium

## Objective

Align Ruby worker APIs with cross-language standards. Ruby requires the most significant changes, primarily the handler signature migration from `call(task, sequence, step)` to `call(context)`.

## Key Discovery: TaskSequenceStepWrapper Already Exists

**We already have a unified context type!** The `TaskSequenceStepWrapper` in `workers/ruby/lib/tasker_core/models.rb` wraps:
- `task` → `TaskWrapper` (task_uuid, context, namespace_name, etc.)
- `workflow_step` → `WorkflowStepWrapper` (step execution state)
- `dependency_results` → `DependencyResultsWrapper`
- `step_definition` → `StepDefinitionWrapper`

**Plan**: Alias or rename `TaskSequenceStepWrapper` to `StepContext` and add convenience accessors for cross-language standard fields.

## Summary of Changes

| Area | Current State | Target State | Effort |
|------|---------------|--------------|--------|
| Handler Signature | `call(task, sequence, step)` | `call(context)` | **Medium** (simplified by reusing existing wrapper) |
| Result Factories | Already aligned | No change | None |
| Error Fields | Has `error_code` | Document `error_type` values | Low |
| Registry API | Different method names | Rename methods | Medium |
| API Handler | No HTTP conveniences | Add `get/post/put/delete` | Medium |
| Batchable | Different method names | Rename methods | Low |
| Domain Events | Exists | Add `publish(ctx)` method | Low |

## Files Requiring Signature Updates

### Ruby Code Files (16 files)

**Core Library:**
```
workers/ruby/lib/tasker_core/step_handler/base.rb           # Base class - primary change
workers/ruby/lib/tasker_core/step_handler/decision.rb       # Decision handler docs
workers/ruby/lib/tasker_core.rb                              # Module docs
workers/ruby/lib/tasker_core/handlers.rb                     # Handler docs
workers/ruby/lib/tasker_core/models.rb                       # Wrapper docs
workers/ruby/lib/tasker_core/types/batch_processing_outcome.rb
workers/ruby/lib/tasker_core/types/decision_point_outcome.rb
```

**Example Handlers (all use `def call(task, sequence, step)` or `def call(task, _sequence, _step)`):**
```
workers/ruby/spec/handlers/examples/order_fulfillment/step_handlers/validate_order_handler.rb
workers/ruby/spec/handlers/examples/order_fulfillment/step_handlers/process_payment_handler.rb
workers/ruby/spec/handlers/examples/order_fulfillment/step_handlers/reserve_inventory_handler.rb
workers/ruby/spec/handlers/examples/order_fulfillment/step_handlers/ship_order_handler.rb
workers/ruby/spec/handlers/examples/blog_examples/post_01_ecommerce/step_handlers/*.rb (5 files)
workers/ruby/spec/handlers/examples/error_scenarios/step_handlers/*.rb (3 files)
workers/ruby/spec/handlers/examples/diamond_workflow/step_handlers/*.rb (4 files)
workers/ruby/spec/handlers/examples/mixed_dag_workflow/step_handlers/*.rb (7 files)
# Plus ~60 more handlers in blog_examples subdirectories
```

### Documentation Files (60+ references)

**Primary docs to update:**
- `docs/worker-crates/ruby.md` (12 references)
- `docs/worker-crates/README.md` (2 references)
- `docs/worker-crates/patterns-and-practices.md` (3 references)
- `docs/batch-processing.md` (2 references)
- `docs/conditional-workflows.md` (5 references)
- `docs/use-cases-and-patterns.md` (1 reference)

**Ticket-specs (historical, may not need updating):**
- Various TAS-* specs contain historical examples

## Implementation Plan

### Phase 1: Create StepContext as Enhanced Wrapper

**Approach**: Create `StepContext` that wraps/extends `TaskSequenceStepWrapper` with cross-language standard accessors.

**File:** `workers/ruby/lib/tasker_core/types/step_context.rb`

```ruby
# frozen_string_literal: true

module TaskerCore
  module Types
    # StepContext provides a unified context for step handler execution.
    #
    # This is the cross-language standard context object passed to handler.call(context).
    # It wraps the FFI-provided TaskSequenceStepWrapper and adds convenience accessors
    # that match Python and Rust naming conventions.
    #
    # @example Accessing context in a handler
    #   def call(context)
    #     # Cross-language standard fields
    #     task_uuid = context.task_uuid
    #     step_uuid = context.step_uuid
    #     input_data = context.input_data
    #     deps = context.dependency_results
    #
    #     # Ruby-specific accessors (for compatibility)
    #     task = context.task
    #     step = context.workflow_step
    #   end
    class StepContext
      # @return [TaskWrapper] Task metadata and context
      # @return [WorkflowStepWrapper] Step execution state
      # @return [DependencyResultsWrapper] Results from parent steps
      # @return [StepDefinitionWrapper] Step definition from template
      attr_reader :task, :workflow_step, :dependency_results, :step_definition

      # Cross-language standard field names (delegated)
      delegate :task_uuid, :context, :namespace_name, to: :task
      delegate :workflow_step_uuid, :name, :inputs, :results, :attempts, :max_attempts,
               :retryable, to: :workflow_step

      # Creates a StepContext from FFI step data
      #
      # @param step_data [Hash, TaskSequenceStepWrapper] The step data from Rust FFI
      def initialize(step_data)
        if step_data.is_a?(Models::TaskSequenceStepWrapper)
          @task = step_data.task
          @workflow_step = step_data.workflow_step
          @dependency_results = step_data.dependency_results
          @step_definition = step_data.step_definition
        else
          wrapper = Models::TaskSequenceStepWrapper.new(step_data)
          @task = wrapper.task
          @workflow_step = wrapper.workflow_step
          @dependency_results = wrapper.dependency_results
          @step_definition = wrapper.step_definition
        end
      end

      # Cross-language standard: step_uuid
      # @return [String] UUID of the workflow step
      def step_uuid
        workflow_step.workflow_step_uuid
      end

      # Cross-language standard: input_data
      # @return [HashWithIndifferentAccess] Step input data
      def input_data
        workflow_step.inputs
      end

      # Cross-language standard: step_config
      # @return [HashWithIndifferentAccess] Handler configuration from template
      def step_config
        step_definition.handler&.initialization || {}.with_indifferent_access
      end

      # Cross-language standard: step_inputs (alias for input_data)
      alias step_inputs input_data

      # Cross-language standard: retry_count
      # @return [Integer] Current retry attempt count
      def retry_count
        workflow_step.attempts
      end

      # Cross-language standard: max_retries
      # @return [Integer] Maximum retry attempts allowed
      def max_retries
        workflow_step.max_attempts
      end

      # Convenience: get task context field
      # @param field_name [String, Symbol] Field name in task context
      # @return [Object, nil] The field value
      def get_task_field(field_name)
        task.context[field_name.to_s]
      end

      # Convenience: get dependency result
      # @param step_name [String, Symbol] Name of parent step
      # @return [Object, nil] The result value
      def get_dependency_result(step_name)
        dependency_results.get_results(step_name)
      end
    end
  end
end
```

### Phase 2: Update Base Handler

**File:** `workers/ruby/lib/tasker_core/step_handler/base.rb`

```ruby
# Update the call signature
def call(context)
  raise NotImplementedError, "#{self.class} must implement #call(context)"
end
```

The dispatch layer (FFI bridge) will construct the `StepContext` from the `TaskSequenceStepWrapper` before calling the handler.

### Phase 3: Registry API Alignment

**File:** `workers/ruby/lib/tasker_core/registry/handler_registry.rb` (if exists, otherwise in appropriate location)

| Before | After |
|--------|-------|
| `register_handler(name, klass)` | `register(name, klass)` |
| `handler_available?(name)` | `is_registered(name)` |
| `registered_handlers` | `list_handlers` |
| (via StepHandlerResolver) | `resolve(name)` |

### Phase 4: API Handler Enhancements

**File:** `workers/ruby/lib/tasker_core/step_handler/api.rb`

Add HTTP convenience methods:
```ruby
def get(path, params: {}, headers: {})
  connection.get(path, params, headers)
end

def post(path, data: {}, headers: {})
  connection.post(path, data, headers)
end

def put(path, data: {}, headers: {})
  connection.put(path, data, headers)
end

def delete(path, params: {}, headers: {})
  connection.delete(path, params, headers)
end
```

### Phase 5: Batchable Method Renames

**File:** `workers/ruby/lib/tasker_core/step_handler/batchable.rb`

| Before | After |
|--------|-------|
| `batch_worker_complete(...)` | `batch_worker_success(...)` |
| `extract_cursor_context(...)` | `get_batch_context(...)` |

Field names: `items_processed`, `items_succeeded`, `items_failed`

### Phase 6: Domain Events Publisher

**File:** `workers/ruby/lib/tasker_core/domain_events/base_publisher.rb`

Add `publish(ctx)` method that wraps existing `transform_payload`/`should_publish?`/`additional_metadata` hooks.

### Phase 7: Error Types Module

**New file:** `workers/ruby/lib/tasker_core/types/error_types.rb`

```ruby
module TaskerCore
  module Types
    module ErrorTypes
      PERMANENT_ERROR = 'permanent_error'
      RETRYABLE_ERROR = 'retryable_error'
      VALIDATION_ERROR = 'validation_error'
      TIMEOUT = 'timeout'
      HANDLER_ERROR = 'handler_error'

      ALL = [PERMANENT_ERROR, RETRYABLE_ERROR, VALIDATION_ERROR, TIMEOUT, HANDLER_ERROR].freeze

      def self.valid?(error_type)
        ALL.include?(error_type)
      end
    end
  end
end
```

### Phase 8: Update All Example Handlers

All handlers change from:
```ruby
def call(task, sequence, step)
  # Old style
end
```

To:
```ruby
def call(context)
  # New style - use context.task_uuid, context.input_data, etc.
  # Or for gradual migration: context.task, context.workflow_step
end
```

### Phase 9: Update Tests

Update all specs to use new APIs.

## Handler Migration Examples

**Simple handler:**
```ruby
# Before
def call(task, _sequence, step)
  even_number = task.context['even_number']
  success(result: even_number * 2)
end

# After
def call(context)
  even_number = context.get_task_field('even_number')
  success(result: even_number * 2)
end
```

**Handler with dependencies:**
```ruby
# Before
def call(_task, sequence, step)
  prev_result = sequence.get_dependency_result('step_1')
  success(result: prev_result + 1)
end

# After
def call(context)
  prev_result = context.get_dependency_result('step_1')
  success(result: prev_result + 1)
end
```

## Verification Checklist

- [ ] `StepContext` class implemented with all cross-language standard fields
- [ ] `StepContext` provides Ruby-specific convenience methods
- [ ] `Base#call(context)` signature works
- [ ] FFI dispatch builds StepContext from step_data
- [ ] Registry methods renamed: `register`, `is_registered`, `resolve`, `list_handlers`
- [ ] API handler has `get/post/put/delete` methods
- [ ] Batchable uses `batch_worker_success` and `get_batch_context`
- [ ] Domain event publisher has `publish(ctx)` method
- [ ] Error type constants defined in `ErrorTypes` module
- [ ] All ~89 example handlers updated to new signature
- [ ] All unit tests pass
- [ ] All integration tests pass
- [ ] FFI integration tests pass

## Risk Assessment

**Medium Risk** (reduced from High):
- Discovery of existing `TaskSequenceStepWrapper` simplifies implementation
- Pre-alpha status means no backward compatibility needed
- Cross-language standard fields are additive, not breaking

## Estimated Scope

- **New lines**: ~150 (StepContext with delegation, ErrorTypes)
- **Modified lines**: ~200 (base.rb, registry, api.rb, batchable.rb)
- **Handler updates**: ~89 files (mostly mechanical signature change)
- **Doc updates**: ~60 references in .md files

---

TAS-96: Ruby Worker API Alignment - Implementation Plan

**Ticket**: [TAS-96](https://linear.app/tasker-systems/issue/TAS-96)
**Branch**: `jcoletaylor/tas-96-ruby-worker-api-alignment`
**Parent**: TAS-92 (Consistent Developer-space APIs and Ergonomics)

## Objective

Align Ruby worker APIs with cross-language standards (Python/Rust). Primary change: handler signature migration from `call(task, sequence, step)` to `call(context)`.

## Key Discovery

**`TaskSequenceStepWrapper` already exists** in `workers/ruby/lib/tasker_core/models.rb` as the unified context containing all step data. The current subscriber (`subscriber.rb:53-57`) passes components separately:
```ruby
result = handler.call(
  step_data.task,              # TaskWrapper
  step_data.dependency_results, # DependencyResultsWrapper
  step_data.workflow_step       # WorkflowStepWrapper
)
```

We'll create `StepContext` wrapping `step_data` and pass it as a single argument.

---

## Implementation Phases

### Phase 1: Create StepContext Type (~100 lines)

**New File**: `workers/ruby/lib/tasker_core/types/step_context.rb`

Cross-language standard fields (matching Python):
| Field | Source | Description |
|-------|--------|-------------|
| `task_uuid` | `task.task_uuid` | Task UUID |
| `step_uuid` | `workflow_step.workflow_step_uuid` | Step UUID |
| `input_data` | `workflow_step.inputs` | Step inputs |
| `step_config` | `step_definition.handler.initialization` | Handler config |
| `step_inputs` | Alias for `input_data` | Cross-language alias |
| `retry_count` | `workflow_step.attempts` | Current retry |
| `max_retries` | `workflow_step.max_attempts` | Max retries |
| `dependency_results` | Delegated | Parent step results |

Ruby-specific accessors (backward compat):
- `task`, `workflow_step`, `step_definition` - Direct wrapper access
- `get_task_field(name)`, `get_dependency_result(step_name)` - Convenience methods

### Phase 2: Create ErrorTypes Module (~25 lines)

**New File**: `workers/ruby/lib/tasker_core/types/error_types.rb`

Constants: `PERMANENT_ERROR`, `RETRYABLE_ERROR`, `VALIDATION_ERROR`, `TIMEOUT`, `HANDLER_ERROR`

### Phase 3: Update Base Handler Signature

**File**: `workers/ruby/lib/tasker_core/step_handler/base.rb`

- Line 30: Change `def call(task, sequence, step)` → `def call(context)`
- Update docstrings and examples

### Phase 4: Update FFI Dispatch Layer

**File**: `workers/ruby/lib/tasker_core/subscriber.rb`

Lines 53-57, change from:
```ruby
result = handler.call(
  step_data.task,
  step_data.dependency_results,
  step_data.workflow_step
)
```
To:
```ruby
context = TaskerCore::Types::StepContext.new(step_data)
result = handler.call(context)
```

### Phase 5: Add Registry Aliases

**File**: `workers/ruby/lib/tasker_core/registry/handler_registry.rb`

Add aliases after existing methods:
```ruby
alias register register_handler
alias is_registered handler_available?
alias list_handlers registered_handlers
alias resolve resolve_handler
```

### Phase 6: Update Batchable Handler

**File**: `workers/ruby/lib/tasker_core/step_handler/batchable.rb`

- Add `get_batch_context(context)` that extracts from `context.workflow_step`
- Add `batch_worker_success(...)` as alias/wrapper for success with batch fields
- Keep existing methods as aliases for backward compat

### Phase 7: Add Domain Events `publish(ctx)` Method

**File**: `workers/ruby/lib/tasker_core/domain_events/base_publisher.rb`

Add new method:
```ruby
def publish(ctx)
  # Wrapper coordinating: should_publish?, transform_payload, additional_metadata
  # before_publish/after_publish hooks
end
```

### Phase 8: Update Example Handlers (80 files)

**Location**: `workers/ruby/spec/handlers/examples/**/*_handler.rb`

Mechanical signature change from:
```ruby
def call(task, sequence, step)       # or
def call(task, _sequence, _step)     # or
def call(_task, sequence, _step)
```
To:
```ruby
def call(context)
```

Migration patterns:
- `task.context['field']` → `context.get_task_field('field')`
- `sequence.get_results('step')` → `context.get_dependency_result('step')`
- `step.name` → `context.workflow_step.name`

### Phase 9: Update Tests

Update test fixtures to use context-based API:
- `spec/step_handler/*.rb`
- `spec/types/*.rb`
- `spec/batch_processing/*.rb`
- `spec/domain_events/*.rb`
- `spec/ffi/*.rb`

---

## Critical Files

| File | Change Type | Lines |
|------|-------------|-------|
| `lib/tasker_core/types/step_context.rb` | New | ~100 |
| `lib/tasker_core/types/error_types.rb` | New | ~25 |
| `lib/tasker_core/step_handler/base.rb` | Modify | ~30 |
| `lib/tasker_core/subscriber.rb` | Modify | ~15 |
| `lib/tasker_core/registry/handler_registry.rb` | Modify | ~10 |
| `lib/tasker_core/step_handler/batchable.rb` | Modify | ~40 |
| `lib/tasker_core/domain_events/base_publisher.rb` | Modify | ~30 |
| `spec/handlers/examples/**/*.rb` (80 files) | Modify | ~160 total |

---

## Verification

After each phase, run:
```bash
cd workers/ruby
bundle exec rspec
```

After Phase 4 (FFI changes), run integration tests:
```bash
DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test \
TASKER_ENV=test bundle exec rspec spec/integration/
```

---

## Checklist

- [ ] StepContext class with cross-language standard fields
- [ ] StepContext convenience methods (get_task_field, get_dependency_result)
- [ ] ErrorTypes module with constants
- [ ] Base#call(context) signature
- [ ] FFI dispatch creates StepContext
- [ ] Registry aliases: register, is_registered, resolve, list_handlers
- [ ] Batchable: get_batch_context, batch_worker_success
- [ ] BasePublisher: publish(ctx) method
- [ ] All 80 example handlers updated
- [ ] All tests pass
