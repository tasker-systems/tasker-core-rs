# API Convergence Matrix

**Last Updated**: 2025-12-21
**Status**: Active
**Related Tickets**: TAS-92, TAS-95, TAS-96, TAS-97, TAS-98

<- Back to [Worker Crates Overview](README.md)

---

## Overview

This document provides a quick reference for the aligned APIs across Ruby, Python, and Rust worker implementations. All three languages now share consistent patterns for handler execution, result creation, and registry operations.

---

## Handler Signatures

| Language | Base Class | Signature |
|----------|------------|-----------|
| Ruby | `TaskerCore::StepHandler::Base` | `def call(context)` |
| Python | `BaseStepHandler` | `def call(self, context: StepContext) -> StepHandlerResult` |
| Rust | `StepHandler` trait | `async fn call(&self, step_data: &TaskSequenceStep) -> StepExecutionResult` |

---

## StepContext Fields (Ruby/Python)

The `StepContext` provides unified access to step execution data across Ruby and Python.

| Field | Type | Description |
|-------|------|-------------|
| `task_uuid` | String | Unique task identifier (UUID v7) |
| `step_uuid` | String | Unique step identifier (UUID v7) |
| `input_data` | Dict/Hash | Input data for the step from `workflow_step.inputs` |
| `step_inputs` | Dict/Hash | Alias for `input_data` |
| `step_config` | Dict/Hash | Handler configuration from `step_definition.handler.initialization` |
| `dependency_results` | Wrapper | Results from parent steps (DependencyResultsWrapper) |
| `retry_count` | Integer | Current retry attempt (from `workflow_step.attempts`) |
| `max_retries` | Integer | Maximum retry attempts (from `workflow_step.max_attempts`) |

### Convenience Methods

| Method | Description |
|--------|-------------|
| `get_task_field(name)` | Get field from task context |
| `get_dependency_result(step_name)` | Get result from a parent step |

### Ruby-Specific Accessors

| Property | Type | Description |
|----------|------|-------------|
| `task` | TaskWrapper | Full task wrapper with context and metadata |
| `workflow_step` | WorkflowStepWrapper | Workflow step with execution state |
| `step_definition` | StepDefinitionWrapper | Step definition from task template |

---

## Result Factories

### Success Results

| Language | Method | Example |
|----------|--------|---------|
| Ruby | `success(result:, metadata:)` | `success(result: { id: 123 }, metadata: { ms: 50 })` |
| Python | `self.success(result, metadata)` | `self.success({"id": 123}, {"ms": 50})` |
| Rust | `StepExecutionResult::success(...)` | `StepExecutionResult::success(result, metadata)` |

### Failure Results

| Language | Method | Key Parameters |
|----------|--------|----------------|
| Ruby | `failure(message:, error_type:, error_code:, retryable:, metadata:)` | keyword arguments |
| Python | `self.failure(message, error_type, error_code, retryable, metadata)` | positional/keyword |
| Rust | `StepExecutionResult::failure(...)` | structured fields |

---

## Result Fields

| Field | Ruby | Python | Rust | Description |
|-------|------|--------|------|-------------|
| success | bool | bool | bool | Whether step succeeded |
| result | Hash | Dict | HashMap | Result data |
| metadata | Hash | Dict | HashMap | Additional context |
| error_message | String | str | String | Human-readable error |
| error_type | String | str | String | Error classification |
| error_code | String (optional) | str (optional) | String (optional) | Application error code |
| retryable | bool | bool | bool | Whether to retry |

---

## Standard error_type Values

Use these standard values for consistent error classification:

| Value | Description | Retry Behavior |
|-------|-------------|----------------|
| `PermanentError` | Non-recoverable failure | Never retry |
| `RetryableError` | Temporary failure | Will retry |
| `ValidationError` | Input validation failed | No retry |
| `TimeoutError` | Operation timed out | May retry |
| `UnexpectedError` | Unexpected handler error | May retry |

---

## Registry API

| Operation | Ruby | Python | Rust |
|-----------|------|--------|------|
| Register | `register(name, klass)` | `register(name, klass)` | `register_handler(name, handler)` |
| Check | `is_registered(name)` | `is_registered(name)` | `is_registered(name)` |
| Resolve | `resolve(name)` | `resolve(name)` | `get_handler(name)` |
| List | `list_handlers` | `list_handlers()` | `list_handlers()` |

**Note**: Ruby also provides original method names (`register_handler`, `handler_available?`, `resolve_handler`, `registered_handlers`) as the primary API with the above as cross-language aliases.

---

## Specialized Handlers

### API Handler

| Operation | Ruby | Python |
|-----------|------|--------|
| GET | `get(path, params: {}, headers: {})` | `self.get(path, params={}, headers={})` |
| POST | `post(path, data: {}, headers: {})` | `self.post(path, data={}, headers={})` |
| PUT | `put(path, data: {}, headers: {})` | `self.put(path, data={}, headers={})` |
| DELETE | `delete(path, params: {}, headers: {})` | `self.delete(path, params={}, headers={})` |

### Decision Handler

| Language | Simple API | Result Fields |
|----------|------------|---------------|
| Ruby | `decision_success(steps:, result_data:)` | `decision_point_outcome: { type, step_names }` |
| Python | `decision_success(steps, routing_context)` | `decision_point_outcome: { type, step_names }` |
| Rust | Result with `activated_steps` field | Pattern-based |

**Decision Helper Methods:**
- `decision_success(steps:, result_data:)` - Create dynamic steps
- `decision_no_branches(result_data:)` - Skip conditional steps

### Batchable Handler

| Operation | Ruby | Python |
|-----------|------|--------|
| Get Context | `get_batch_context(context)` | `get_batch_context(context)` |
| Complete Batch | `batch_worker_complete(processed_count:, result_data:)` | `batch_worker_complete(processed_count, result_data)` |
| Handle No-Op | `handle_no_op_worker(batch_ctx)` | `handle_no_op_worker(batch_ctx)` |

**Standard Batch Result Fields:**
- `processed_count` / `items_processed`
- `items_succeeded` / `items_failed`
- `start_cursor`, `end_cursor`, `batch_size`, `last_cursor`

---

## Domain Events

### Publisher Contract

| Language | Base Class | Key Method |
|----------|------------|------------|
| Ruby | `TaskerCore::DomainEvents::BasePublisher` | `publish(ctx)` |
| Python | `BasePublisher` | `publish(ctx)` |
| Rust | `StepEventPublisher` trait | `publish(ctx)` |

### StepEventContext Fields

| Field | Description |
|-------|-------------|
| `task_uuid` | Task identifier |
| `step_uuid` | Step identifier |
| `step_name` | Handler/step name |
| `namespace` | Task namespace |
| `correlation_id` | Tracing correlation ID |
| `result` | Step execution result |
| `metadata` | Additional metadata |

### Subscriber Contract

| Language | Base Class | Key Methods |
|----------|------------|-------------|
| Ruby | `TaskerCore::DomainEvents::BaseSubscriber` | `subscribes_to`, `handle(event)` |
| Python | `BaseSubscriber` | `subscribes_to()`, `handle(event)` |
| Rust | EventHandler closures | N/A |

---

## Migration Summary

### Ruby (TAS-96)

| Before | After |
|--------|-------|
| `def call(task, sequence, step)` | `def call(context)` |
| `task.context['field']` | `context.get_task_field('field')` |
| `sequence.get_results('step')` | `context.get_dependency_result('step')` |
| `step.results` | `context.workflow_step.results` |

### Python (TAS-95)

| Before | After |
|--------|-------|
| `def handle(self, task, sequence, step)` | `def call(self, context)` |
| N/A | `self.success(result, metadata)` |
| N/A | `self.failure(message, error_type, ...)` |

### Rust (TAS-97)

| Before | After |
|--------|-------|
| (already aligned) | (already aligned) |

---

## See Also

- [Example Handlers](example-handlers.md) - Side-by-side code examples
- [Patterns and Practices](patterns-and-practices.md) - Common patterns
- [Ruby Worker](ruby.md) - Ruby implementation details
- [Python Worker](python.md) - Python implementation details
- [Rust Worker](rust.md) - Rust implementation details
