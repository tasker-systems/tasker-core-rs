"""TAS-93: Explicit mapping resolver (priority 10).

This resolver handles explicitly registered handlers with exact key matching.
It's the highest priority resolver in the default chain.

Example:
    >>> resolver = ExplicitMappingResolver()
    >>> resolver.register("my_handler", MyHandlerClass)
    >>>
    >>> definition = HandlerDefinition(callable="my_handler")
    >>> handler = resolver.resolve(definition)
    >>> assert isinstance(handler, MyHandlerClass)
"""

from __future__ import annotations

import threading
from typing import TYPE_CHECKING, Any

from ..base_resolver import BaseResolver

if TYPE_CHECKING:
    from ..handler_definition import HandlerDefinition


class ExplicitMappingResolver(BaseResolver):
    """Resolver for explicitly registered handlers.

    Priority 10 - checked first in the default chain.

    Supports registering:
    - Handler classes (instantiated on resolve)
    - Handler instances (returned directly)
    - Factory functions (called with config on resolve)

    Thread-safe for concurrent registration and resolution.
    """

    def __init__(self, name: str = "explicit_mapping") -> None:
        """Initialize the resolver.

        Args:
            name: Resolver name for identification.
        """
        self._name = name
        self._handlers: dict[str, Any] = {}
        self._lock = threading.RLock()

    @property
    def name(self) -> str:
        """Return the resolver name."""
        return self._name

    @property
    def priority(self) -> int:
        """Return the resolver priority (10 = highest)."""
        return 10

    def can_resolve(
        self,
        definition: HandlerDefinition,
        _config: dict[str, Any] | None = None,
    ) -> bool:
        """Check if this resolver has the handler registered.

        Args:
            definition: Handler configuration.
            _config: Additional context (unused, part of interface).

        Returns:
            True if handler is registered.
        """
        return definition.callable in self._handlers

    def resolve(
        self,
        definition: HandlerDefinition,
        config: dict[str, Any] | None = None,
    ) -> Any | None:
        """Resolve and instantiate a registered handler.

        Args:
            definition: Handler configuration.
            config: Additional context passed to factories.

        Returns:
            Handler instance or None if not registered.
        """
        entry = self._handlers.get(definition.callable)
        if entry is None:
            return None

        return self._instantiate_handler(entry, definition, config or {})

    def register(self, key: str, handler: Any) -> None:
        """Register a handler.

        Args:
            key: Handler identifier (matched against definition.callable).
            handler: Handler class, instance, or factory function.
        """
        with self._lock:
            self._handlers[key] = handler

    def unregister(self, key: str) -> bool:
        """Unregister a handler.

        Args:
            key: Handler identifier to remove.

        Returns:
            True if handler was removed, False if not found.
        """
        with self._lock:
            if key in self._handlers:
                del self._handlers[key]
                return True
            return False

    def registered_callables(self) -> list[str]:
        """Return all registered handler keys.

        Returns:
            List of registered keys.
        """
        return list(self._handlers.keys())

    def _instantiate_handler(
        self,
        entry: Any,
        definition: HandlerDefinition,
        config: dict[str, Any],
    ) -> Any:
        """Instantiate a handler from the registered entry.

        Args:
            entry: Registered handler (class, instance, or factory).
            definition: Handler configuration.
            config: Additional context.

        Returns:
            Handler instance.
        """
        # If it's a class, instantiate it
        if isinstance(entry, type):
            return self._instantiate_class(entry, definition, config)

        # If it's a callable (factory function), call it
        if callable(entry) and not hasattr(entry, "call"):
            return entry(config)

        # Otherwise assume it's already an instance
        return entry

    def _instantiate_class(
        self,
        handler_class: type,
        definition: HandlerDefinition,
        _config: dict[str, Any],
    ) -> Any | None:
        """Instantiate a handler class.

        Tries instantiation without args first, then with config if needed.

        Args:
            handler_class: Handler class to instantiate.
            definition: Handler configuration.
            _config: Additional context (unused, uses definition.initialization).

        Returns:
            Handler instance or None if instantiation fails.
        """
        # First try: instantiate without arguments (most common case)
        try:
            return handler_class()
        except TypeError:
            # May need config - try with it
            pass
        except Exception:
            # Other error - fail
            return None

        # Second try: instantiate with config
        try:
            return handler_class(config=definition.initialization or {})
        except Exception:
            # Instantiation failed - return None
            return None
