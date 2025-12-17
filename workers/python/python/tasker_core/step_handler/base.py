"""Step handler base class.

This module provides the StepHandler abstract base class that all step
handlers must inherit from.

Example:
    >>> from tasker_core.step_handler import StepHandler
    >>> from tasker_core import StepContext, StepHandlerResult
    >>>
    >>> class MyHandler(StepHandler):
    ...     handler_name = "my_handler"
    ...
    ...     def call(self, context: StepContext) -> StepHandlerResult:
    ...         return StepHandlerResult.success_handler_result({"processed": True})
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from tasker_core.types import StepContext, StepHandlerResult


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
        ...         return StepHandlerResult.success_handler_result(
        ...             {"order_id": order_id, "status": "processed"}
        ...         )
    """

    # Class attributes - must be set by subclasses
    handler_name: str = ""
    handler_version: str = "1.0.0"

    @abstractmethod
    def call(self, context: StepContext) -> StepHandlerResult:
        """Execute the step handler logic.

        This method is called by the execution subscriber when a step
        event is received that matches this handler's name.

        Args:
            context: Execution context with input data, dependency results,
                and configuration.

        Returns:
            StepHandlerResult indicating success or failure.

        Raises:
            Any exception will be caught by the execution subscriber
            and converted to a failure result.

        Example:
            >>> def call(self, context: StepContext) -> StepHandlerResult:
            ...     try:
            ...         result = process(context.input_data)
            ...         return StepHandlerResult.success_handler_result(result)
            ...     except ValidationError as e:
            ...         return StepHandlerResult.failure_handler_result(
            ...             message=str(e),
            ...             error_type="ValidationError",
            ...             retryable=False,
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
            StepHandlerResult with success=True.
        """
        from tasker_core.types import StepHandlerResult

        return StepHandlerResult.success_handler_result(result or {}, metadata)

    def failure(
        self,
        message: str,
        error_type: str = "handler_error",
        retryable: bool = True,
        metadata: dict[str, Any] | None = None,
    ) -> StepHandlerResult:
        """Create a failure result.

        Convenience method for creating failure results.

        Args:
            message: Error message.
            error_type: Error type classification.
            retryable: Whether the error is retryable.
            metadata: Optional metadata dictionary.

        Returns:
            StepHandlerResult with success=False.
        """
        from tasker_core.types import StepHandlerResult

        return StepHandlerResult.failure_handler_result(
            message=message,
            error_type=error_type,
            retryable=retryable,
            metadata=metadata,
        )

    def __repr__(self) -> str:
        """Return a string representation of the handler."""
        return f"{self.__class__.__name__}(name={self.name!r}, version={self.version!r})"


__all__ = ["StepHandler"]
