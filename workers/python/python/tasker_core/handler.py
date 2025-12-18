"""Step handler base class and registry for handler discovery.

This module provides the StepHandler abstract base class that all step
handlers must inherit from, and the HandlerRegistry for registering
and resolving handlers.

Note:
    StepHandler is now defined in tasker_core.step_handler.base and
    re-exported here for backwards compatibility. New specialized handlers
    (ApiHandler, DecisionHandler) are available in tasker_core.step_handler.

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
import os
import pkgutil
import sys
from pathlib import Path
from typing import TYPE_CHECKING, Any

from .logging import log_debug, log_error, log_info, log_warn

# Import StepHandler from its new canonical location
from .step_handler.base import StepHandler

if TYPE_CHECKING:
    pass


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
        self._bootstrapped: bool = False

    @classmethod
    def instance(cls, skip_bootstrap: bool = False) -> HandlerRegistry:
        """Get the singleton registry instance.

        On first access, automatically bootstraps handlers from the
        environment-appropriate source (test handlers or YAML templates).

        Parameters
        ----------
        skip_bootstrap : bool, optional
            Skip automatic handler bootstrap (for tests), by default False.

        Returns
        -------
        HandlerRegistry
            The singleton HandlerRegistry instance.

        Example:
            >>> registry = HandlerRegistry.instance()
            >>> assert registry is HandlerRegistry.instance()
        """
        if cls._instance is None:
            cls._instance = cls()
            if not skip_bootstrap:
                cls._instance.bootstrap_handlers()
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
        if not isinstance(handler_class, type) or not issubclass(handler_class, StepHandler):
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
        """Clear all registered handlers and reset bootstrap state.

        Primarily for testing.
        """
        self._handlers.clear()
        self._bootstrapped = False
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

            try:
                if not issubclass(obj, base):
                    continue
            except TypeError:
                continue

            if obj is base:
                continue

            # Must have handler_name set
            if not getattr(obj, "handler_name", None):
                continue

            handler_name = obj.handler_name
            self.register(handler_name, obj)
            discovered += 1

        return discovered

    # =========================================================================
    # Bootstrap Methods (TAS-88: Template-driven discovery)
    # =========================================================================

    def bootstrap_handlers(self) -> int:
        """Bootstrap handlers from environment-appropriate source.

        Discovery priority:
        1. Test environment with preloaded handlers -> use those
        2. YAML template-driven discovery

        Returns
        -------
        int
            Number of handlers registered.

        Notes
        -----
        This method is called automatically on first `instance()` access.
        To skip auto-bootstrap (e.g., for tests), use `instance(skip_bootstrap=True)`.
        """
        if self._bootstrapped:
            return len(self._handlers)

        log_info("Bootstrapping Python handler registry with template-driven discovery")

        registered_count = 0

        # Check for test environment with preloaded handlers
        if self._test_environment_active():
            log_info("Test environment detected, checking for preloaded handlers")
            registered_count = self._register_preloaded_handlers()

        # If no handlers registered yet, discover from templates
        if registered_count == 0:
            registered_count = self._discover_handlers_from_templates()

        self._bootstrapped = True
        log_info(f"Handler registry bootstrapped with {registered_count} handlers")
        return registered_count

    def _test_environment_active(self) -> bool:
        """Check if running in test environment."""
        return os.environ.get("TASKER_ENV") == "test"

    def _register_preloaded_handlers(self) -> int:
        """Register handlers that are already loaded in memory.

        Used in test environments where handlers are imported before
        registry bootstrap.

        Returns
        -------
        int
            Number of handlers registered.
        """
        registered_count = 0

        # Scan loaded modules for StepHandler subclasses
        for module_name, module in list(sys.modules.items()):
            if module is None:
                continue

            # Skip standard library and common packages
            if module_name.startswith(("_", "builtins", "typing")):
                continue

            # Only scan relevant packages (test handlers, etc.)
            if not any(
                term in module_name
                for term in ("handlers", "step_handler", "workflow", "test")
            ):
                continue

            try:
                registered_count += self._scan_module_for_handlers(module, StepHandler)  # type: ignore[type-abstract]
            except Exception:
                # Skip modules that can't be scanned
                continue

        if registered_count > 0:
            log_info(f"Registered {registered_count} preloaded test handlers")

        return registered_count

    def _discover_handlers_from_templates(self) -> int:
        """Discover handlers from YAML template files.

        Returns
        -------
        int
            Number of handlers registered.
        """
        # Import here to avoid circular imports
        from .template_discovery import HandlerDiscovery

        template_path = self._determine_template_path()

        if not template_path:
            log_warn("No template directory found, no handlers will be discovered")
            return 0

        log_debug(f"Discovering handlers from template path: {template_path}")

        # Get all handler callables from templates
        handler_callables = HandlerDiscovery.discover_all_handlers(template_path)

        registered_count = 0
        for callable_name in handler_callables:
            handler_class = self._find_and_load_handler_class(callable_name)
            if handler_class:
                # Get handler_name from the class
                handler_name = getattr(handler_class, "handler_name", None)
                if handler_name:
                    self.register(handler_name, handler_class)
                    registered_count += 1
                else:
                    log_debug(f"Handler {callable_name} has no handler_name, skipping")
            else:
                log_debug(f"Handler class not found for: {callable_name}")

        return registered_count

    def _determine_template_path(self) -> Path | None:
        """Determine the template path based on environment.

        Returns
        -------
        Path | None
            Path to template directory, or None if not found.
        """
        from .template_discovery import TemplatePath

        # 1. Explicit override takes highest priority
        env_path = os.environ.get("TASKER_TEMPLATE_PATH")
        if env_path:
            path = Path(env_path)
            if path.is_dir():
                return path

        # 2. Test environment: use test fixtures
        if self._test_environment_active():
            return TemplatePath._find_test_template_path()

        # 3. Standard discovery
        return TemplatePath.find_template_config_directory()

    def _find_and_load_handler_class(
        self,
        callable_name: str,
    ) -> type[StepHandler] | None:
        """Find and load a handler class by its callable name.

        Parameters
        ----------
        callable_name : str
            The callable name from template (e.g., "module.ClassName").

        Returns
        -------
        type[StepHandler] | None
            The handler class, or None if not found.

        Notes
        -----
        Attempts to import the module and get the class. The callable_name
        should be in format "module.submodule.ClassName".
        """
        if not callable_name or "." not in callable_name:
            return None

        # Split into module path and class name
        parts = callable_name.rsplit(".", 1)
        if len(parts) != 2:
            return None

        module_path, class_name = parts

        try:
            # Try to import the module
            module = importlib.import_module(module_path)

            # Get the class from the module
            handler_class = getattr(module, class_name, None)

            if handler_class is None:
                log_debug(f"Class {class_name} not found in module {module_path}")
                return None

            # Verify it's a StepHandler subclass
            if not isinstance(handler_class, type):
                return None

            if not issubclass(handler_class, StepHandler):
                log_debug(f"Class {class_name} is not a StepHandler subclass")
                return None

            return handler_class

        except ImportError as e:
            log_debug(f"Failed to import module {module_path}: {e}")
            return None
        except Exception as e:
            log_debug(f"Error loading handler {callable_name}: {e}")
            return None

    def template_discovery_info(self) -> dict[str, Any]:
        """Get template discovery information for debugging.

        Returns
        -------
        dict[str, Any]
            Information about template discovery state.
        """
        from .template_discovery import HandlerDiscovery, TemplatePath

        template_path = self._determine_template_path()

        return {
            "template_path": str(template_path) if template_path else None,
            "template_files": (
                [str(f) for f in TemplatePath.discover_template_files(template_path)]
                if template_path
                else []
            ),
            "discovered_handlers": (
                HandlerDiscovery.discover_all_handlers(template_path)
                if template_path
                else []
            ),
            "handlers_by_namespace": (
                HandlerDiscovery.discover_handlers_by_namespace(template_path)
                if template_path
                else {}
            ),
            "environment": os.environ.get("TASKER_ENV", "development"),
            "bootstrapped": self._bootstrapped,
        }


__all__ = ["StepHandler", "HandlerRegistry"]
