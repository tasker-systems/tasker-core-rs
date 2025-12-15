# Python and Ruby Worker Parity Analysis

**Ticket**: TAS-72 (PyO3 Python Worker Foundations)
**Date**: December 2025
**Status**: Phase 6a - E2E Integration Testing

---

## Executive Summary

This document provides a comprehensive analysis of the differences between the Ruby and Python worker implementations in tasker-core. The analysis identifies critical gaps, missing functionality, and a prioritized implementation plan to achieve feature parity.

### Critical Bugs Found (December 15, 2025)

1. **Handler Resolution Bug (FIXED)**: Python was looking at `workflow_step.name` instead of `step_definition.handler.callable`. This caused "Handler not found: unknown_handler" errors.

2. **Field Name Mismatch Bug (FIXED)**: Python's `StepExecutionResult` used field name `output` but Rust FFI expects `result`. This caused all handler results to be `null`, breaking workflow progression because dependency results were empty.

   - **Symptom**: Tasks stuck in "waiting_for_dependencies" state
   - **Root Cause**: `StepExecutionResult.output` serialized as `"output"` but Rust expects `"result"`
   - **Fix**: Renamed field from `output` to `result` in `types.py`

3. **Dependency Result Unwrapping (FIXED)**: Python handlers needed to unwrap nested dependency results structure. Added `StepContext.get_dependency_result()` convenience method matching Ruby's `get_results()` behavior.

4. **PGMQ Message Processing Issue (UNRESOLVED - Rust layer)**: The Rust worker layer shows "Message not found when processing event" warnings for subsequent steps. This is NOT a Python issue - it's in the `tasker-worker` crate's `StepExecutorActor`.

   - **Symptom**: First two steps complete, step 3+ never executes
   - **Log Evidence**: `WARN tasker_worker::worker::actors::step_executor_actor: Message not found when processing event msg_id=31`
   - **Status**: Needs investigation in Rust worker layer

### E2E Test Status (December 15, 2025)

| Test | Status | Notes |
|------|--------|-------|
| test_python_success_scenario | ✅ PASS | Single-step workflow works |
| test_python_permanent_failure_scenario | ✅ PASS | Single-step failure works |
| test_python_linear_workflow_* | ❌ FAIL | Steps 3+ never execute (Rust layer issue) |
| test_python_diamond_workflow_* | ❌ FAIL | Branch steps never execute (Rust layer issue) |
| test_python_retryable_failure_scenario | ❌ FAIL | Stuck waiting (Rust layer issue) |

---

## File Structure Comparison

### Ruby Worker (42 files)
```
workers/ruby/lib/
├── tasker_core.rb                          # Main entry point
└── tasker_core/
    ├── batch_processing/
    │   ├── batch_aggregation_scenario.rb    # Batch result aggregation
    │   └── batch_worker_context.rb          # Batch processing context
    ├── bootstrap.rb                         # Lifecycle management
    ├── domain_events/
    │   ├── base_publisher.rb               # Event publisher base
    │   ├── base_subscriber.rb              # Event subscriber base
    │   ├── publisher_registry.rb           # Publisher registry
    │   └── subscriber_registry.rb          # Subscriber registry
    ├── domain_events.rb                    # Module entry point
    ├── errors/
    │   ├── common.rb                       # Error types
    │   └── error_classifier.rb             # Retryability classification
    ├── errors.rb                           # Module entry point
    ├── event_bridge.rb                     # Rust-Ruby FFI bridge
    ├── handlers.rb                         # Module entry point
    ├── internal.rb                         # Internal utilities
    ├── logger.rb                           # Structured logging
    ├── models.rb                           # FFI data wrappers
    ├── observability/
    │   └── types.rb                        # Health/metrics types
    ├── observability.rb                    # Module entry point
    ├── registry/
    │   ├── handler_registry.rb             # Handler registration
    │   └── step_handler_resolver.rb        # Handler resolution
    ├── registry.rb                         # Module entry point
    ├── step_handler/
    │   ├── api.rb                          # API handler type
    │   ├── base.rb                         # Base handler class
    │   ├── batchable.rb                    # Batchable handler mixin
    │   └── decision.rb                     # Decision handler type
    ├── subscriber.rb                       # Step execution subscriber
    ├── task_handler/
    │   └── base.rb                         # Task handler base
    ├── template_discovery.rb               # YAML template loading
    ├── test_environment.rb                 # Test utilities
    ├── tracing.rb                          # OpenTelemetry support
    ├── types/
    │   ├── batch_processing_outcome.rb     # Batch outcome type
    │   ├── decision_point_outcome.rb       # Decision outcome type
    │   ├── simple_message.rb               # Message type
    │   ├── step_handler_call_result.rb     # Handler result type
    │   ├── step_message.rb                 # Step message type
    │   ├── step_types.rb                   # Step type definitions
    │   ├── task_template.rb                # Template types
    │   └── task_types.rb                   # Task type definitions
    ├── types.rb                            # Module entry point
    ├── version.rb                          # Version info
    └── worker/
        ├── event_poller.rb                 # FFI event polling
        └── in_process_domain_event_poller.rb # Domain event polling
```

### Python Worker (11 files)
```
workers/python/python/
└── tasker_core/
    ├── __init__.py                         # Package entry + FFI re-exports
    ├── bootstrap.py                        # Lifecycle management
    ├── domain_events.py                    # Domain event polling
    ├── event_bridge.py                     # In-process pub/sub (pyee)
    ├── event_poller.py                     # FFI event polling
    ├── exceptions.py                       # Exception types
    ├── handler.py                          # Handler base + registry
    ├── logging.py                          # Structured logging
    ├── observability.py                    # Health/metrics
    ├── step_execution_subscriber.py        # Event routing to handlers
    └── types.py                            # Pydantic models
```

---

## Critical Gaps

### 1. Handler Resolution Bug (CRITICAL - Blocking E2E Tests)

**Ruby Implementation** (subscriber.rb:39-45):
```ruby
# Resolve step handler from registry
handler = @handler_registry.resolve_handler(step_data.step_definition.handler.callable)

unless handler
  raise Errors::ConfigurationError,
        "No handler found for #{step_data.step_definition.handler.callable}"
end
```

**Python Implementation** (step_execution_subscriber.py:200-238):
```python
def _get_handler_name(self, event: FfiStepEvent) -> str:
    tss = event.task_sequence_step
    workflow_step = tss.get("workflow_step", {})

    # WRONG: Looking at workflow_step.name
    step_name = workflow_step.get("name")
    if step_name:
        return str(step_name)

    # Falls back to "unknown_handler"
    return "unknown_handler"
```

**Fix Required**:
```python
def _get_handler_name(self, event: FfiStepEvent) -> str:
    tss = event.task_sequence_step
    step_definition = tss.get("step_definition", {})
    handler = step_definition.get("handler", {})
    callable_name = handler.get("callable")

    if callable_name:
        return str(callable_name)

    return "unknown_handler"
```

---

### 2. Missing Wrapper Classes (Models)

Ruby provides typed wrapper classes that make handler development easier by providing attribute access instead of dictionary key lookups.

**Ruby Has** (models.rb):
- `TaskSequenceStepWrapper` - Main wrapper with attribute accessors
- `TaskWrapper` - Task context with `task_uuid`, `context`, `namespace_name`
- `WorkflowStepWrapper` - Step state with `name`, `attempts`, `max_attempts`, etc.
- `DependencyResultsWrapper` - Dependency access with `get_result()`, `get_results()`
- `StepDefinitionWrapper` - Step definition with `handler`, `dependencies`, `timeout_seconds`
- `HandlerWrapper` - Handler config with `callable`, `initialization`

**Python Lacks**: All of these - handlers receive raw dictionaries

**Implementation Plan**:
```python
# workers/python/python/tasker_core/models.py

@dataclass
class HandlerWrapper:
    callable: str
    initialization: dict[str, Any]

@dataclass
class StepDefinitionWrapper:
    name: str
    description: str
    handler: HandlerWrapper
    dependencies: list[str]
    timeout_seconds: int

@dataclass
class WorkflowStepWrapper:
    workflow_step_uuid: str
    task_uuid: str
    name: str
    attempts: int
    max_attempts: int
    # ... etc

@dataclass
class TaskWrapper:
    task_uuid: str
    context: dict[str, Any]
    namespace_name: str

@dataclass
class DependencyResultsWrapper:
    _results: dict[str, Any]

    def get_result(self, step_name: str) -> Any:
        return self._results.get(step_name)

    def get_results(self, step_name: str) -> Any:
        """Get just the computed value, not full metadata."""
        result = self._results.get(step_name)
        if isinstance(result, dict) and "result" in result:
            return result["result"]
        return result

@dataclass
class TaskSequenceStepWrapper:
    task: TaskWrapper
    workflow_step: WorkflowStepWrapper
    dependency_results: DependencyResultsWrapper
    step_definition: StepDefinitionWrapper

    @classmethod
    def from_dict(cls, data: dict) -> "TaskSequenceStepWrapper":
        # Factory method to create from FFI dict
        ...
```

---

### 3. Missing Specialized Handler Types

**Ruby Has** (step_handler/):
- `StepHandler::Base` - Abstract base with `#call(task, dependency_results, workflow_step)`
- `StepHandler::Api` - HTTP API handler with built-in request/response handling
- `StepHandler::Decision` - Decision point handler returning `DecisionPointOutcome`
- `StepHandler::Batchable` - Mixin for batch processing handlers

**Python Has**:
- `StepHandler` - Basic abstract base class only

**Implementation Priority**: Medium (API and Decision handlers are convenience classes, not strictly required for basic operation)

---

### 4. Missing Error Classification

**Ruby Has** (errors/error_classifier.rb):
```ruby
module Errors
  class ErrorClassifier
    RETRYABLE_ERRORS = [
      Net::OpenTimeout,
      Net::ReadTimeout,
      Errno::ECONNREFUSED,
      # ... many more
    ].freeze

    def self.retryable?(error)
      RETRYABLE_ERRORS.any? { |klass| error.is_a?(klass) }
    end
  end
end
```

**Python Lacks**: Error classification - all errors are treated as retryable by default

**Implementation Plan**:
```python
# workers/python/python/tasker_core/errors/error_classifier.py

RETRYABLE_ERRORS = (
    TimeoutError,
    ConnectionError,
    ConnectionRefusedError,
    ConnectionResetError,
    BrokenPipeError,
    OSError,  # Network errors
)

PERMANENT_ERRORS = (
    ValueError,
    TypeError,
    KeyError,
    AttributeError,
    NotImplementedError,
)

def is_retryable(error: Exception) -> bool:
    """Classify error retryability using systematic classifier."""
    if isinstance(error, PERMANENT_ERRORS):
        return False
    if isinstance(error, RETRYABLE_ERRORS):
        return True
    # Default to retryable for unknown errors
    return True
```

---

### 5. Missing Domain Events Publisher Infrastructure

**Ruby Has** (domain_events/):
- `BasePublisher` - Abstract publisher base
- `BaseSubscriber` - Abstract subscriber base
- `PublisherRegistry` - Registry for event publishers
- `SubscriberRegistry` - Registry for event subscribers

**Python Has**:
- `InProcessDomainEventPoller` - Consumer only (no publishing capability)

**Note**: Domain events are published by Rust orchestration, not worker handlers. Python can consume but not publish. This is by design - only Rust has the full execution context to publish authoritative events.

**Implementation Priority**: Low (consumers work, publishing is handled by Rust)

---

### 6. Missing Batch Processing Support

**Ruby Has** (batch_processing/):
- `BatchAggregationScenario` - Aggregation patterns
- `BatchWorkerContext` - Batch execution context
- `StepHandler::Batchable` - Mixin for batch handlers

**Python Lacks**: All batch processing support

**Note**: Batch processing involves:
1. Template step definitions with `template_step_name` for dynamically created steps
2. Handlers that can process multiple items
3. Result aggregation across batch items

**Implementation Priority**: Medium (needed for batch workflow patterns)

---

### 7. Missing Handler Calling Convention

**Ruby Handler Signature**:
```ruby
def call(task, dependency_results, workflow_step)
  # task = TaskWrapper
  # dependency_results = DependencyResultsWrapper
  # workflow_step = WorkflowStepWrapper
end
```

**Python Handler Signature**:
```python
def call(self, context: StepContext) -> StepHandlerResult:
    # context.input_data - raw dict
    # context.dependencies - raw dict
    # context.step_config - raw dict
```

**Issue**: Python's `StepContext` doesn't align with Ruby's three-argument convention.

**Recommendation**: Update `StepContext.from_ffi_event()` to create wrapper objects:
```python
@dataclass
class StepContext:
    task: TaskWrapper
    dependency_results: DependencyResultsWrapper
    workflow_step: WorkflowStepWrapper
    step_definition: StepDefinitionWrapper

    # Convenience accessors
    @property
    def input_data(self) -> dict:
        return self.task.context

    def get_dependency_result(self, step_name: str) -> Any:
        return self.dependency_results.get_results(step_name)
```

---

## Implementation Plan

### Phase 1: Fix Critical Handler Resolution (Immediate)

**Files to modify**:
- `workers/python/python/tasker_core/step_execution_subscriber.py`

**Changes**:
1. Fix `_get_handler_name()` to use `step_definition.handler.callable`
2. Log handler resolution for debugging

**Effort**: 15 minutes

---

### Phase 2: Add Wrapper Classes (Short-term)

**New files**:
- `workers/python/python/tasker_core/models.py`

**Changes**:
1. Implement all wrapper dataclasses
2. Add factory methods (`from_dict()`)
3. Update `StepContext.from_ffi_event()` to use wrappers
4. Update `StepExecutionSubscriber._execute_handler()` to pass wrappers

**Effort**: 2-3 hours

---

### Phase 3: Add Error Classification (Short-term)

**New files**:
- `workers/python/python/tasker_core/errors/`
- `workers/python/python/tasker_core/errors/__init__.py`
- `workers/python/python/tasker_core/errors/error_classifier.py`

**Changes**:
1. Implement `ErrorClassifier` with retryability rules
2. Update `StepExecutionSubscriber._create_error_result()` to use classifier
3. Ensure `retryable` flag is set correctly in completion metadata

**Effort**: 1 hour

---

### Phase 4: Specialized Handler Types (Medium-term)

**New files**:
- `workers/python/python/tasker_core/step_handler/`
- `workers/python/python/tasker_core/step_handler/__init__.py`
- `workers/python/python/tasker_core/step_handler/base.py`
- `workers/python/python/tasker_core/step_handler/api.py`
- `workers/python/python/tasker_core/step_handler/decision.py`

**Changes**:
1. Move `StepHandler` base to `step_handler/base.py`
2. Implement `ApiHandler` with HTTP client support
3. Implement `DecisionHandler` with outcome types
4. Re-export from main handler module

**Effort**: 4-6 hours

---

### Phase 5: Batch Processing (Medium-term)

**New files**:
- `workers/python/python/tasker_core/batch_processing/`
- `workers/python/python/tasker_core/batch_processing/__init__.py`
- `workers/python/python/tasker_core/batch_processing/batch_context.py`
- `workers/python/python/tasker_core/batch_processing/batchable.py`

**Changes**:
1. Implement batch context class
2. Implement `Batchable` mixin for handlers
3. Handle `template_step_name` in handler resolution

**Effort**: 4-6 hours

---

## Comparison Table

| Feature | Ruby | Python | Priority |
|---------|------|--------|----------|
| FFI Event Polling | EventPoller | EventPoller | Complete |
| Event Bridge (pub/sub) | dry-events | pyee | Complete |
| Step Execution Subscriber | subscriber.rb | step_execution_subscriber.py | Complete |
| Handler Registry | HandlerRegistry | HandlerRegistry | Complete |
| Handler Resolution | `step_definition.handler.callable` | **BROKEN** | **CRITICAL** |
| Wrapper Classes | 6 classes | None | High |
| Error Classification | ErrorClassifier | None | High |
| StepHandler::Base | Complete | Basic | Medium |
| StepHandler::Api | Complete | None | Medium |
| StepHandler::Decision | Complete | None | Medium |
| StepHandler::Batchable | Complete | None | Medium |
| Domain Event Publisher | Complete | Consumer only | Low |
| Batch Processing | Complete | None | Medium |
| Tracing (OpenTelemetry) | Complete | Basic | Low |
| Template Discovery | Complete | Via PYTHON_HANDLER_PATH | Complete |

---

## Step-by-Step Field Access Comparison

### Handler Name Resolution Path

**Ruby** (from `subscriber.rb:45`):
```ruby
step_data.step_definition.handler.callable
# step_data = TaskSequenceStepWrapper
# step_data.step_definition = StepDefinitionWrapper
# step_data.step_definition.handler = HandlerWrapper
# step_data.step_definition.handler.callable = "LinearWorkflow::StepHandlers::LinearStep1Handler"
```

**Python** (from raw FFI dict):
```python
event.task_sequence_step["step_definition"]["handler"]["callable"]
# event.task_sequence_step = dict
# ["step_definition"] = dict with name, description, handler, dependencies, etc.
# ["handler"] = dict with callable, initialization
# ["callable"] = "linear_workflow.step_handlers.linear_step_1_handler"
```

### Dependency Result Access

**Ruby** (from handler):
```ruby
def call(task, dependency_results, workflow_step)
  previous_value = dependency_results.get_results('linear_step_1')
  # Returns just the computed value: 36
end
```

**Python** (from handler):
```python
def call(self, context: StepContext) -> StepHandlerResult:
    previous_value = context.dependencies.get('linear_step_1', {}).get('result')
    # More verbose, no convenience method
```

### Task Context Access

**Ruby**:
```ruby
def call(task, dependency_results, workflow_step)
  even_number = task.context['even_number']
end
```

**Python**:
```python
def call(self, context: StepContext) -> StepHandlerResult:
    even_number = context.input_data.get('even_number')
```

---

## Test Handler Requirements

For E2E tests to pass, handlers must be registered with names matching `step_definition.handler.callable`:

**Linear Workflow Example**:
```yaml
# Task template step definition
steps:
  - name: linear_step_1
    handler:
      callable: linear_workflow.step_handlers.linear_step_1_handler
```

**Handler Registration**:
```python
@dataclass
class LinearStep1Handler(StepHandler):
    handler_name = "linear_workflow.step_handlers.linear_step_1_handler"

    def call(self, context: StepContext) -> StepHandlerResult:
        ...

# In step_handlers/__init__.py
registry = HandlerRegistry.instance()
registry.register(LinearStep1Handler.handler_name, LinearStep1Handler)
```

---

## Conclusion

The Python worker has the core infrastructure (polling, event bridge, subscriber) in place but is missing critical functionality for E2E tests to pass:

1. **Handler resolution is broken** - looks at wrong field
2. **No wrapper classes** - handlers work with raw dicts
3. **No error classification** - all errors treated the same

Implementing Phase 1 (handler resolution fix) will unblock E2E tests. Phases 2-3 will bring Python to functional parity with Ruby for basic workflows. Phases 4-5 add convenience features for advanced patterns.

---

## Appendix: FFI Data Structure Reference

### FfiStepEvent (from poll_step_events())
```python
{
    "event_id": "uuid-string",
    "task_uuid": "uuid-string",
    "step_uuid": "uuid-string",
    "correlation_id": "uuid-string",
    "trace_id": "optional-trace-id",
    "span_id": "optional-span-id",
    "task_correlation_id": "uuid-string",
    "parent_correlation_id": "optional-uuid",
    "task_sequence_step": {
        "task": {
            "task": {
                "task_uuid": "uuid-string",
                "context": {"even_number": 2}
            },
            "namespace_name": "linear_workflow",
            "task_name": "linear_test",
            "task_version": "1.0"
        },
        "workflow_step": {
            "workflow_step_uuid": "uuid-string",
            "task_uuid": "uuid-string",
            "name": "linear_step_1",
            "attempts": 0,
            "max_attempts": 3,
            "in_process": false,
            "processed": false,
            "results": {}
        },
        "dependency_results": {
            "previous_step": {"result": 36, "metadata": {}}
        },
        "step_definition": {
            "name": "linear_step_1",
            "description": "Square the initial even number",
            "handler": {
                "callable": "linear_workflow.step_handlers.linear_step_1_handler",
                "initialization": {}
            },
            "dependencies": [],
            "timeout_seconds": 30
        }
    }
}
```
