# Python Best Practices for Tasker Core

**Purpose**: Codify Python-specific coding standards for the tasker-core workers/python project.

---

## Code Style

### Formatting & Linting
- Use `ruff` for linting and formatting (enforced via `cargo make check-python`)
- Use `mypy` for type checking
- Follow PEP 8 with project-specific settings in `pyproject.toml`

### Naming Conventions
```python
# Classes: PascalCase
class StepHandler:
    pass

class OrderProcessingHandler(StepHandler):
    pass

# Functions/methods: snake_case
def process_step(context: StepContext) -> StepHandlerResult:
    pass

# Constants: SCREAMING_SNAKE_CASE
DEFAULT_TIMEOUT = 30
MAX_RETRIES = 3

# Private: leading underscore
def _internal_helper(data: dict) -> dict:
    pass

# Module-level: snake_case
order_handler = OrderHandler()
```

### Module Organization
```python
# handlers/order_handler.py
"""Order processing step handler.

This module provides handlers for order lifecycle operations.
"""

from __future__ import annotations

# 1. Standard library imports
from typing import Any

# 2. Third-party imports
import httpx

# 3. Local imports
from tasker_core.step_handler import StepHandler
from tasker_core.step_handler.mixins import APIMixin, DecisionMixin
from tasker_core.types import StepContext, StepHandlerResult

# 4. Module constants
DEFAULT_API_TIMEOUT = 30

# 5. Classes
class OrderHandler(StepHandler, APIMixin):
    """Handles order processing operations."""

    def call(self, context: StepContext) -> StepHandlerResult:
        """Process the order step."""
        pass

# 6. Module-level functions (if any)
def create_handler() -> OrderHandler:
    """Factory function for handler creation."""
    return OrderHandler()
```

---

## Type Hints

### Always Use Type Hints
```python
from typing import Any, Optional

from tasker_core.types import StepContext, StepHandlerResult

class OrderHandler(StepHandler):
    def call(self, context: StepContext) -> StepHandlerResult:
        order_id: str = context.input_data.get("order_id", "")
        config: dict[str, Any] = context.step_config

        result: Optional[dict[str, Any]] = self._process_order(order_id)

        if result is not None:
            return self.success(result)
        return self.failure("Order not found", error_type="NotFoundError")

    def _process_order(self, order_id: str) -> Optional[dict[str, Any]]:
        """Process a single order."""
        pass
```

### Type Aliases for Complex Types
```python
from typing import TypeAlias

OrderData: TypeAlias = dict[str, Any]
BatchResult: TypeAlias = list[dict[str, Any]]

def process_orders(orders: list[OrderData]) -> BatchResult:
    pass
```

---

## Handler Patterns

### Base Handler Structure
```python
from tasker_core.step_handler import StepHandler
from tasker_core.step_handler.mixins import APIMixin
from tasker_core.types import StepContext, StepHandlerResult


class OrderHandler(StepHandler, APIMixin):
    """Processes order operations via external API.

    This handler fetches order data and validates inventory availability.
    Uses the API mixin for HTTP operations.
    """

    def call(self, context: StepContext) -> StepHandlerResult:
        """Execute the order processing step.

        Args:
            context: Step execution context with input data and config.

        Returns:
            Success result with order data, or failure with error details.
        """
        try:
            order_id = context.input_data["order_id"]
            response = self.get(f"/api/orders/{order_id}")

            if response.status_code == 200:
                return self.success(
                    result=response.json(),
                    metadata={"fetched_at": datetime.now().isoformat()},
                )

            return self.failure(
                message=f"API error: {response.status_code}",
                error_type="APIError",
                retryable=response.status_code >= 500,
            )

        except KeyError as e:
            return self.failure(
                message=f"Missing required field: {e}",
                error_type="ValidationError",
                retryable=False,
            )
        except Exception as e:
            return self.failure(
                message=str(e),
                error_type="UnexpectedError",
                retryable=True,
            )
```

### Result Factory Methods
```python
# Success result
self.success(
    result={"order_id": "123", "status": "processed"},
    metadata={"duration_ms": 150},
)

# Failure result
self.failure(
    message="Order validation failed",
    error_type="ValidationError",
    error_code="INVALID_QUANTITY",
    retryable=False,
    metadata={"field": "quantity"},
)

# Decision result (for decision handlers)
self.decision_success(
    steps=["ship_order", "send_confirmation"],
    routing_context={"decision": "standard_flow"},
)

# Skip branches
self.skip_branches(
    reason="No items require processing",
    routing_context={"skip_reason": "empty_cart"},
)
```

---

## Mixin Pattern (TAS-112)

### Using Mixins
```python
from tasker_core.step_handler import StepHandler
from tasker_core.step_handler.mixins import APIMixin, DecisionMixin, BatchableMixin


class MultiCapabilityHandler(StepHandler, APIMixin, DecisionMixin):
    """Handler with multiple capabilities via mixins."""

    def call(self, context: StepContext) -> StepHandlerResult:
        # API mixin provides HTTP methods
        response = self.get("/api/status")

        if response.json().get("needs_decision"):
            # Decision mixin provides routing methods
            return self.decision_success(
                steps=["route_a", "route_b"],
                routing_context={"type": "conditional"},
            )

        return self.success(result=response.json())
```

### Available Mixins
| Mixin | Purpose | Methods Provided |
|-------|---------|------------------|
| `APIMixin` | HTTP requests | `get`, `post`, `put`, `delete` |
| `DecisionMixin` | Decision points | `decision_success`, `skip_branches`, `decision_failure` |
| `BatchableMixin` | Batch processing | `get_batch_context`, `batch_worker_complete`, `handle_no_op_worker` |

---

## Error Handling

### Use Specific Exceptions
```python
# Define handler-specific exceptions
class HandlerError(Exception):
    """Base exception for handler errors."""

class ValidationError(HandlerError):
    """Raised when input validation fails."""

class ConfigurationError(HandlerError):
    """Raised when handler configuration is invalid."""


# Usage in handler
def call(self, context: StepContext) -> StepHandlerResult:
    try:
        self._validate_input(context.input_data)
        result = self._process(context)
        return self.success(result=result)
    except ValidationError as e:
        return self.failure(str(e), error_type="ValidationError", retryable=False)
    except httpx.TimeoutException as e:
        return self.failure(str(e), error_type="TimeoutError", retryable=True)
    except Exception as e:
        return self.failure(str(e), error_type="UnexpectedError", retryable=True)
```

---

## Documentation

### Docstrings (Google Style)
```python
class OrderHandler(StepHandler):
    """Handles order processing operations.

    This handler coordinates order validation, inventory checks,
    and fulfillment initiation.

    Attributes:
        api_base_url: Base URL for the order API.
        timeout: Request timeout in seconds.

    Example:
        >>> handler = OrderHandler()
        >>> context = build_context(input_data={"order_id": "123"})
        >>> result = handler.call(context)
        >>> assert result.success
    """

    def call(self, context: StepContext) -> StepHandlerResult:
        """Process an order step.

        Args:
            context: Execution context containing order data and config.
                Expected input_data keys:
                - order_id (str): The order identifier
                - priority (str, optional): Processing priority

        Returns:
            StepHandlerResult with order processing outcome.

        Raises:
            ValidationError: If order_id is missing or invalid.

        Example:
            >>> result = handler.call(context)
            >>> if result.success:
            ...     print(f"Order {result.result['id']} processed")
        """
        pass
```

---

## Testing

### pytest Patterns
```python
# tests/handlers/test_order_handler.py
import pytest
from unittest.mock import Mock, patch

from tasker_core.types import StepContext
from handlers.order_handler import OrderHandler


class TestOrderHandler:
    """Tests for OrderHandler."""

    @pytest.fixture
    def handler(self) -> OrderHandler:
        """Create handler instance."""
        return OrderHandler()

    @pytest.fixture
    def context(self) -> StepContext:
        """Create test context."""
        return StepContext(
            task_uuid="test-task-uuid",
            step_uuid="test-step-uuid",
            input_data={"order_id": "12345"},
            step_config={},
        )

    def test_call_with_valid_order_returns_success(
        self,
        handler: OrderHandler,
        context: StepContext,
    ) -> None:
        """Test successful order processing."""
        with patch.object(handler, "get") as mock_get:
            mock_get.return_value = Mock(
                status_code=200,
                json=lambda: {"id": "12345", "status": "pending"},
            )

            result = handler.call(context)

            assert result.success
            assert result.result["id"] == "12345"

    def test_call_with_missing_order_id_returns_failure(
        self,
        handler: OrderHandler,
    ) -> None:
        """Test failure when order_id is missing."""
        context = StepContext(
            task_uuid="test-task",
            step_uuid="test-step",
            input_data={},  # Missing order_id
            step_config={},
        )

        result = handler.call(context)

        assert not result.success
        assert result.error_type == "ValidationError"
        assert not result.retryable

    @pytest.mark.parametrize(
        "status_code,expected_retryable",
        [
            (400, False),
            (404, False),
            (500, True),
            (502, True),
            (503, True),
        ],
    )
    def test_call_handles_api_errors_correctly(
        self,
        handler: OrderHandler,
        context: StepContext,
        status_code: int,
        expected_retryable: bool,
    ) -> None:
        """Test API error handling for various status codes."""
        with patch.object(handler, "get") as mock_get:
            mock_get.return_value = Mock(status_code=status_code)

            result = handler.call(context)

            assert not result.success
            assert result.retryable == expected_retryable
```

---

## FFI Considerations

### Working with Rust Extensions
```python
# The native extension is loaded automatically
from tasker_core._native import some_ffi_function

# Types are converted automatically:
# Python dict <-> Rust HashMap
# Python list <-> Rust Vec
# Python str <-> Rust String

# Be mindful of large data transfers across FFI boundary
def process(context: StepContext) -> StepHandlerResult:
    # Extract needed data immediately
    order_id = context.input_data["order_id"]

    # Process with Python logic, minimize FFI calls
    result = self._python_processing(order_id)

    return self.success(result=result)
```

---

## Project-Specific Patterns

### Registry Usage
```python
from tasker_core.registry import Registry

# Register handlers
Registry.instance().register("order_handler", OrderHandler)

# Check availability
Registry.instance().is_registered("order_handler")

# Resolve handler
handler = Registry.instance().resolve("order_handler")
```

### Domain Events
```python
from tasker_core.domain_events import BasePublisher, StepEventContext


class OrderCompletedPublisher(BasePublisher):
    """Publishes order completion events."""

    def subscribes_to(self) -> str:
        return "order.completed"

    def publish(self, ctx: StepEventContext) -> None:
        """Publish the order completed event."""
        self._notify_downstream(ctx)

    # Lifecycle hooks (TAS-112)
    def before_publish(self, ctx: StepEventContext) -> None:
        logger.info(f"Publishing order event for {ctx.step_uuid}")

    def after_publish(self, ctx: StepEventContext, result: Any) -> None:
        metrics.increment("order.completed.published")

    def on_publish_error(self, ctx: StepEventContext, error: Exception) -> None:
        logger.error(f"Failed to publish: {error}")
```

---

## References

- [PEP 8 Style Guide](https://peps.python.org/pep-0008/)
- [Google Python Style Guide](https://google.github.io/styleguide/pyguide.html)
- [API Convergence Matrix](../../workers/api-convergence-matrix.md)
- [Python Worker Documentation](../../workers/python.md)
- [Composition Over Inheritance](../../principles/composition-over-inheritance.md)
