"""Step handler base class.

This module provides the StepHandler abstract base class that all step
handlers must inherit from.

Handlers can implement either synchronous or asynchronous call methods:

Example (Synchronous):
    >>> from tasker_core.step_handler import StepHandler
    >>> from tasker_core import StepContext, StepHandlerResult, ErrorType
    >>>
    >>> class MyHandler(StepHandler):
    ...     handler_name = "my_handler"
    ...
    ...     def call(self, context: StepContext) -> StepHandlerResult:
    ...         return StepHandlerResult.success({"processed": True})

Example (Asynchronous - TAS-131):
    >>> import asyncio
    >>> from tasker_core.step_handler import StepHandler
    >>> from tasker_core import StepContext, StepHandlerResult
    >>>
    >>> class MyAsyncHandler(StepHandler):
    ...     handler_name = "my_async_handler"
    ...
    ...     async def call(self, context: StepContext) -> StepHandlerResult:
    ...         # Can use async operations like aiohttp, asyncpg, etc.
    ...         await asyncio.sleep(0.1)
    ...         return StepHandlerResult.success({"processed": True})
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from collections.abc import Awaitable
from typing import TYPE_CHECKING, Any, Union

if TYPE_CHECKING:
    from tasker_core.types import StepContext, StepHandlerResult

# Type alias for handler call return type (sync or async)
# Handlers can return StepHandlerResult directly (sync) or Awaitable[StepHandlerResult] (async)
StepHandlerCallResult = Union["StepHandlerResult", Awaitable["StepHandlerResult"]]


class StepHandler(ABC):
    """Abstract base class for step handlers.

    All step handlers must inherit from this class and implement
    the `call` method. The handler_name class attribute must be set
    to a unique identifier for the handler.

    Class Attributes:
        handler_name: Unique identifier for this handler. Must be set by subclasses.
        handler_version: Version string for the handler (default: "1.0.0").

    Example:
        >>> class ProcessOrderHandler(StepHandler):
        ...     handler_name = "process_order"
        ...     handler_version = "1.0.0"
        ...
        ...     def call(self, context: StepContext) -> StepHandlerResult:
        ...         order_id = context.input_data.get("order_id")
        ...         # Process the order...
        ...         return StepHandlerResult.success(
        ...             {"order_id": order_id, "status": "processed"}
        ...         )
    """

    # Class attributes - must be set by subclasses
    handler_name: str = ""
    handler_version: str = "1.0.0"

    @abstractmethod
    def call(self, context: StepContext) -> StepHandlerCallResult:
        """Execute the step handler logic.

        This method is called by the execution subscriber when a step
        event is received that matches this handler's name.

        TAS-131: This method can be implemented as either synchronous or
        asynchronous. The execution subscriber will detect which pattern
        is used and handle accordingly.

        Args:
            context: Execution context with input data, dependency results,
                and configuration.

        Returns:
            StepHandlerResult indicating success or failure.
            Can be returned directly (sync) or as an awaitable (async).

        Raises:
            Any exception will be caught by the execution subscriber
            and converted to a failure result.

        Example (Synchronous):
            >>> def call(self, context: StepContext) -> StepHandlerResult:
            ...     try:
            ...         result = process(context.input_data)
            ...         return StepHandlerResult.success(result)
            ...     except ValidationError as e:
            ...         return StepHandlerResult.failure(
            ...             message=str(e),
            ...             error_type=ErrorType.VALIDATION_ERROR,
            ...             retryable=False,
            ...         )

        Example (Asynchronous):
            >>> async def call(self, context: StepContext) -> StepHandlerResult:
            ...     try:
            ...         result = await fetch_data_async(context.input_data)
            ...         return StepHandlerResult.success(result)
            ...     except asyncio.TimeoutError:
            ...         return StepHandlerResult.failure(
            ...             message="Request timed out",
            ...             error_type="timeout_error",
            ...             retryable=True,
            ...         )
        """
        ...

    @property
    def name(self) -> str:
        """Return the handler name.

        Returns:
            The handler_name class attribute, or the class name if not set.
        """
        return self.handler_name or self.__class__.__name__

    @property
    def version(self) -> str:
        """Return the handler version.

        Returns:
            The handler_version class attribute.
        """
        return self.handler_version

    @property
    def capabilities(self) -> list[str]:
        """Return handler capabilities.

        Override this to advertise specific capabilities for handler selection.

        Returns:
            List of capability strings (default: ["process"]).
        """
        return ["process"]

    def config_schema(self) -> dict[str, Any] | None:
        """Return JSON schema for handler configuration.

        Override this to provide a schema for validating step_config.

        Returns:
            JSON schema dict, or None if no schema is defined.
        """
        return None

    def success(
        self,
        result: dict[str, Any] | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> StepHandlerResult:
        """Create a success result.

        Convenience method for creating success results.

        Args:
            result: Result data dictionary.
            metadata: Optional metadata dictionary.

        Returns:
            StepHandlerResult with is_success=True.
        """
        from tasker_core.types import StepHandlerResult

        return StepHandlerResult.success(result or {}, metadata)

    def failure(
        self,
        message: str,
        error_type: str = "handler_error",
        retryable: bool = True,
        metadata: dict[str, Any] | None = None,
        error_code: str | None = None,
    ) -> StepHandlerResult:
        """Create a failure result.

        Convenience method for creating failure results.

        Args:
            message: Error message.
            error_type: Error type classification. Use ErrorType enum for consistency.
            retryable: Whether the error is retryable.
            metadata: Optional metadata dictionary.
            error_code: Optional application-specific error code.

        Returns:
            StepHandlerResult with success=False.
        """
        from tasker_core.types import StepHandlerResult

        return StepHandlerResult.failure(
            message=message,
            error_type=error_type,
            retryable=retryable,
            metadata=metadata,
            error_code=error_code,
        )

    def __repr__(self) -> str:
        """Return a string representation of the handler."""
        return f"{self.__class__.__name__}(name={self.name!r}, version={self.version!r})"


__all__ = ["StepHandler", "StepHandlerCallResult"]
