"""TAS-93: Abstract base class for step handler resolvers.

This module defines the contract that all resolvers must implement.
Resolvers are tried in priority order by the ResolverChain until
one successfully resolves the handler.

Resolution Contract:
1. name - Human-readable identifier for logging/debugging
2. priority - Lower numbers = tried first (10 = explicit, 100 = inferential)
3. can_resolve() - Quick check if this resolver might handle the definition
4. resolve() - Actually resolve the handler callable

Example Implementation:
    class MyResolver(BaseResolver):
        @property
        def name(self) -> str:
            return "my_custom_resolver"

        @property
        def priority(self) -> int:
            return 50  # Between explicit (10) and class lookup (100)

        def can_resolve(self, definition: HandlerDefinition, config: dict) -> bool:
            return definition.callable.startswith("MyApp.")

        def resolve(self, definition: HandlerDefinition, config: dict) -> Any | None:
            # Return handler or None
            pass
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from .handler_definition import HandlerDefinition


class BaseResolver(ABC):
    """Abstract base class for step handler resolvers.

    Defines the contract that all resolvers must implement.
    Resolvers are tried in priority order by the ResolverChain.
    """

    @property
    @abstractmethod
    def name(self) -> str:
        """Human-readable name for this resolver (for logging/debugging).

        Returns:
            The resolver name.
        """
        ...

    @property
    @abstractmethod
    def priority(self) -> int:
        """Resolution priority (lower = tried first).

        Standard priorities:
        - 10: Explicit mapping (registered handlers)
        - 100: Class lookup (inferential)

        Returns:
            The priority value.
        """
        ...

    @abstractmethod
    def can_resolve(
        self, definition: HandlerDefinition, config: dict[str, Any] | None = None
    ) -> bool:
        """Quick eligibility check (called before resolve).

        This should be a fast check to determine if this resolver
        is even worth trying for the given definition.

        Args:
            definition: Handler configuration.
            config: Additional configuration context.

        Returns:
            True if this resolver might be able to resolve the handler.
        """
        ...

    @abstractmethod
    def resolve(
        self, definition: HandlerDefinition, config: dict[str, Any] | None = None
    ) -> Any | None:
        """Resolve the handler callable from the definition.

        Args:
            definition: Handler configuration.
            config: Additional configuration context.

        Returns:
            Handler object or None if cannot resolve.
        """
        ...

    def registered_callables(self) -> list[str]:
        """Return all callable keys this resolver knows about.

        Used for debugging and introspection.

        Returns:
            List of registered callable identifiers.
        """
        return []
