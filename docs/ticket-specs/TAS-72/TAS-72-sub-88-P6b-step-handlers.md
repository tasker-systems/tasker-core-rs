# TAS-88: Phase 6b - Specialized Handler Types and Batch Processing

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

#### 5.2 Batchable Mixin

**File**: `workers/python/python/tasker_core/batch_processing/batchable.py`

Key features to implement:
- Context extraction from workflow step cursor configuration
- Cursor config creation for dividing work
- Outcome builders (no_batches_outcome, create_batches_outcome)
- Aggregation helpers for combining batch results

```python
from __future__ import annotations

from typing import Any, Callable

from tasker_core.step_handler.base import StepHandler
from tasker_core.types import StepHandlerResult
from tasker_core.types.batch_outcome import BatchProcessingOutcome
from tasker_core.batch_processing.batch_context import BatchWorkerContext
from tasker_core.batch_processing.batch_aggregation import BatchAggregationScenario


class Batchable(StepHandler):
    """Batchable step handler mixin for parallel batch processing.

    Provides helper methods for:
    - Extracting cursor context from workflow steps
    - Creating cursor configurations for batch workers
    - Building batch processing outcomes
    - Aggregating results from batch workers

    Example (Analyzer):
        >>> class CsvAnalyzer(Batchable):
        ...     handler_name = "analyze_csv"
        ...
        ...     def call(self, context: StepContext) -> StepHandlerResult:
        ...         total_rows = count_csv_rows(context.input_data["file"])
        ...
        ...         if total_rows == 0:
        ...             return self.no_batches_outcome(reason="empty_file")
        ...
        ...         cursor_configs = self.create_cursor_configs(total_rows, 5)
        ...         return self.create_batches_outcome(
        ...             worker_template_name="process_csv_batch",
        ...             cursor_configs=cursor_configs,
        ...             total_items=total_rows,
        ...         )

    Example (Worker):
        >>> class CsvBatchWorker(Batchable):
        ...     handler_name = "process_csv_batch"
        ...
        ...     def call(self, context: StepContext) -> StepHandlerResult:
        ...         batch_context = self.extract_cursor_context(context)
        ...
        ...         if batch_context.no_op:
        ...             return self.handle_no_op_worker(batch_context)
        ...
        ...         # Process rows from start_cursor to end_cursor
        ...         processed = process_rows(
        ...             batch_context.start_cursor,
        ...             batch_context.end_cursor,
        ...         )
        ...         return StepHandlerResult.success_handler_result({
        ...             "processed_count": processed,
        ...         })
    """

    @property
    def capabilities(self) -> list[str]:
        return super().capabilities + [
            "batchable",
            "batch_processing",
            "parallel_execution",
            "cursor_based",
        ]

    # Context extraction
    def extract_cursor_context(self, context) -> BatchWorkerContext:
        """Extract cursor context from step workflow data."""
        return BatchWorkerContext.from_step_context(context)

    # Cursor config creation
    def create_cursor_configs(
        self,
        total_items: int,
        worker_count: int,
    ) -> list[dict[str, Any]]:
        """Create cursor configurations for batch workers."""
        if worker_count <= 0:
            raise ValueError("worker_count must be > 0")

        items_per_worker = -(-total_items // worker_count)  # Ceiling division
        configs = []

        for i in range(worker_count):
            start = (i * items_per_worker) + 1
            end = min(start + items_per_worker, total_items + 1)

            configs.append({
                "batch_id": f"{i + 1:03d}",
                "start_cursor": start,
                "end_cursor": end,
                "batch_size": end - start,
            })

        return configs

    # Outcome builders
    def no_batches_outcome(
        self,
        reason: str,
        metadata: dict | None = None,
    ) -> StepHandlerResult:
        """Create NoBatches outcome for analyzer steps."""
        outcome = BatchProcessingOutcome.no_batches()

        result = {
            "batch_processing_outcome": outcome.to_dict(),
            "reason": reason,
            **(metadata or {}),
        }

        return StepHandlerResult.success_handler_result(result)

    def create_batches_outcome(
        self,
        worker_template_name: str,
        cursor_configs: list[dict[str, Any]],
        total_items: int,
        metadata: dict | None = None,
    ) -> StepHandlerResult:
        """Create CreateBatches outcome for analyzer steps."""
        outcome = BatchProcessingOutcome.create_batches(
            worker_template_name=worker_template_name,
            worker_count=len(cursor_configs),
            cursor_configs=cursor_configs,
            total_items=total_items,
        )

        result = {
            "batch_processing_outcome": outcome.to_dict(),
            "worker_count": len(cursor_configs),
            "total_items": total_items,
            **(metadata or {}),
        }

        return StepHandlerResult.success_handler_result(result)

    # No-op worker handling
    def handle_no_op_worker(
        self,
        context: BatchWorkerContext,
    ) -> StepHandlerResult | None:
        """Handle no-op placeholder worker scenario."""
        if not context.no_op:
            return None

        return StepHandlerResult.success_handler_result({
            "batch_id": context.batch_id,
            "no_op": True,
            "processed_count": 0,
        })
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

## E2E Test Plan

### Test Templates

Create task templates in `tests/fixtures/task_templates/python/`:

| Template | Purpose |
|----------|---------|
| `api_handler_py.yaml` | Test ApiHandler HTTP operations |
| `decision_workflow_py.yaml` | Test DecisionHandler routing |
| `batch_processing_py.yaml` | Test Batchable batch creation |

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

## Dependencies

### Python Packages

Add to `pyproject.toml`:
```toml
[project.dependencies]
httpx = ">=0.25.0"  # Async-capable HTTP client
```

## Success Criteria

1. All specialized handler types implemented with full Ruby parity
2. Unit tests for each handler type (>90% coverage)
3. E2E tests passing for all specialized handler scenarios
4. Documentation with examples for each handler type
5. Exports added to `tasker_core.__init__.py`

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

## References

- Ruby StepHandler::Api: `workers/ruby/lib/tasker_core/step_handler/api.rb`
- Ruby StepHandler::Decision: `workers/ruby/lib/tasker_core/step_handler/decision.rb`
- Ruby StepHandler::Batchable: `workers/ruby/lib/tasker_core/step_handler/batchable.rb`
- Python-Ruby Parity Analysis: `docs/ticket-specs/TAS-72/python-and-ruby-parity.md`
- Existing E2E tests: `tests/e2e/python/`
