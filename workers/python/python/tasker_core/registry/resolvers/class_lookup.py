"""TAS-93: Class lookup resolver (priority 100).

This resolver infers handler classes from callable strings using
Python's importlib. It handles module.path.ClassName format.

Example:
    >>> resolver = ClassLookupResolver()
    >>>
    >>> # Resolve "myapp.handlers.PaymentHandler"
    >>> definition = HandlerDefinition(callable="myapp.handlers.PaymentHandler")
    >>> handler = resolver.resolve(definition)
    >>> assert handler.__class__.__name__ == "PaymentHandler"
"""

from __future__ import annotations

import importlib
import re
from typing import TYPE_CHECKING, Any

from ..base_resolver import BaseResolver

if TYPE_CHECKING:
    from ..handler_definition import HandlerDefinition


class ClassLookupResolver(BaseResolver):
    """Resolver that infers handler classes from callable strings.

    Priority 100 - checked last in the default chain (inferential).

    Handles callable formats like:
    - "module.ClassName" → import module, get ClassName
    - "package.submodule.ClassName" → import package.submodule, get ClassName

    The callable must look like a Python class path (contains at least
    one dot and ends with a capitalized component).
    """

    # Pattern to match class-like callables: module.path.ClassName
    # Must have at least one dot and end with a capitalized identifier
    CLASS_PATTERN = re.compile(
        r"^[a-zA-Z_][a-zA-Z0-9_]*(\.[a-zA-Z_][a-zA-Z0-9_]*)*\.[A-Z][a-zA-Z0-9_]*$"
    )

    @property
    def name(self) -> str:
        """Return the resolver name."""
        return "class_lookup"

    @property
    def priority(self) -> int:
        """Return the resolver priority (100 = inferential)."""
        return 100

    def can_resolve(
        self,
        definition: HandlerDefinition,
        _config: dict[str, Any] | None = None,
    ) -> bool:
        """Check if callable looks like a class path.

        Args:
            definition: Handler configuration.
            _config: Additional context (unused, part of interface).

        Returns:
            True if callable matches class path pattern.
        """
        return bool(self.CLASS_PATTERN.match(definition.callable))

    def resolve(
        self,
        definition: HandlerDefinition,
        config: dict[str, Any] | None = None,
    ) -> Any | None:
        """Resolve handler by importing module and getting class.

        Args:
            definition: Handler configuration.
            config: Additional context.

        Returns:
            Handler instance or None if not found.
        """
        handler_class = self._import_class(definition.callable)
        if handler_class is None:
            return None

        return self._instantiate_handler(handler_class, definition, config or {})

    def _import_class(self, callable_str: str) -> type | None:
        """Import a class from a module path string.

        Args:
            callable_str: Full class path (e.g., "module.ClassName").

        Returns:
            The class or None if not found.
        """
        if "." not in callable_str:
            return None

        # Split into module path and class name
        parts = callable_str.rsplit(".", 1)
        if len(parts) != 2:
            return None

        module_path, class_name = parts

        try:
            module = importlib.import_module(module_path)
            handler_class = getattr(module, class_name, None)

            if handler_class is None:
                return None

            if not isinstance(handler_class, type):
                return None

            return handler_class

        except ImportError:
            # Module not found - expected for non-existent paths
            return None
        except Exception:
            # Other import errors (syntax, etc.) - silently fail
            # as this is an inferential resolver; explicit registration
            # should be used if the handler must succeed
            return None

    def _instantiate_handler(
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
            # Handler requires arguments - fall through to try with config
            pass
        except Exception:
            # Instantiation error (not just missing args) - fail silently
            # as this is an inferential resolver
            return None

        # Second try: instantiate with config keyword argument
        try:
            return handler_class(config=definition.initialization or {})
        except Exception:
            # Instantiation failed - handler may have incompatible constructor
            # Fail silently; use explicit registration for handlers that must succeed
            return None
