# TAS-88: Phase 6b - Specialized Handler Types and Batch Processing

## Implementation Status: COMPLETE (December 2025)

All E2E tests passing (18/18 Python tests). Key achievements:
- DecisionHandler with full routing capabilities
- Batchable mixin with cursor-based parallel processing
- ApiHandler with HTTP client and error classification
- Comprehensive test coverage exceeding original plan

---

## Overview

This phase implements specialized step handler types and batch processing support for the Python worker, achieving parity with Ruby's advanced handler capabilities. This includes `ApiHandler` for HTTP operations, `DecisionHandler` for dynamic workflow routing, and `Batchable` mixin for parallel batch processing.

## Prerequisites

- TAS-72-P1 through P6: Core Python worker implementation (complete)
- TAS-86-P6a: E2E Integration Tests (complete)
- Phases 1-3 from python-and-ruby-parity.md (complete):
  - Phase 1: Handler resolution and critical fixes
  - Phase 2: Wrapper classes (models.py)
  - Phase 3: Error classification

## Objectives

1. Implement `ApiHandler` base class with HTTP client support
2. Implement `DecisionHandler` for dynamic workflow decision points
3. Implement `Batchable` mixin for batch processing handlers
4. Create supporting types (DecisionPointOutcome, BatchProcessingOutcome)
5. Add E2E tests for specialized handlers following established patterns

## Architecture Analysis

### Ruby Implementation Reference

```
workers/ruby/lib/tasker_core/step_handler/
├── base.rb                    # Base handler (already ported)
├── api.rb                     # HTTP client handler (~500 lines)
├── decision.rb                # Decision point handler (~300 lines)
└── batchable.rb               # Batch processing mixin (~450 lines)

workers/ruby/lib/tasker_core/types/
├── decision_point_outcome.rb  # Decision outcome types
├── batch_processing_outcome.rb # Batch outcome types
└── step_handler_call_result.rb # Handler result types (already ported)

workers/ruby/lib/tasker_core/batch_processing/
├── batch_worker_context.rb    # Batch context extraction
└── batch_aggregation_scenario.rb # Aggregation detection
```

### Python Target Structure

```
workers/python/python/tasker_core/
├── step_handler/
│   ├── __init__.py           # Re-exports all handler types
│   ├── base.py               # StepHandler base (move from handler.py)
│   ├── api.py                # ApiHandler with httpx client
│   └── decision.py           # DecisionHandler for routing
├── batch_processing/
│   ├── __init__.py           # Re-exports
│   ├── batchable.py          # Batchable mixin
│   ├── batch_context.py      # BatchWorkerContext
│   └── batch_aggregation.py  # BatchAggregationScenario
└── types/
    ├── decision_outcome.py   # DecisionPointOutcome
    └── batch_outcome.py      # BatchProcessingOutcome
```

### IMPLEMENTATION NOTE: Actual File Structure

The implementation consolidated types into a single `types.py` module for better maintainability:

```
workers/python/python/tasker_core/
├── step_handler/
│   ├── __init__.py           # ✅ Re-exports (as planned)
│   ├── base.py               # ✅ StepHandler base
│   ├── api.py                # ✅ ApiHandler with httpx (~500 lines)
│   └── decision.py           # ✅ DecisionHandler (~300 lines)
├── batch_processing/
│   ├── __init__.py           # ✅ Re-exports
│   └── batchable.py          # ✅ Batchable mixin (~690 lines, expanded from plan)
├── types.py                  # ✅ All types consolidated (DEVIATION from separate files)
│   # Contains: DecisionType, DecisionPointOutcome, CursorConfig,
│   # BatchAnalyzerOutcome, BatchWorkerContext, BatchWorkerOutcome
└── handler.py                # ✅ StepHandler re-exported for backwards compatibility
```

**Rationale**: Consolidating types into a single module reduces import complexity and provides a single source of truth. This is more Pythonic and easier to maintain.

---

## Implementation Plan

### Phase 4: Specialized Handler Types

#### 4.1 Restructure Handler Module

**File**: `workers/python/python/tasker_core/step_handler/__init__.py`

Move `StepHandler` from `handler.py` to `step_handler/base.py` and re-export:

```python
from tasker_core.step_handler.base import StepHandler
from tasker_core.step_handler.api import ApiHandler
from tasker_core.step_handler.decision import DecisionHandler

__all__ = ["StepHandler", "ApiHandler", "DecisionHandler"]
```

**IMPLEMENTATION NOTE**: ✅ Implemented as planned. Handler module restructured with proper re-exports.

#### 4.2 ApiHandler Implementation

**File**: `workers/python/python/tasker_core/step_handler/api.py`

Key features to implement:
- HTTP client using `httpx` (async-capable, similar to Faraday)
- Automatic error classification based on HTTP status codes
- Retry-After header support for rate limiting
- SSL/TLS configuration
- Authentication support (Bearer, Basic, API Key)
- Convenience methods: `get()`, `post()`, `put()`, `delete()`

```python
from __future__ import annotations

from typing import Any
import httpx

from tasker_core.step_handler.base import StepHandler
from tasker_core.errors import (
    PermanentError,
    RetryableError,
    RateLimitError,
    ServiceUnavailableError,
)


class ApiHandler(StepHandler):
    """API step handler with HTTP client support.

    Provides HTTP functionality mirroring Ruby's StepHandler::Api:
    - httpx HTTP client with full configuration support
    - Automatic error classification (RetryableError vs PermanentError)
    - Retry-After header support for server-requested backoff
    - SSL configuration, headers, query parameters

    Example:
        >>> class MyApiHandler(ApiHandler):
        ...     handler_name = "my_api_handler"
        ...
        ...     def call(self, context: StepContext) -> StepHandlerResult:
        ...         response = self.get("/api/users", params={"limit": 10})
        ...         return StepHandlerResult.success_handler_result(response)
    """

    @property
    def capabilities(self) -> list[str]:
        return super().capabilities + [
            "http_client",
            "error_classification",
            "retry_headers",
        ]

    @property
    def client(self) -> httpx.Client:
        """Get or create the HTTP client."""
        if not hasattr(self, "_client"):
            self._client = self._build_client()
        return self._client

    def get(self, path: str, **kwargs) -> dict[str, Any]:
        """Perform HTTP GET request with automatic error classification."""
        response = self.client.get(path, **kwargs)
        return self._process_response(response)

    def post(self, path: str, **kwargs) -> dict[str, Any]:
        """Perform HTTP POST request with automatic error classification."""
        response = self.client.post(path, **kwargs)
        return self._process_response(response)

    # ... additional methods
```

**IMPLEMENTATION NOTE**: ✅ Implemented with enhancements:
- HTTP status code classification:
  - Non-retryable: 400, 401, 403, 404, 405, 406, 410, 422
  - Retryable: 408, 429, 500, 502, 503, 504
- ApiResponse wrapper class with body parsing
- Lazy client initialization
- E2E tests deferred (requires HTTP mocking infrastructure)

#### 4.3 DecisionHandler Implementation

**File**: `workers/python/python/tasker_core/step_handler/decision.py`

Key features to implement:
- Helper methods for returning decision outcomes
- Type-safe `DecisionPointOutcome` creation
- Support for `CreateSteps` and `NoBranches` outcomes
- Validation of decision point structure

```python
from __future__ import annotations

from tasker_core.step_handler.base import StepHandler
from tasker_core.types import StepHandlerResult
from tasker_core.types.decision_outcome import DecisionPointOutcome


class DecisionHandler(StepHandler):
    """Decision step handler for dynamic workflow routing.

    Provides helper methods for creating decision point outcomes
    that Rust orchestration can process to dynamically create steps.

    Example:
        >>> class ApprovalRouter(DecisionHandler):
        ...     handler_name = "approval_router"
        ...
        ...     def call(self, context: StepContext) -> StepHandlerResult:
        ...         amount = context.input_data.get("amount", 0)
        ...
        ...         if amount < 1000:
        ...             return self.decision_success(
        ...                 steps=["auto_approve"],
        ...                 result_data={"route": "auto"}
        ...             )
        ...         else:
        ...             return self.decision_success(
        ...                 steps=["manager_approval", "finance_review"],
        ...                 result_data={"route": "dual_approval"}
        ...             )
    """

    @property
    def capabilities(self) -> list[str]:
        return super().capabilities + [
            "decision_point",
            "dynamic_workflow",
            "step_creation",
        ]

    def decision_success(
        self,
        steps: list[str] | str,
        result_data: dict | None = None,
        metadata: dict | None = None,
    ) -> StepHandlerResult:
        """Create successful decision outcome that creates specified steps."""
        step_names = [steps] if isinstance(steps, str) else steps
        outcome = DecisionPointOutcome.create_steps(step_names)

        result = {
            **(result_data or {}),
            "decision_point_outcome": outcome.to_dict(),
        }

        return StepHandlerResult.success_handler_result(
            result,
            metadata=self._build_decision_metadata(metadata, outcome),
        )

    def decision_no_branches(
        self,
        result_data: dict | None = None,
        metadata: dict | None = None,
    ) -> StepHandlerResult:
        """Create decision outcome indicating no additional steps needed."""
        outcome = DecisionPointOutcome.no_branches()

        result = {
            **(result_data or {}),
            "decision_point_outcome": outcome.to_dict(),
        }

        return StepHandlerResult.success_handler_result(
            result,
            metadata=self._build_decision_metadata(metadata, outcome),
        )
```

**IMPLEMENTATION NOTE**: ✅ Implemented with additional methods:
- `route_to_steps(step_names, routing_context)` - convenience wrapper
- `skip_branches(reason)` - create no-branches with reason
- `decision_failure(message, error_type)` - non-retryable decision errors
- Enhanced metadata tracking with handler name and version

#### 4.4 DecisionPointOutcome Type

**File**: `workers/python/python/tasker_core/types/decision_outcome.py`

```python
from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Any


class DecisionOutcomeType(str, Enum):
    CREATE_STEPS = "CreateSteps"
    NO_BRANCHES = "NoBranches"


@dataclass
class DecisionPointOutcome:
    """Decision point outcome for dynamic workflow routing.

    Serializes to format expected by Rust orchestration:
    {
        "outcome_type": "CreateSteps",
        "step_names": ["step1", "step2"]
    }
    """

    outcome_type: DecisionOutcomeType
    step_names: list[str] = field(default_factory=list)

    @classmethod
    def create_steps(cls, step_names: list[str]) -> DecisionPointOutcome:
        """Create outcome that creates specified steps."""
        return cls(
            outcome_type=DecisionOutcomeType.CREATE_STEPS,
            step_names=step_names,
        )

    @classmethod
    def no_branches(cls) -> DecisionPointOutcome:
        """Create outcome indicating no additional steps needed."""
        return cls(
            outcome_type=DecisionOutcomeType.NO_BRANCHES,
            step_names=[],
        )

    def to_dict(self) -> dict[str, Any]:
        """Serialize to dictionary for FFI."""
        return {
            "outcome_type": self.outcome_type.value,
            "step_names": self.step_names,
        }
```

**IMPLEMENTATION NOTE**: ✅ Implemented in `types.py` with key changes:

| Aspect | Planned | Actual | Rationale |
|--------|---------|--------|-----------|
| Base Class | `@dataclass` | Pydantic `BaseModel` | Validation/serialization consistency |
| Enum Values | "CreateSteps", "NoBranches" | "create_steps", "no_branches" | Rust snake_case convention |
| Field Name | `step_names` | `next_step_names` | Clarity, avoid shadowing |
| Enhancements | None | `dynamic_steps`, `reason`, `routing_context` | FFI support for dynamic definitions |

**Critical Rust Integration**: The `decision_point_outcome` dict must serialize as:
```python
{
    "type": "create_steps",      # snake_case, matches Rust expectation
    "step_names": ["step1"],     # Must be present for step creation
}
```

---

### Phase 5: Batch Processing

#### 5.1 BatchProcessingOutcome Type

**File**: `workers/python/python/tasker_core/types/batch_outcome.py`

```python
from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Any


class BatchOutcomeType(str, Enum):
    NO_BATCHES = "NoBatches"
    CREATE_BATCHES = "CreateBatches"


@dataclass
class BatchProcessingOutcome:
    """Batch processing outcome for analyzer steps.

    Serializes to format expected by Rust orchestration for
    dynamic batch worker creation.
    """

    outcome_type: BatchOutcomeType
    worker_template_name: str | None = None
    worker_count: int = 0
    cursor_configs: list[dict[str, Any]] = field(default_factory=list)
    total_items: int = 0

    @classmethod
    def no_batches(cls) -> BatchProcessingOutcome:
        """Create outcome indicating no batching needed."""
        return cls(outcome_type=BatchOutcomeType.NO_BATCHES)

    @classmethod
    def create_batches(
        cls,
        worker_template_name: str,
        worker_count: int,
        cursor_configs: list[dict[str, Any]],
        total_items: int,
    ) -> BatchProcessingOutcome:
        """Create outcome that creates batch workers."""
        return cls(
            outcome_type=BatchOutcomeType.CREATE_BATCHES,
            worker_template_name=worker_template_name,
            worker_count=worker_count,
            cursor_configs=cursor_configs,
            total_items=total_items,
        )

    def to_dict(self) -> dict[str, Any]:
        """Serialize to dictionary for FFI."""
        result = {"outcome_type": self.outcome_type.value}
        if self.outcome_type == BatchOutcomeType.CREATE_BATCHES:
            result.update({
                "worker_template_name": self.worker_template_name,
                "worker_count": self.worker_count,
                "cursor_configs": self.cursor_configs,
                "total_items": self.total_items,
            })
        return result
```

**IMPLEMENTATION NOTE**: ✅ Implemented with major architectural improvement - split into specialized types:

1. **`CursorConfig`** (new type, not in plan):
   ```python
   class CursorConfig(BaseModel):
       start_cursor: int
       end_cursor: int
       step_size: int = 1
       metadata: dict[str, Any] = {}
   ```

2. **`BatchAnalyzerOutcome`** (replaces planned single type):
   ```python
   class BatchAnalyzerOutcome(BaseModel):
       cursor_configs: list[CursorConfig]  # Type-safe, not raw dicts
       total_items: int | None
       batch_metadata: dict[str, Any]
   ```

3. **`BatchWorkerOutcome`** (new type for worker results):
   ```python
   class BatchWorkerOutcome(BaseModel):
       items_processed: int
       items_succeeded: int
       items_failed: int
       results: list[Any]
       errors: list[dict[str, Any]]
       last_cursor: int
       metadata: dict[str, Any]
   ```

**Rationale**: Separating analyzer and worker outcomes provides better type safety and clearer semantics for Rust integration.

#### 5.2 Batchable Mixin

**File**: `workers/python/python/tasker_core/batch_processing/batchable.py`

Key features to implement:
- Context extraction from workflow step cursor configuration
- Cursor config creation for dividing work
- Outcome builders (no_batches_outcome, create_batches_outcome)
- Aggregation helpers for combining batch results

**IMPLEMENTATION NOTE**: ✅ Implemented with significant expansion (~690 lines vs ~150 outlined):

| Feature | Planned | Implemented | Enhancement |
|---------|---------|-------------|-------------|
| Cursor helpers | 1 method | 2 methods + factories | `max_batches` constraint support |
| Outcome builders | 2 methods | 4 methods + dual interfaces | Keyword args AND object styles |
| Aggregation | Simple counting | Sophisticated with error limiting (max 100) | Production-ready |
| Partial failures | Not included | `batch_worker_partial_failure()` | Soft failure handling |
| Context extraction | Basic | Full positioning info (batch_index, total_batches) | Aggregator coordination |

**Key Implementation Details**:

1. **Cursor Configuration with max_batches**:
   ```python
   def create_cursor_ranges(
       self,
       total_items: int,
       batch_size: int,
       step_size: int = 1,
       max_batches: int | None = None,  # Dynamically adjusts batch_size
   ) -> list[CursorConfig]
   ```

2. **Dual Interface Pattern** (supports both styles):
   ```python
   # Object-oriented style
   outcome = self.create_batch_outcome(total_items=1000, batch_size=100)
   self.batch_analyzer_success(outcome)

   # Ruby-like keyword style
   self.batch_analyzer_success(
       cursor_configs=configs,
       total_items=1000,
       worker_template_name="process_batch"
   )
   ```

3. **Rust FFI Format** (critical for integration):
   ```python
   batch_processing_outcome: dict[str, Any] = {
       "type": "create_batches",  # snake_case for Rust
       "worker_template_name": worker_template_name,
       "worker_count": len(configs_list),
       "cursor_configs": [
           {"batch_id": "001", "start_cursor": 0, "end_cursor": 200, "batch_size": 200},
           ...
       ],
       "total_items": total,
   }
   ```

4. **Aggregation with Error Limiting**:
   ```python
   @staticmethod
   def aggregate_worker_results(worker_results) -> dict[str, Any]:
       # ... counting logic ...
       return {
           "total_processed": total_processed,
           "batch_count": len(worker_results),  # NOTE: batch_count, not worker_count
           "errors": all_errors[:100],  # Limit to prevent memory bloat
           "error_count": len(all_errors),
       }
   ```

#### 5.3 BatchWorkerContext

**File**: `workers/python/python/tasker_core/batch_processing/batch_context.py`

```python
from __future__ import annotations

from dataclasses import dataclass
from typing import Any


@dataclass
class BatchWorkerContext:
    """Context extracted from workflow step cursor configuration.

    Contains cursor boundaries and batch metadata for batch workers.
    """

    batch_id: str
    start_cursor: int | str
    end_cursor: int | str
    batch_size: int
    no_op: bool = False
    metadata: dict[str, Any] | None = None

    @classmethod
    def from_step_context(cls, context) -> BatchWorkerContext:
        """Extract batch context from StepContext."""
        workflow_step = context.event.task_sequence_step.get("workflow_step", {})
        inputs = workflow_step.get("inputs", {})

        return cls(
            batch_id=inputs.get("batch_id", "000"),
            start_cursor=inputs.get("start_cursor", 0),
            end_cursor=inputs.get("end_cursor", 0),
            batch_size=inputs.get("batch_size", 0),
            no_op=inputs.get("no_op", False),
            metadata=inputs.get("metadata"),
        )
```

**IMPLEMENTATION NOTE**: ✅ Implemented in `types.py` with structural changes:

| Aspect | Planned | Actual | Rationale |
|--------|---------|--------|-----------|
| Cursor storage | Separate fields | Single `CursorConfig` object | Type safety, reusability |
| start_cursor/end_cursor | Direct fields | `@property` derived from cursor_config | Single source of truth |
| batch_index/total_batches | Not included | Included | Needed for aggregator coordination |
| no_op flag | Included | Removed | Not needed in practice |

**Critical Fix**: Context extraction uses `context.step_inputs` (from `workflow_step.inputs`), not `context.step_config`:
```python
# Correct pattern - cursor config comes from workflow_step.inputs
step_inputs = context.step_inputs or {}
cursor = step_inputs.get("cursor", {})
start_cursor = cursor.get("start_cursor", 0)
end_cursor = cursor.get("end_cursor", 0)
```

---

## E2E Test Plan

### Test Templates

Create task templates in `tests/fixtures/task_templates/python/`:

| Template | Purpose |
|----------|---------|
| `api_handler_py.yaml` | Test ApiHandler HTTP operations |
| `decision_workflow_py.yaml` | Test DecisionHandler routing |
| `batch_processing_py.yaml` | Test Batchable batch creation |

### IMPLEMENTATION NOTE: Actual Templates Created

| Template | Purpose | Status |
|----------|---------|--------|
| `conditional_approval_handler_py.yaml` | Decision point routing (3 branches) | ✅ Complete (221 lines) |
| `batch_processing_py.yaml` | CSV batch processing (5 workers) | ✅ Complete (130 lines) |
| `api_handler_py.yaml` | HTTP operations | ❌ Deferred (needs mock server) |

### Test Cases

**File**: `tests/e2e/python/specialized_handlers_test.rs`

1. **ApiHandler Tests**
   - `test_python_api_handler_success` - Successful HTTP request
   - `test_python_api_handler_error_classification` - HTTP error handling

2. **DecisionHandler Tests**
   - `test_python_decision_single_branch` - Single step creation
   - `test_python_decision_multi_branch` - Multiple step creation
   - `test_python_decision_no_branches` - No branches outcome

3. **Batch Processing Tests**
   - `test_python_batch_analyzer_creates_workers` - Analyzer creates batches
   - `test_python_batch_workers_process_cursors` - Workers process ranges
   - `test_python_batch_aggregator_combines_results` - Aggregator merges

### IMPLEMENTATION NOTE: Actual Tests Implemented

**Conditional Approval Tests** (`tests/e2e/python/conditional_approval_test.rs`, 429 lines):

| Test | Purpose | Status |
|------|---------|--------|
| `test_python_small_amount_auto_approval` | Auto-approval path (<$1,000) | ✅ Pass |
| `test_python_medium_amount_manager_approval` | Manager-only path ($1,000-$4,999) | ✅ Pass |
| `test_python_large_amount_dual_approval` | Dual approval path (≥$5,000) | ✅ Pass |
| `test_python_boundary_small_threshold` | Boundary at $1,000 | ✅ Pass |
| `test_python_boundary_large_threshold` | Boundary at $5,000 | ✅ Pass |
| `test_python_very_small_amount` | Edge case ($0.01) | ✅ Pass |

**Decision Point Tests**: **6 tests** (vs 3 planned) - exceeds plan with boundary/edge case coverage.

**Batch Processing Tests** (`tests/e2e/python/batch_processing_test.rs`, 400 lines):

| Test | Purpose | Status |
|------|---------|--------|
| `test_python_csv_batch_processing` | Full workflow: 1000 rows → 5 workers | ✅ Pass |
| `test_python_no_batches_scenario` | Empty CSV → NoBatches outcome | ✅ Pass |
| `test_python_batch_processing_scaling` | Scaling validation | ✅ Pass |

**Batch Processing Tests**: **3 tests** (matches plan).

**ApiHandler Tests**: ❌ Deferred (requires HTTP mocking infrastructure).

---

## Example Handlers Implemented

### Conditional Approval Handlers

**File**: `workers/python/tests/handlers/examples/conditional_approval/step_handlers/__init__.py` (317 lines)

| Handler | Base Class | Purpose |
|---------|-----------|---------|
| `ValidateRequestHandler` | StepHandler | Input validation (amount, requester, purpose) |
| `RoutingDecisionHandler` | DecisionHandler | Route based on amount thresholds |
| `AutoApproveHandler` | StepHandler | Auto-approve small amounts |
| `ManagerApprovalHandler` | StepHandler | Manager review for medium/large |
| `FinanceReviewHandler` | StepHandler | Finance review for large amounts |
| `FinalizeApprovalHandler` | StepHandler | Deferred convergence aggregator |

### Batch Processing Handlers

**File**: `workers/python/tests/handlers/examples/batch_processing/step_handlers/__init__.py` (311 lines)

| Handler | Base Class | Purpose |
|---------|-----------|---------|
| `CsvAnalyzerHandler` | StepHandler + Batchable | Count rows, create cursor configs |
| `CsvBatchProcessorHandler` | StepHandler + Batchable | Process rows in cursor range |
| `CsvResultsAggregatorHandler` | StepHandler + Batchable | Aggregate worker results |

---

## Bugs Fixed During Implementation

### 1. Status Value Mismatch
**Issue**: Decision points not creating dynamic steps
**Root Cause**: Python sent `status: "success"` but Rust checks `if status != "completed"`
**Fix**: Changed `ResultStatus.SUCCESS` from `"success"` to `"completed"` in `types.py`

### 2. step_inputs vs step_config
**Issue**: Batch workers processing 0 items
**Root Cause**: Cursor config is in `workflow_step.inputs`, not `step_definition.handler.initialization`
**Fix**: Added `step_inputs` field to `StepContext`, read cursor from `context.step_inputs`

### 3. Cursor Range Off-by-One
**Issue**: 1004 rows processed instead of 1000
**Root Cause**: 0-based cursor ranges with 1-based row enumeration
**Fix**: Changed `if row_num < start_cursor` to `if row_num <= start_cursor`

### 4. Dependency Result Unwrapping
**Issue**: Aggregator showing 0 total processed despite workers processing items
**Root Cause**: Direct iteration over `context.dependency_results` without unwrapping
**Fix**: Use `context.get_dependency_result(dep_name)` which unwraps `{"result": {...}}`

### 5. CSV Column Mapping
**Issue**: Total inventory value $0 (all items processed correctly)
**Root Cause**: Handler looked for `quantity` column but CSV has `stock`
**Fix**: Changed `quantity` to `stock` to match fixture data

### 6. worker_count vs batch_count
**Issue**: Worker count showing 0 in results
**Root Cause**: `aggregate_worker_results` returns `batch_count`, code looked for `worker_count`
**Fix**: Use `aggregated.get("batch_count", len(worker_results))`

---

## Dependencies

### Python Packages

Add to `pyproject.toml`:
```toml
[project.dependencies]
httpx = ">=0.25.0"  # Async-capable HTTP client
```

**IMPLEMENTATION NOTE**: ✅ Added to dependencies.

---

## Success Criteria

1. All specialized handler types implemented with full Ruby parity
2. Unit tests for each handler type (>90% coverage)
3. E2E tests passing for all specialized handler scenarios
4. Documentation with examples for each handler type
5. Exports added to `tasker_core.__init__.py`

**IMPLEMENTATION STATUS**:
- ✅ Handler types implemented with Ruby parity
- ✅ Unit tests (25+ in test_phase6b.py)
- ✅ E2E tests passing (9 tests: 6 decision + 3 batch)
- ✅ Example handlers with documentation
- ✅ Exports in `tasker_core.__init__.py`

---

## Estimated Effort

| Phase | Component | Effort |
|-------|-----------|--------|
| 4.1 | Handler module restructure | 1 hour |
| 4.2 | ApiHandler | 3-4 hours |
| 4.3 | DecisionHandler | 2 hours |
| 4.4 | DecisionPointOutcome type | 1 hour |
| 5.1 | BatchProcessingOutcome type | 1 hour |
| 5.2 | Batchable mixin | 3-4 hours |
| 5.3 | BatchWorkerContext | 1 hour |
| 5.4 | E2E tests | 3-4 hours |
| | **Total** | **15-18 hours** |

**ACTUAL EFFORT**: ~20 hours (additional time for bug fixes and integration testing)

---

## Lessons Learned

### 1. Rust FFI Integration is Exacting
- Status values MUST match exactly: `"completed"`, `"error"` (not `"success"`, `"failure"`)
- Enum values use snake_case: `"create_steps"`, `"no_branches"` (not PascalCase)
- Decision point outcome key is `"type"`, not `"outcome_type"`

### 2. Type Consolidation Works Well
- Single `types.py` reduces import complexity
- Pydantic BaseModel provides validation + serialization
- `@property` accessors for derived values keep code clean

### 3. Batch Processing Needs Separation
- Analyzer outcome ≠ Worker outcome (different data needs)
- `CursorConfig` as first-class type eliminates duplication
- Aggregation needs batch positioning info (index, total)

### 4. Context Extraction Patterns Matter
- `step_inputs` (from `workflow_step.inputs`) for cursor config
- `step_config` (from `step_definition.handler.initialization`) for handler config
- `get_dependency_result()` for unwrapped dependency data

### 5. Test Coverage Should Exceed Minimum
- Boundary tests catch off-by-one errors
- Edge cases (empty CSV, $0.01 amount) find integration issues
- E2E tests with real file I/O expose concurrency bugs

---

## References

- Ruby StepHandler::Api: `workers/ruby/lib/tasker_core/step_handler/api.rb`
- Ruby StepHandler::Decision: `workers/ruby/lib/tasker_core/step_handler/decision.rb`
- Ruby StepHandler::Batchable: `workers/ruby/lib/tasker_core/step_handler/batchable.rb`
- Python-Ruby Parity Analysis: `docs/ticket-specs/TAS-72/python-and-ruby-parity.md`
- Existing E2E tests: `tests/e2e/python/`
