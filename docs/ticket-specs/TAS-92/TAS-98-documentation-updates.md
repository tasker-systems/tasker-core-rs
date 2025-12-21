# TAS-98: Cross-Language Worker API Documentation Updates

**Parent**: [TAS-92](./README.md)
**Linear**: [TAS-98](https://linear.app/tasker-systems/issue/TAS-98)
**Branch**: `jcoletaylor/tas-98-cross-language-worker-api-documentation-updates`
**Priority**: Medium

## Objective

Update all documentation to reflect the aligned APIs across Ruby, Python, and Rust workers. This ticket should be completed AFTER TAS-95, TAS-96, and TAS-97 are merged.

## Prerequisites

- [ ] TAS-95 (Python Worker API Alignment) merged
- [ ] TAS-96 (Ruby Worker API Alignment) merged
- [ ] TAS-97 (Rust Worker API Alignment) merged

## Scope Overview

| Document | Change Type | Effort |
|----------|-------------|--------|
| API Convergence Matrix | Create new | Medium |
| Example Handlers Reference | Create new | Medium |
| patterns-and-practices.md | Major update | High |
| ruby.md | Update examples | Medium |
| python.md | Update examples | Medium |
| rust.md | Update examples | Low |
| domain-events.md | Update examples | Medium |
| README.md | Update overview | Low |

## Implementation Plan

### Phase 1: Create API Convergence Matrix

**New file:** `docs/worker-crates/api-convergence-matrix.md`

**Structure:**
```markdown
# API Convergence Matrix

## Overview

This document provides a quick reference for the aligned APIs across Ruby, Python,
and Rust worker implementations.

## Handler Signatures

| Language | Signature | Context Type |
|----------|-----------|--------------|
| Ruby | `call(context)` | `StepContext` |
| Python | `call(self, context)` | `StepContext` |
| Rust | `async fn call(&self, step_data: &TaskSequenceStep)` | `TaskSequenceStep` |

### StepContext Fields (Ruby/Python)

| Field | Type | Description |
|-------|------|-------------|
| `task_uuid` | String | Unique task identifier |
| `step_uuid` | String | Unique step identifier |
| `input_data` | Dict/Hash | Input data for the step |
| `dependency_results` | Dict/Hash | Results from dependent steps |
| `step_config` | Dict/Hash | Handler configuration |
| `step_inputs` | Dict/Hash | Step-specific inputs |
| `retry_count` | Integer | Current retry attempt |
| `max_retries` | Integer | Maximum retry attempts |

## Result Factories

| Language | Success | Failure |
|----------|---------|---------|
| Ruby | `success(result:, metadata:)` | `failure(message:, error_type:, ...)` |
| Python | `self.success(result, metadata)` | `self.failure(message, error_type, ...)` |
| Rust | `StepExecutionResult::success(...)` | `StepExecutionResult::failure(...)` |

### Result Fields

| Field | Ruby | Python | Rust |
|-------|------|--------|------|
| success | bool | bool | bool |
| result | Hash | Dict | HashMap |
| metadata | Hash | Dict | HashMap |
| error_message | String | str | String |
| error_type | String | str | String |
| error_code | String (optional) | str (optional) | metadata |
| retryable | bool | bool | bool |

### Standard error_type Values

- `permanent_error` - Non-recoverable failure
- `retryable_error` - Temporary failure, can retry
- `validation_error` - Input validation failed
- `timeout` - Operation timed out
- `handler_error` - Handler execution error

## Registry API

| Operation | Ruby | Python | Rust |
|-----------|------|--------|------|
| Register | `register(name, klass)` | `register(name, klass)` | `register_handler(name, handler)` |
| Check | `is_registered(name)` | `is_registered(name)` | `is_registered(name)` |
| Resolve | `resolve(name)` | `resolve(name)` | `get_handler(name)` |
| List | `list_handlers` | `list_handlers()` | `list_handlers()` |

## Specialized Handlers

### API Handler

| Operation | Ruby | Python | Rust |
|-----------|------|--------|------|
| GET | `get(path, params:, headers:)` | `self.get(path, params, headers)` | Pattern-based |
| POST | `post(path, data:, headers:)` | `self.post(path, data, headers)` | Pattern-based |
| PUT | `put(path, data:, headers:)` | `self.put(path, data, headers)` | Pattern-based |
| DELETE | `delete(path, params:, headers:)` | `self.delete(path, params, headers)` | Pattern-based |

### Decision Handler

| Language | Simple API | Full API |
|----------|------------|----------|
| Ruby | `decision_success(steps:, result_data:)` | `decision_success_with_outcome(outcome)` |
| Python | `decision_success(steps, routing_context)` | `decision_success_with_outcome(outcome)` |
| Rust | Result with `activated_steps` field | N/A |

### Batchable Handler

| Operation | Ruby | Python |
|-----------|------|--------|
| Success | `batch_worker_success(...)` | `batch_worker_success(...)` |
| Get Context | `get_batch_context(context)` | `get_batch_context(context)` |

Standard batch result fields:
- `items_processed`
- `items_succeeded`
- `items_failed`
- `start_cursor`, `end_cursor`, `batch_size`, `last_cursor`

## Domain Events

### Publisher Contract

| Language | Base Class | Key Method |
|----------|------------|------------|
| Ruby | `BasePublisher` | `publish(ctx)` |
| Python | `BasePublisher` | `publish(ctx)` |
| Rust | `StepEventPublisher` trait | `publish(ctx)` |

### StepEventContext Fields

| Field | Description |
|-------|-------------|
| `task_uuid` | Task identifier |
| `step_uuid` | Step identifier |
| `step_name` | Handler name |
| `namespace` | Task namespace |
| `correlation_id` | Tracing correlation ID |
| `result` | Step execution result |
| `metadata` | Additional metadata |

### Subscriber Contract

| Language | Base Class | Key Methods |
|----------|------------|-------------|
| Ruby | `BaseSubscriber` | `subscribes_to`, `handle(event)` |
| Python | `BaseSubscriber` | `subscribes_to()`, `handle(event)` |
| Rust | EventHandler closures | N/A |
```

### Phase 2: Create Example Handlers Reference

**New file:** `docs/worker-crates/example-handlers.md`

**Structure:**
```markdown
# Example Handlers - Cross-Language Reference

## Simple Step Handler

### Ruby
\`\`\`ruby
class MyHandler < TaskerCore::StepHandler::Base
  def call(context)
    result = process_data(context.input_data)
    success(result: result, metadata: { processed_at: Time.now.iso8601 })
  rescue StandardError => e
    failure(
      message: e.message,
      error_type: ErrorTypes::HANDLER_ERROR,
      retryable: true
    )
  end
end
\`\`\`

### Python
\`\`\`python
class MyHandler(BaseStepHandler):
    def call(self, context: StepContext) -> StepHandlerResult:
        try:
            result = self.process_data(context.input_data)
            return self.success(result, {"processed_at": datetime.now().isoformat()})
        except Exception as e:
            return self.failure(
                message=str(e),
                error_type="handler_error",
                retryable=True
            )
\`\`\`

### Rust
\`\`\`rust
impl StepHandler for MyHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> StepExecutionResult {
        match self.process_data(&step_data.inputs).await {
            Ok(result) => StepExecutionResult::success(result, None),
            Err(e) => StepExecutionResult::failure(
                &e.to_string(),
                error_types::HANDLER_ERROR,
                true, // retryable
            ),
        }
    }
}
\`\`\`

[Additional examples for API, Decision, Batch handlers...]
```

### Phase 3: Update patterns-and-practices.md

**File:** `docs/worker-crates/patterns-and-practices.md`

**Changes:**
1. Replace "Current Inconsistencies" section with "API Alignment Status"
2. Update all handler signature examples
3. Update result factory examples to use `success()` / `failure()`
4. Update registry API examples
5. Add error_type recommendations
6. Update specialized handler sections

### Phase 4: Update Language-Specific Docs

#### ruby.md
- Update handler signature to `call(context)`
- Update registry API examples
- Update API handler with `get/post/put/delete`
- Update batchable with `batch_worker_success`
- Add domain events `publish(ctx)` example

#### python.md
- Update result factories to `success()` / `failure()`
- Add `error_code` field examples
- Update decision handler with simple helper
- Add `BasePublisher` and `BaseSubscriber` examples

#### rust.md
- Add `with_error_code()` helper example
- Add `is_registered()` and `list_handlers()` examples
- Reference pattern documentation
- Verify `StepEventPublisher` documentation

### Phase 5: Update domain-events.md

**Changes:**
- Add Python `BasePublisher` examples
- Add Python `BaseSubscriber` examples
- Update Ruby `publish(ctx)` method documentation
- Ensure `StepEventContext` documented consistently
- Add cross-language comparison table

### Phase 6: Update README.md

**File:** `docs/worker-crates/README.md`

**Changes:**
- Update handler registration examples
- Update signature table (all show `call(context)`)
- Update result factory examples
- Add link to convergence matrix
- Add link to example handlers reference

## Files Summary

### New Files
| File | Purpose |
|------|---------|
| `docs/worker-crates/api-convergence-matrix.md` | Quick reference for aligned APIs |
| `docs/worker-crates/example-handlers.md` | Side-by-side handler examples |

### Updated Files
| File | Effort |
|------|--------|
| `docs/worker-crates/README.md` | Low |
| `docs/worker-crates/patterns-and-practices.md` | High |
| `docs/worker-crates/ruby.md` | Medium |
| `docs/worker-crates/python.md` | Medium |
| `docs/worker-crates/rust.md` | Low |
| `docs/domain-events.md` | Medium |

## Verification Checklist

### Code Accuracy
- [ ] All Ruby examples use `call(context)` signature
- [ ] All Python examples use `success()` / `failure()`
- [ ] All registry examples use aligned method names
- [ ] All error_type examples use standard values
- [ ] All specialized handler examples aligned

### Cross-References
- [ ] All links between docs work
- [ ] No broken internal references
- [ ] Convergence matrix linked from relevant docs

### Completeness
- [ ] No references to old API names
- [ ] All three languages covered in each section
- [ ] Example handlers cover all patterns

### Build Verification
- [ ] Documentation builds without warnings
- [ ] All code examples are syntactically correct
- [ ] Examples can be copy-pasted and work

## Validation Process

After completing documentation updates:

1. **Build docs locally** - Verify no build errors
2. **Review each example** - Ensure code matches implementation
3. **Cross-reference check** - Verify all links work
4. **Copy-paste test** - Try examples in each language
5. **Peer review** - Have someone unfamiliar review for clarity

## Estimated Scope

- **New content**: ~500 lines (convergence matrix, examples)
- **Updated content**: ~400 lines (existing docs)
- **Total effort**: Medium-High
