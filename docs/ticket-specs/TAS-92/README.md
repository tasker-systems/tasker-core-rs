# TAS-92: Consistent Developer-space APIs and Ergonomics

**Status**: In Progress
**Branch**: `jcoletaylor/tas-92-consistent-developer-space-apis-and-ergonomics-parent`
**Linear**: [TAS-92](https://linear.app/tasker-systems/issue/TAS-92)

## Overview

This parent ticket coordinates the alignment of developer-facing APIs across Ruby, Python, and Rust worker implementations. The goal is to reduce context switching for developers working across languages while respecting each language's idioms.

**Pre-alpha status**: No backward compatibility required - breaking changes can be made directly.

## Analysis

The more full analysis of the work to be done is available in the [TAS-92 Analysis](./analysis.md) document.

## Child Tickets

| Ticket | Title | Priority | Status |
|--------|-------|----------|--------|
| [TAS-95](./TAS-95-python-worker-api-alignment.md) | Python Worker API Alignment | Medium | Todo |
| [TAS-96](./TAS-96-ruby-worker-api-alignment.md) | Ruby Worker API Alignment | Medium | Todo |
| [TAS-97](./TAS-97-rust-worker-api-alignment.md) | Rust Worker API Alignment | Medium | Todo |
| [TAS-98](./TAS-98-documentation-updates.md) | Cross-Language Documentation Updates | Medium | Todo |

## Execution Order

The recommended execution order:

1. **TAS-97 (Rust)** - Minimal changes, establishes baseline patterns
2. **TAS-95 (Python)** - Moderate changes, Python already well-aligned
3. **TAS-96 (Ruby)** - Most significant changes, handler signature migration
4. **TAS-98 (Documentation)** - Must be done LAST after all code changes

## Key Alignment Goals

### A. Handler Signatures → `call(context)`

| Language | Current | Target |
|----------|---------|--------|
| Ruby | `call(task, sequence, step)` | `call(context)` |
| Python | `call(context)` | (already aligned) |
| Rust | `call(&TaskSequenceStep)` | (already aligned) |

### B. Result Factories → `success()` / `failure()`

| Language | Current Success | Target Success |
|----------|-----------------|----------------|
| Ruby | `success(...)` | (already aligned) |
| Python | `success_handler_result(...)` | `success(...)` |
| Rust | `StepExecutionResult::success(...)` | (already aligned) |

### C. Registry API Parity

Target methods for all languages:
- `register(name, handler_class)`
- `is_registered(name) -> bool`
- `resolve(name) -> handler_instance`
- `list_handlers() -> list[str]`

### D. Specialized Handlers

- **API Handler**: Add `get/post/put/delete` convenience methods to Ruby
- **Decision Handler**: Add `decision_success(steps, routing_context)` helper to Python
- **Batchable**: Standardize on `batch_worker_success` and `get_batch_context`

### E. Error Fields

Standardize across all languages:
- `error_message`, `error_type`, `error_code` (optional), `retryable`
- Recommended `error_type` values: `permanent_error`, `retryable_error`, `validation_error`, `timeout`, `handler_error`

### F. Domain Events

- Standardize `BasePublisher` with `publish(ctx)` method
- Add Python `BaseSubscriber` with `subscribes_to` pattern
- Document `StepEventContext` structure consistently

## Validation Plan

See [validation-plan.md](./validation-plan.md) for the comprehensive verification checklist to be executed after all child tickets are complete.

## Open Questions - RESOLVED

| Question | Resolution |
|----------|------------|
| Should Ruby `StepContext` be a new class or a compatibility wrapper? | **New class wrapping existing `TaskSequenceStepWrapper`** - The FFI bridge already provides `TaskSequenceStepWrapper` with all needed data. Create `StepContext` as a thin wrapper that adds cross-language standard accessors (`task_uuid`, `step_uuid`, `input_data`, etc.) while exposing the underlying wrapper objects for Ruby-specific access. |
| Python error_type: hard enum or soft with Literal type hints? | **Hard enum (`str, Enum`)** - Use Python's `str, Enum` pattern for type safety while maintaining string serialization. Matches Ruby and Rust patterns for consistency. |
| Rust error_code: first-class field or metadata? | **Already in metadata** - `error_code` already exists as a field in `StepExecutionMetadata`. Just add convenience helper methods (`with_error_code()`, `error_code()`) on `StepExecutionResult` for ergonomics. |

## Files Structure

```
docs/ticket-specs/TAS-92/
├── README.md                              # This file
├── analysis.md                            # Original analysis from Linear
├── TAS-95-python-worker-api-alignment.md  # Python plan
├── TAS-96-ruby-worker-api-alignment.md    # Ruby plan
├── TAS-97-rust-worker-api-alignment.md    # Rust plan
├── TAS-98-documentation-updates.md        # Documentation plan
└── validation-plan.md                     # Final validation checklist
```
