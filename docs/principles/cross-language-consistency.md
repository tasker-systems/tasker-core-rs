# Cross-Language Consistency

This document describes Tasker Core's philosophy for maintaining consistent APIs across Rust, Ruby, Python, and TypeScript workers while respecting each language's idioms.

## The Core Philosophy

> "There should be one--and preferably only one--obvious way to do it."
> -- The Zen of Python

When a developer learns one Tasker worker language, they should understand all of them at the conceptual level. The specific syntax changes; the patterns remain constant.

---

## Consistency Without Uniformity

### What We Align

**Developer-facing touchpoints** that affect daily work:

| Touchpoint | Why Align |
|------------|-----------|
| Handler signatures | Developers switch languages within projects |
| Result factories | Error handling should feel familiar |
| Registry APIs | Service configuration is cross-cutting |
| Context access patterns | Data access is the core operation |
| Specialized handlers | API, Decision, Batchable are reusable patterns |

### What We Don't Force

**Language idioms** that feel natural in their ecosystem:

| Ruby | Python | TypeScript | Rust |
|------|--------|------------|------|
| Blocks, `yield` | Decorators, context managers | Generics, interfaces | Traits, associated types |
| Symbols (`:name`) | Type hints | `async/await` | Pattern matching |
| Duck typing | ABC, Protocol | Union types | Enums, `Result<T,E>` |

---

## The Aligned APIs

### Handler Signatures

All handlers receive context, return results:

```ruby
# Ruby
class MyHandler < TaskerCore::StepHandler::Base
  def call(context)
    success(result: { data: "value" })
  end
end
```

```python
# Python
class MyHandler(BaseStepHandler):
    def call(self, context: StepContext) -> StepHandlerResult:
        return self.success({"data": "value"})
```

```typescript
// TypeScript
class MyHandler extends BaseStepHandler {
  async call(context: StepContext): Promise<StepHandlerResult> {
    return this.success({ data: "value" });
  }
}
```

```rust
// Rust
impl StepHandler for MyHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> StepExecutionResult {
        StepExecutionResult::success(json!({"data": "value"}), None)
    }
}
```

### Result Factories

Success and failure patterns are identical:

| Operation | Pattern |
|-----------|---------|
| Success | `success(result_data, metadata?)` |
| Failure | `failure(message, error_type, error_code?, retryable?, metadata?)` |

The factory methods hide implementation details (wrapper classes, enum variants) behind a consistent interface.

### Registry Operations

All registries support the same core operations:

| Operation | Description |
|-----------|-------------|
| `register(name, handler)` | Register a handler by name |
| `is_registered(name)` | Check if handler exists |
| `resolve(name)` | Get handler instance |
| `list_handlers()` | List all registered handlers |

---

## Context Access Patterns

The `StepContext` provides unified access to execution data:

### Core Fields (All Languages)

| Field | Type | Description |
|-------|------|-------------|
| `task_uuid` | String | Unique task identifier |
| `step_uuid` | String | Unique step identifier |
| `input_data` | Dict/Hash | Input data for the step |
| `step_config` | Dict/Hash | Handler configuration |
| `dependency_results` | Wrapper | Results from parent steps |
| `retry_count` | Integer | Current retry attempt |
| `max_retries` | Integer | Maximum retry attempts |

### Convenience Methods

| Method | Description |
|--------|-------------|
| `get_task_field(name)` | Get field from task context |
| `get_dependency_result(step_name)` | Get result from a parent step |

---

## Specialized Handler Patterns

### API Handler

HTTP operations available in all languages:

| Method | Pattern |
|--------|---------|
| GET | `get(path, params?, headers?)` |
| POST | `post(path, data?, headers?)` |
| PUT | `put(path, data?, headers?)` |
| DELETE | `delete(path, params?, headers?)` |

### Decision Handler

Conditional workflow branching:

```ruby
# Ruby
decision_success(steps: ["branch_a", "branch_b"], result_data: { routing: "criteria" })
decision_no_branches(result_data: { reason: "no action needed" })
```

```python
# Python
self.decision_success(["branch_a", "branch_b"], {"routing": "criteria"})
self.decision_no_branches({"reason": "no action needed"})
```

### Batchable Handler

Cursor-based batch processing:

| Operation | Description |
|-----------|-------------|
| `get_batch_context(context)` | Extract batch metadata |
| `batch_worker_complete(count, data)` | Signal batch completion |
| `handle_no_op_worker(batch_ctx)` | Handle empty batch |

---

## FFI Boundary Types

When data crosses the FFI boundary (Rust <-> Ruby/Python/TypeScript), types must serialize identically:

### Required Explicit Types

| Type | Purpose |
|------|---------|
| `DecisionPointOutcome` | Decision handler results |
| `BatchProcessingOutcome` | Batch handler results |
| `CursorConfig` | Batch cursor configuration |
| `StepHandlerResult` | All handler results |

### Serialization Guarantee

The same JSON representation must work across all languages:

```json
{
  "success": true,
  "result": { "data": "value" },
  "metadata": { "timing_ms": 50 }
}
```

---

## Why This Matters

### Developer Productivity

When switching from a Ruby handler to a Python handler:
- No relearning core concepts
- Same mental model applies
- Documentation transfers

### Code Review Consistency

Reviewers can evaluate handlers in any language:
- Pattern violations are obvious
- Best practices are universal
- Anti-patterns are recognizable

### Documentation Efficiency

One set of conceptual docs serves all languages:
- Language-specific pages show syntax only
- Core patterns documented once
- Examples parallel across languages

---

## The Pre-Alpha Advantage

In pre-alpha, we can make breaking changes to achieve consistency:

| Change Type | Example |
|-------------|---------|
| Method renames | `handle()` → `call()` |
| Signature changes | `(task, step)` → `(context)` |
| Return type unification | Separate Success/Error → unified result |

These changes would be costly post-release but are cheap now.

---

## Migration Path

When APIs diverge, we follow this pattern:

1. **Non-Breaking First**: Add aliases, helpers, new modules
2. **Deprecation Period**: Mark old APIs deprecated (warnings in logs)
3. **Breaking Release**: Remove old APIs, document migration

Example timeline (from TAS-92):

```
TAS-95: Python migration (non-breaking + breaking)
TAS-96: Ruby migration (non-breaking + breaking)
TAS-97: Rust alignment (already aligned)
TAS-98: TypeScript alignment (new implementation)
TAS-119: Breaking changes release (all languages together)
```

---

## Anti-Patterns

### Don't: Force Identical Syntax

```python
# BAD: Ruby-style symbols in Python
def call(context) -> Hash[:success => true]  # Not Python!
```

### Don't: Ignore Language Idioms

```ruby
# BAD: Python-style type hints in Ruby
def call(context: StepContext) -> StepHandlerResult  # Not Ruby!
```

### Don't: Duplicate Orchestration Logic

```ruby
# BAD: Worker creating decision steps
def call(context)
  # Don't do orchestration's job!
  create_decision_steps(...)  # Orchestration handles this
end
```

---

## Related Documentation

- [Tasker Core Tenets](./tasker-core-tenets.md) - Tenet #4: Cross-Language Consistency
- [API Convergence Matrix](../worker-crates/api-convergence-matrix.md) - Detailed API reference
- [Patterns and Practices](../worker-crates/patterns-and-practices.md) - Common patterns
- [Example Handlers](../worker-crates/example-handlers.md) - Side-by-side code examples
