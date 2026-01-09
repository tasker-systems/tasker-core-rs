# API Convergence Matrix

**Last Updated**: 2026-01-08
**Status**: Active
**Related Tickets**: TAS-92, TAS-93, TAS-95, TAS-96, TAS-97, TAS-98, TAS-112, TAS-125

<- Back to [Worker Crates Overview](README.md)

---

## Overview

This document provides a quick reference for the aligned APIs across Ruby, Python, TypeScript, and Rust worker implementations. All four languages share consistent patterns for handler execution, result creation, registry operations, and composition via mixins/traits.

**TAS-112 Updates (2026-01-01)**:
- Added TypeScript to all cross-language tables
- Added composition pattern (mixin/trait) documentation
- Added lifecycle hooks for publishers and subscribers
- Updated domain events section with cross-language parity

**TAS-93 Updates (2026-01-08)**:
- Added Resolver Chain API section with cross-language parity
- Documented StepHandlerResolver interface for custom resolvers
- Added HandlerDefinition fields and method dispatch patterns
- Links to new Handler Resolution Guide

---

## Handler Signatures

| Language | Base Class | Signature |
|----------|------------|-----------|
| Ruby | `TaskerCore::StepHandler::Base` | `def call(context)` |
| Python | `BaseStepHandler` | `def call(self, context: StepContext) -> StepHandlerResult` |
| TypeScript | `StepHandler` | `async call(context: StepContext): Promise<StepHandlerResult>` |
| Rust | `StepHandler` trait | `async fn call(&self, step_data: &TaskSequenceStep) -> StepExecutionResult` |

---

## Composition Pattern (TAS-112)

All languages use composition via mixins/traits rather than inheritance hierarchies.

### Handler Composition

| Language | Base | Mixin Syntax | Example |
|----------|------|--------------|---------|
| Ruby | `StepHandler::Base` | `include Mixins::API` | `class Handler < Base; include Mixins::API` |
| Python | `StepHandler` | Multiple inheritance | `class Handler(StepHandler, APIMixin)` |
| TypeScript | `StepHandler` | `applyAPI(this)` | Mixin functions applied in constructor |
| Rust | `impl StepHandler` | `impl APICapable` | Multiple trait implementations |

### Available Mixins/Traits

| Capability | Ruby | Python | TypeScript | Rust |
|------------|------|--------|------------|------|
| API | `Mixins::API` | `APIMixin` | `applyAPI()` | `APICapable` |
| Decision | `Mixins::Decision` | `DecisionMixin` | `applyDecision()` | `DecisionCapable` |
| Batchable | `Mixins::Batchable` | `BatchableMixin` | `BatchableHandler` | `BatchableCapable` |

---

## StepContext Fields

The `StepContext` provides unified access to step execution data across Ruby, Python, and TypeScript.

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

## Resolver Chain API (TAS-93)

Handler resolution uses a chain-of-responsibility pattern to convert callable addresses into executable handlers.

### StepHandlerResolver Interface

| Method | Ruby | Python | TypeScript | Rust |
|--------|------|--------|------------|------|
| Get Name | `name` | `resolver_name()` | `resolverName()` | `resolver_name(&self)` |
| Get Priority | `priority` | `priority()` | `priority()` | `priority(&self)` |
| Can Resolve? | `can_resolve?(definition, config)` | `can_resolve(definition)` | `canResolve(definition)` | `can_resolve(&self, definition)` |
| Resolve | `resolve(definition, config)` | `resolve(definition, context)` | `resolve(definition, context)` | `resolve(&self, definition, context)` |

### ResolverChain Operations

| Operation | Ruby | Python | TypeScript | Rust |
|-----------|------|--------|------------|------|
| Create | `ResolverChain.new` | `ResolverChain()` | `new ResolverChain()` | `ResolverChain::new()` |
| Register | `register(resolver)` | `register(resolver)` | `register(resolver)` | `register(resolver)` |
| Resolve | `resolve(definition, context)` | `resolve(definition, context)` | `resolve(definition, context)` | `resolve(definition, context)` |
| Can Resolve? | `can_resolve?(definition)` | `can_resolve(definition)` | `canResolve(definition)` | `can_resolve(definition)` |
| List | `resolvers` | `resolvers` | `resolvers` | `resolvers()` |

### Built-in Resolvers

| Resolver | Priority | Function | Rust | Ruby | Python | TypeScript |
|----------|----------|----------|------|------|--------|------------|
| ExplicitMappingResolver | 10 | Hash lookup of registered handlers | ✅ | ✅ | ✅ | ✅ |
| ClassConstantResolver | 100 | Runtime class lookup (Ruby) | ❌ | ✅ | - | - |
| ClassLookupResolver | 100 | Runtime class lookup (Python/TS) | ❌ | - | ✅ | ✅ |

**Note**: Class lookup resolvers are not available in Rust due to lack of runtime reflection. Rust handlers must use ExplicitMappingResolver. Ruby uses `ClassConstantResolver` (Ruby terminology); Python and TypeScript use `ClassLookupResolver` (same functionality, language-appropriate naming).

### HandlerDefinition Fields

| Field | Type | Description | Required |
|-------|------|-------------|----------|
| `callable` | String | Handler address (name or class path) | Yes |
| `method` | String | Entry point method (default: `"call"`) | No |
| `resolver` | String | Resolution hint to bypass chain | No |
| `initialization` | Dict/Hash | Handler configuration | No |

### Method Dispatch

Multi-method handlers expose multiple entry points through the `method` field:

| Language | Default Method | Dynamic Dispatch |
|----------|---------------|------------------|
| Ruby | `call` | `handler.public_send(method, context)` |
| Python | `call` | `getattr(handler, method)(context)` |
| TypeScript | `call` | `handler[method](context)` |
| Rust | `call` | `handler.invoke_method(method, step)` |

**Creating Multi-Method Handlers:**

| Language | Signature |
|----------|-----------|
| Ruby | Define additional methods alongside `call` |
| Python | Define additional methods alongside `call` |
| TypeScript | Define additional async methods alongside `call` |
| Rust | Implement `invoke_method` to dispatch to internal methods |

See [Handler Resolution Guide](../guides/handler-resolution.md) for complete documentation.

---

## Specialized Handlers

### API Handler

| Operation | Ruby | Python | TypeScript |
|-----------|------|--------|------------|
| GET | `get(path, params: {}, headers: {})` | `self.get(path, params={}, headers={})` | `this.get(path, params?, headers?)` |
| POST | `post(path, data: {}, headers: {})` | `self.post(path, data={}, headers={})` | `this.post(path, data?, headers?)` |
| PUT | `put(path, data: {}, headers: {})` | `self.put(path, data={}, headers={})` | `this.put(path, data?, headers?)` |
| DELETE | `delete(path, params: {}, headers: {})` | `self.delete(path, params={}, headers={})` | `this.delete(path, params?, headers?)` |

### Decision Handler

| Language | Simple API | Result Fields |
|----------|------------|---------------|
| Ruby | `decision_success(steps:, routing_context:)` | `decision_point_outcome: { type, step_names }` |
| Python | `decision_success(steps, routing_context)` | `decision_point_outcome: { type, step_names }` |
| TypeScript | `decisionSuccess(steps, routingContext?)` | `decision_point_outcome: { type, step_names }` |
| Rust | `decision_success(step_uuid, step_names, ...)` | Pattern-based |

**Decision Helper Methods (Cross-Language):**
- `decision_success(steps, routing_context)` - Create dynamic steps
- `skip_branches(reason, routing_context)` - Skip all conditional branches
- `decision_failure(message, error_type)` - Decision could not be made

### Batchable Handler

| Operation | Ruby | Python | TypeScript |
|-----------|------|--------|------------|
| Get Context | `get_batch_context(context)` | `get_batch_context(context)` | `getBatchContext(context)` |
| Complete Batch | `batch_worker_complete(processed_count:, result_data:)` | `batch_worker_complete(processed_count, result_data)` | `batchWorkerComplete(processedCount, resultData)` |
| Handle No-Op | `handle_no_op_worker(batch_ctx)` | `handle_no_op_worker(batch_ctx)` | `handleNoOpWorker(batchCtx)` |

**Standard Batch Result Fields:**
- `processed_count` / `items_processed`
- `items_succeeded` / `items_failed`
- `start_cursor`, `end_cursor`, `batch_size`, `last_cursor`

**Cursor Indexing (TAS-112):**
- All languages use **0-indexed cursors** (start at 0, not 1)
- Ruby was updated from 1-indexed to 0-indexed for consistency

### Checkpoint Yielding (TAS-125)

Checkpoint yielding enables batch workers to persist progress and yield control for re-dispatch.

| Operation | Ruby | Python | TypeScript |
|-----------|------|--------|------------|
| Checkpoint | `checkpoint_yield(cursor:, items_processed:, accumulated_results:)` | `checkpoint_yield(cursor, items_processed, accumulated_results)` | `checkpointYield({ cursor, itemsProcessed, accumulatedResults })` |

**BatchWorkerContext Checkpoint Accessors:**

| Accessor | Ruby | Python | TypeScript |
|----------|------|--------|------------|
| Cursor | `checkpoint_cursor` | `checkpoint_cursor` | `checkpointCursor` |
| Accumulated Results | `accumulated_results` | `accumulated_results` | `accumulatedResults` |
| Has Checkpoint? | `has_checkpoint?` | `has_checkpoint()` | `hasCheckpoint()` |
| Items Processed | `checkpoint_items_processed` | `checkpoint_items_processed` | `checkpointItemsProcessed` |

**FFI Contract:**

| Function | Description |
|----------|-------------|
| `checkpoint_yield_step_event(event_id, data)` | Persist checkpoint and re-dispatch step |

**Key Invariants:**
- Progress is atomically saved before re-dispatch
- Step remains `InProgress` during checkpoint yield cycle
- Only `Success`/`Failure` trigger state transitions

See [Batch Processing Guide - Checkpoint Yielding](../guides/batch-processing.md#checkpoint-yielding-tas-125) for full documentation.

---

## Domain Events

### Publisher Contract

| Language | Base Class | Key Method |
|----------|------------|------------|
| Ruby | `TaskerCore::DomainEvents::BasePublisher` | `publish(ctx)` |
| Python | `BasePublisher` | `publish(ctx)` |
| TypeScript | `BasePublisher` | `publish(ctx)` |
| Rust | `StepEventPublisher` trait | `publish(ctx)` |

### Publisher Lifecycle Hooks (TAS-112)

All languages support publisher lifecycle hooks for instrumentation:

| Hook | Ruby | Python | TypeScript | Description |
|------|------|--------|------------|-------------|
| Before Publish | `before_publish(ctx)` | `before_publish(ctx)` | `beforePublish(ctx)` | Called before publishing |
| After Publish | `after_publish(ctx, result)` | `after_publish(ctx, result)` | `afterPublish(ctx, result)` | Called after successful publish |
| On Error | `on_publish_error(ctx, error)` | `on_publish_error(ctx, error)` | `onPublishError(ctx, error)` | Called on publish failure |
| Metadata | `additional_metadata(ctx)` | `additional_metadata(ctx)` | `additionalMetadata(ctx)` | Inject custom metadata |

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
| TypeScript | `BaseSubscriber` | `subscribesTo()`, `handle(event)` |
| Rust | EventHandler closures | N/A |

### Subscriber Lifecycle Hooks (TAS-112)

All languages support subscriber lifecycle hooks:

| Hook | Ruby | Python | TypeScript | Description |
|------|------|--------|------------|-------------|
| Before Handle | `before_handle(event)` | `before_handle(event)` | `beforeHandle(event)` | Called before handling |
| After Handle | `after_handle(event, result)` | `after_handle(event, result)` | `afterHandle(event, result)` | Called after handling |
| On Error | `on_handle_error(event, error)` | `on_handle_error(event, error)` | `onHandleError(event, error)` | Called on handler failure |

### Registries

| Language | Publisher Registry | Subscriber Registry |
|----------|-------------------|---------------------|
| Ruby | `PublisherRegistry.instance` | `SubscriberRegistry.instance` |
| Python | `PublisherRegistry.instance()` | `SubscriberRegistry.instance()` |
| TypeScript | `PublisherRegistry.getInstance()` | `SubscriberRegistry.getInstance()` |

---

## Migration Summary

### Ruby (TAS-96, TAS-112)

| Before | After |
|--------|-------|
| `def call(task, sequence, step)` | `def call(context)` |
| `class Handler < API` | `class Handler < Base; include Mixins::API` |
| `task.context['field']` | `context.get_task_field('field')` |
| `sequence.get_results('step')` | `context.get_dependency_result('step')` |
| 1-indexed cursors | 0-indexed cursors |

### Python (TAS-95, TAS-112)

| Before | After |
|--------|-------|
| `def handle(self, task, sequence, step)` | `def call(self, context)` |
| `class Handler(APIHandler)` | `class Handler(StepHandler, APIMixin)` |
| N/A | `self.success(result, metadata)` |
| N/A | Publisher/Subscriber lifecycle hooks |

### TypeScript (TAS-112)

| Before | After |
|--------|-------|
| `class Handler extends APIHandler` | `class Handler extends StepHandler implements APICapable` |
| No domain events | Complete domain events module |
| N/A | Publisher/Subscriber lifecycle hooks |
| N/A | `applyAPI(this)`, `applyDecision(this)` mixins |

### Rust (TAS-97, TAS-112)

| Before | After |
|--------|-------|
| (already aligned) | (already aligned) |
| N/A | `APICapable`, `DecisionCapable`, `BatchableCapable` traits |

---

## See Also

- [Example Handlers](example-handlers.md) - Side-by-side code examples
- [Patterns and Practices](patterns-and-practices.md) - Common patterns
- [Ruby Worker](ruby.md) - Ruby implementation details
- [Python Worker](python.md) - Python implementation details
- [TypeScript Worker](typescript.md) - TypeScript implementation details
- [Rust Worker](rust.md) - Rust implementation details
- [Composition Over Inheritance](../principles/composition-over-inheritance.md) - Why mixins over inheritance
- [FFI Boundary Types](../reference/ffi-boundary-types.md) - Cross-language type alignment
- [Handler Resolution Guide](../guides/handler-resolution.md) - Custom resolver strategies (TAS-93)
