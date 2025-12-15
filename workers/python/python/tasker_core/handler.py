"""Step handler base class and registry for handler discovery.

This module provides the StepHandler abstract base class that all step
handlers must inherit from, and the HandlerRegistry for registering
and resolving handlers.

Example:
    >>> from tasker_core import StepHandler, StepContext, StepHandlerResult
    >>>
    >>> class MyHandler(StepHandler):
    ...     handler_name = "my_handler"
    ...
    ...     def call(self, context: StepContext) -> StepHandlerResult:
    ...         # Process the step
    ...         return StepHandlerResult.success_handler_result({"processed": True})
    ...
    >>> # Register the handler
    >>> from tasker_core import HandlerRegistry
    >>> registry = HandlerRegistry.instance()
    >>> registry.register("my_handler", MyHandler)
    >>>
    >>> # Resolve and execute
    >>> handler = registry.resolve("my_handler")
    >>> result = handler.call(context)
"""

from __future__ import annotations

import importlib
import pkgutil
from abc import ABC, abstractmethod
from typing import TYPE_CHECKING, Any

from .logging import log_debug, log_error, log_info, log_warn

if TYPE_CHECKING:
    from .types import StepContext, StepHandlerResult


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
            ...         # Do work
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

    def __repr__(self) -> str:
        """Return a string representation of the handler."""
        return f"{self.__class__.__name__}(name={self.name!r}, version={self.version!r})"


class HandlerRegistry:
    """Registry for step handler classes.

    Provides handler discovery, registration, and resolution.
    Implements singleton pattern for global handler management.

    Supports multiple discovery modes:
    - Manual registration via register()
    - Package scanning via discover_handlers()

    Example:
        >>> registry = HandlerRegistry.instance()
        >>>
        >>> # Manual registration
        >>> registry.register("my_handler", MyHandler)
        >>>
        >>> # Resolve handler
        >>> handler = registry.resolve("my_handler")
        >>> assert handler is not None
        >>>
        >>> # Discover handlers from package
        >>> count = registry.discover_handlers("myapp.handlers")
        >>> print(f"Discovered {count} handlers")
    """

    _instance: HandlerRegistry | None = None

    def __init__(self) -> None:
        """Initialize the HandlerRegistry.

        Creates an empty handler registry. Prefer using
        HandlerRegistry.instance() to get the singleton.
        """
        self._handlers: dict[str, type[StepHandler]] = {}

    @classmethod
    def instance(cls) -> HandlerRegistry:
        """Get the singleton registry instance.

        Returns:
            The singleton HandlerRegistry instance.

        Example:
            >>> registry = HandlerRegistry.instance()
            >>> assert registry is HandlerRegistry.instance()
        """
        if cls._instance is None:
            cls._instance = cls()
        return cls._instance

    @classmethod
    def reset_instance(cls) -> None:
        """Reset the singleton instance.

        This is primarily for testing to ensure a clean state between tests.

        Example:
            >>> HandlerRegistry.reset_instance()
            >>> # Fresh registry for next test
        """
        cls._instance = None

    def register(
        self,
        name: str,
        handler_class: type[StepHandler],
    ) -> None:
        """Register a handler class.

        Args:
            name: Handler name (must match step definition).
            handler_class: StepHandler subclass.

        Raises:
            ValueError: If handler_class is not a StepHandler subclass.

        Example:
            >>> registry.register("my_handler", MyHandler)
        """
        if not isinstance(handler_class, type) or not issubclass(
            handler_class, StepHandler
        ):
            raise ValueError(f"handler_class must be a StepHandler subclass, got {handler_class}")

        if name in self._handlers:
            log_warn(f"Overwriting existing handler: {name}")

        self._handlers[name] = handler_class
        log_info(f"Registered handler: {name} -> {handler_class.__name__}")

    def unregister(self, name: str) -> bool:
        """Unregister a handler.

        Args:
            name: Handler name to unregister.

        Returns:
            True if handler was unregistered, False if not found.

        Example:
            >>> if registry.unregister("old_handler"):
            ...     print("Handler removed")
        """
        if name in self._handlers:
            del self._handlers[name]
            log_debug(f"Unregistered handler: {name}")
            return True
        return False

    def resolve(self, name: str) -> StepHandler | None:
        """Resolve and instantiate a handler by name.

        Args:
            name: Handler name to resolve.

        Returns:
            Instantiated handler or None if not found or instantiation fails.

        Example:
            >>> handler = registry.resolve("my_handler")
            >>> if handler:
            ...     result = handler.call(context)
        """
        handler_class = self._handlers.get(name)
        if handler_class is None:
            log_warn(f"Handler not found: {name}")
            return None

        try:
            return handler_class()
        except Exception as e:
            log_error(f"Failed to instantiate handler {name}: {e}")
            return None

    def get_handler_class(self, name: str) -> type[StepHandler] | None:
        """Get a handler class without instantiation.

        Args:
            name: Handler name to look up.

        Returns:
            Handler class or None if not found.

        Example:
            >>> handler_class = registry.get_handler_class("my_handler")
            >>> if handler_class:
            ...     print(f"Handler version: {handler_class.handler_version}")
        """
        return self._handlers.get(name)

    def is_registered(self, name: str) -> bool:
        """Check if a handler is registered.

        Args:
            name: Handler name to check.

        Returns:
            True if handler is registered.

        Example:
            >>> if registry.is_registered("my_handler"):
            ...     handler = registry.resolve("my_handler")
        """
        return name in self._handlers

    def list_handlers(self) -> list[str]:
        """List all registered handler names.

        Returns:
            List of registered handler names.

        Example:
            >>> handlers = registry.list_handlers()
            >>> print(f"Registered handlers: {handlers}")
        """
        return list(self._handlers.keys())

    def handler_count(self) -> int:
        """Get the number of registered handlers.

        Returns:
            Number of registered handlers.
        """
        return len(self._handlers)

    def clear(self) -> None:
        """Clear all registered handlers.

        Primarily for testing.
        """
        self._handlers.clear()
        log_debug("Cleared all handlers from registry")

    def discover_handlers(
        self,
        package_name: str,
        base_class: type[StepHandler] | None = None,
    ) -> int:
        """Discover and register handlers from a package.

        Scans the specified package and all subpackages for StepHandler
        subclasses that have a handler_name class attribute.

        Args:
            package_name: Package to scan (e.g., "myapp.handlers").
            base_class: Base class to filter by (default: StepHandler).

        Returns:
            Number of handlers discovered and registered.

        Example:
            >>> # Discover all handlers in myapp.handlers package
            >>> count = registry.discover_handlers("myapp.handlers")
            >>> print(f"Discovered {count} handlers")
        """
        base = base_class or StepHandler
        discovered = 0

        try:
            package = importlib.import_module(package_name)
        except ImportError as e:
            log_error(f"Failed to import package {package_name}: {e}")
            return 0

        # Always scan the root package itself first (handlers in __init__.py)
        discovered += self._scan_module_for_handlers(package, base)

        # Get package path - handle case where __path__ doesn't exist
        if not hasattr(package, "__path__"):
            log_info(f"Package {package_name} has no __path__, cannot scan submodules")
            log_info(f"Discovered {discovered} handlers in {package_name}")
            return discovered

        # Also scan submodules if they exist
        for _importer, module_name, _is_pkg in pkgutil.walk_packages(
            package.__path__,
            prefix=f"{package_name}.",
        ):
            try:
                module = importlib.import_module(module_name)
                discovered += self._scan_module_for_handlers(module, base)
            except Exception as e:
                log_warn(f"Failed to scan module {module_name}: {e}")

        log_info(f"Discovered {discovered} handlers in {package_name}")
        return discovered

    def _scan_module_for_handlers(
        self,
        module: Any,
        base: type[StepHandler],
    ) -> int:
        """Scan a module for StepHandler subclasses.

        Args:
            module: The module to scan.
            base: Base class to filter by.

        Returns:
            Number of handlers discovered.
        """
        discovered = 0

        for name in dir(module):
            obj = getattr(module, name)

            # Check if it's a StepHandler subclass
            if not isinstance(obj, type):
                continue

            if not issubclass(obj, base) or obj is base:
                continue

            # Must have handler_name set
            if not getattr(obj, "handler_name", None):
                continue

            handler_name = obj.handler_name
            self.register(handler_name, obj)
            discovered += 1

        return discovered


__all__ = ["StepHandler", "HandlerRegistry"]
