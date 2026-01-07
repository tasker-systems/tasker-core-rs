r"""TAS-93: Developer-friendly base class for custom resolvers.

This module provides the RegistryResolver class, a more user-friendly
base class for implementing custom resolvers with pattern matching.

Example:
    from tasker_core.registry import RegistryResolver

    class PaymentResolver(RegistryResolver):
        _name = "payment_resolver"
        _priority = 20
        pattern = r"^payments:(?P<provider>\w+):(?P<action>\w+)$"

        def resolve_handler(self, definition, config):
            match = self._match_pattern(definition.callable)
            if not match:
                return None
            provider = match.group("provider")
            action = match.group("action")
            return PaymentHandlers.get(provider, action)
"""

from __future__ import annotations

import re
from typing import TYPE_CHECKING, Any, ClassVar

from .base_resolver import BaseResolver

if TYPE_CHECKING:
    from .handler_definition import HandlerDefinition


class RegistryResolver(BaseResolver):
    """Developer-friendly base class for custom resolvers.

    Provides pattern matching and prefix matching capabilities.
    Subclasses should override:
    - name: Resolver identifier
    - priority: Resolution priority (lower = first)
    - pattern or prefix: Matching criteria
    - resolve_handler(): Actual resolution logic

    Class Attributes:
        _name: Human-readable resolver name (required).
        _priority: Resolution priority, default 50.
        pattern: Regex pattern to match callables (optional).
        prefix: String prefix to match callables (optional).
    """

    # Subclasses should override these
    _name: ClassVar[str] = "custom_resolver"
    _priority: ClassVar[int] = 50
    pattern: ClassVar[str | None] = None
    prefix: ClassVar[str | None] = None

    def __init__(self) -> None:
        """Initialize the resolver."""
        self._compiled_pattern: re.Pattern[str] | None = None
        if self.pattern:
            self._compiled_pattern = re.compile(self.pattern)

    @property
    def name(self) -> str:
        """Return the resolver name."""
        return self.__class__._name

    @property
    def priority(self) -> int:
        """Return the resolver priority."""
        return self.__class__._priority

    def can_resolve(
        self,
        definition: HandlerDefinition,
        _config: dict[str, Any] | None = None,
    ) -> bool:
        """Check if this resolver can handle the definition.

        Uses pattern or prefix matching if configured.

        Args:
            definition: Handler configuration.
            _config: Additional context (unused, part of interface).

        Returns:
            True if callable matches pattern or prefix.
        """
        callable_str = definition.callable

        if self._compiled_pattern:
            return bool(self._compiled_pattern.match(callable_str))

        if self.prefix:
            return callable_str.startswith(self.prefix)

        return False

    def resolve(
        self,
        definition: HandlerDefinition,
        config: dict[str, Any] | None = None,
    ) -> Any | None:
        """Resolve the handler.

        Delegates to resolve_handler() for subclass implementation.

        Args:
            definition: Handler configuration.
            config: Additional context.

        Returns:
            Handler object or None.
        """
        return self.resolve_handler(definition, config or {})

    def resolve_handler(
        self,
        definition: HandlerDefinition,
        config: dict[str, Any],
    ) -> Any | None:
        """Override this in subclasses for custom resolution logic.

        Args:
            definition: Handler configuration.
            config: Additional context.

        Returns:
            Handler object or None.

        Raises:
            NotImplementedError: If not overridden.
        """
        raise NotImplementedError(
            f"{self.__class__.__name__}.resolve_handler() must be implemented"
        )

    def _match_pattern(self, callable_str: str) -> re.Match[str] | None:
        """Match the callable against the configured pattern.

        Convenience method for subclasses.

        Args:
            callable_str: The callable string to match.

        Returns:
            Match object or None.
        """
        if self._compiled_pattern:
            return self._compiled_pattern.match(callable_str)
        return None
