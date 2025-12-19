# TAS-95: Python Worker API Alignment

**Parent**: [TAS-92](./README.md)
**Linear**: [TAS-95](https://linear.app/tasker-systems/issue/TAS-95)
**Branch**: `jcoletaylor/tas-95-python-worker-api-alignment`
**Priority**: Medium

## Objective

Align Python worker APIs with cross-language standards. Python is already well-aligned; changes are primarily renaming and adding missing features.

## Summary of Changes

| Area | Current State | Target State | Effort |
|------|---------------|--------------|--------|
| Handler Signature | `call(context)` | No change | None |
| Result Factories | `success_handler_result()` | `success()` | Low |
| Error Fields | No `error_code` | Add `error_code` | Low |
| Registry API | Already aligned | No change | None |
| Decision Handler | Complex API | Add simple helper | Low |
| Domain Events | No base classes | Add Publisher/Subscriber | Medium |

## Implementation Plan

### Phase 1: Result Factory Renames

**Files to modify:**
- `python/tasker_core/types.py`
- `python/tasker_core/step_handler/base.py`

**Changes:**

1. In `types.py` - rename class methods:
   ```python
   # Before
   @classmethod
   def success_handler_result(cls, result, metadata=None):
       ...

   @classmethod
   def failure_handler_result(cls, message, error_type, retryable, metadata=None):
       ...

   # After
   @classmethod
   def success(cls, result, metadata=None):
       ...

   @classmethod
   def failure(cls, message, error_type, retryable, metadata=None, error_code=None):
       ...
   ```

2. In `base.py` - update helper method calls:
   ```python
   # Update internal calls to use new method names
   def success(self, result, metadata=None):
       return StepHandlerResult.success(result, metadata)

   def failure(self, message, error_type, retryable, metadata=None, error_code=None):
       return StepHandlerResult.failure(message, error_type, retryable, metadata, error_code)
   ```

### Phase 2: Error Fields Standardization

**Files to modify:**
- `python/tasker_core/types.py`

**Changes:**

1. Add `error_code` field to `StepHandlerResult`:
   ```python
   @dataclass
   class StepHandlerResult:
       success: bool
       result: Optional[Dict[str, Any]] = None
       metadata: Optional[Dict[str, Any]] = None
       error_message: Optional[str] = None
       error_type: Optional[str] = None
       error_code: Optional[str] = None  # NEW
       retryable: bool = False
   ```

2. Add Enum for `error_type` (hard enum to match Ruby and Rust):
   ```python
   from enum import Enum

   class ErrorType(str, Enum):
       """Standard error types for cross-language consistency.

       Using str, Enum allows the value to serialize as a string while
       providing type safety and IDE support.
       """
       PERMANENT_ERROR = "permanent_error"
       RETRYABLE_ERROR = "retryable_error"
       VALIDATION_ERROR = "validation_error"
       TIMEOUT = "timeout"
       HANDLER_ERROR = "handler_error"
   ```

   Note: Using `str, Enum` (StrEnum in Python 3.11+) allows:
   - Type checking enforcement
   - IDE autocomplete
   - Direct string serialization (no `.value` needed)
   - Backward compatibility with existing code expecting strings

### Phase 3: Decision Handler Enhancement

**Files to modify:**
- `python/tasker_core/step_handler/decision.py`

**Changes:**

1. Add simplified helper method:
   ```python
   def decision_success(
       self,
       steps: list[str],
       routing_context: Optional[dict] = None
   ) -> StepHandlerResult:
       """Simplified decision success helper.

       Args:
           steps: List of step names to activate
           routing_context: Optional context for routing decisions
       """
       outcome = DecisionPointOutcome.create_steps(steps, routing_context or {})
       return self.decision_success_with_outcome(outcome)
   ```

2. Rename existing method:
   ```python
   # Before
   def decision_success(self, outcome: DecisionPointOutcome) -> StepHandlerResult:

   # After
   def decision_success_with_outcome(self, outcome: DecisionPointOutcome) -> StepHandlerResult:
   ```

### Phase 4: Domain Events Base Classes

**New files:**
- `python/tasker_core/domain_events/base_publisher.py`
- `python/tasker_core/domain_events/base_subscriber.py`

**Changes:**

1. Create `BasePublisher`:
   ```python
   from abc import ABC, abstractmethod
   from dataclasses import dataclass
   from typing import Any, Dict, Optional

   @dataclass
   class StepEventContext:
       task_uuid: str
       step_uuid: str
       step_name: str
       namespace: str
       correlation_id: str
       result: Optional[Dict[str, Any]] = None
       metadata: Optional[Dict[str, Any]] = None

   class BasePublisher(ABC):
       @abstractmethod
       def name(self) -> str:
           """Return the publisher name."""
           pass

       @abstractmethod
       def publish(self, ctx: StepEventContext) -> None:
           """Publish an event with the given context."""
           pass

       def should_publish(self, ctx: StepEventContext) -> bool:
           """Override to conditionally publish."""
           return True

       def transform_payload(self, ctx: StepEventContext) -> Dict[str, Any]:
           """Override to transform the event payload."""
           return {
               "task_uuid": ctx.task_uuid,
               "step_uuid": ctx.step_uuid,
               "step_name": ctx.step_name,
               "namespace": ctx.namespace,
               "correlation_id": ctx.correlation_id,
               "result": ctx.result,
               "metadata": ctx.metadata,
           }
   ```

2. Create `BaseSubscriber`:
   ```python
   from abc import ABC, abstractmethod
   from typing import Any, Dict, List

   class BaseSubscriber(ABC):
       @classmethod
       @abstractmethod
       def subscribes_to(cls) -> List[str]:
           """Return list of event patterns to subscribe to."""
           pass

       @abstractmethod
       def handle(self, event: Dict[str, Any]) -> None:
           """Handle the received event."""
           pass
   ```

3. Update `__init__.py` exports

### Phase 5: Update Example Handlers

**Files to modify:**
- All files in `tests/handlers/examples/`

**Changes:**
- Replace `success_handler_result` → `success`
- Replace `failure_handler_result` → `failure`
- Use standard `error_type` values
- Update decision handler examples to use simplified helper

### Phase 6: Update Tests

**Files to modify:**
- `tests/test_step_handler.py`
- `tests/test_module_exports.py`
- Create new domain event tests

**Changes:**
- Update all test cases to use new method names
- Add tests for `error_code` field
- Add tests for `BasePublisher` and `BaseSubscriber`
- Add tests for decision handler helpers

## Files Summary

### Core Library Changes
| File | Change Type |
|------|-------------|
| `python/tasker_core/types.py` | Modify |
| `python/tasker_core/step_handler/base.py` | Modify |
| `python/tasker_core/step_handler/decision.py` | Modify |
| `python/tasker_core/domain_events/base_publisher.py` | Create |
| `python/tasker_core/domain_events/base_subscriber.py` | Create |
| `python/tasker_core/__init__.py` | Modify |

### Example/Test Changes
| File | Change Type |
|------|-------------|
| `tests/handlers/examples/*.py` | Modify |
| `tests/test_step_handler.py` | Modify |
| `tests/test_module_exports.py` | Modify |
| `tests/test_domain_events.py` | Create |

## Verification Checklist

- [ ] `success()` and `failure()` methods work correctly
- [ ] `error_code` field properly serializes/deserializes
- [ ] `error_type` Literal hints provide IDE support
- [ ] `decision_success(steps, routing_context)` works
- [ ] `BasePublisher` can be subclassed and used
- [ ] `BaseSubscriber` can be subclassed and used
- [ ] All example handlers updated and functional
- [ ] All unit tests pass
- [ ] All integration tests pass
- [ ] Type checking passes (`mypy`)
- [ ] Linting passes (`ruff`)

## Risk Assessment

**Low Risk**: Python already closely matches target state. Changes are primarily:
- Method renames (non-breaking in pre-alpha)
- Adding optional fields
- Adding new base classes

## Estimated Scope

- **New lines**: ~150 (domain events base classes)
- **Modified lines**: ~100 (renames and enhancements)
- **Test additions**: ~100
