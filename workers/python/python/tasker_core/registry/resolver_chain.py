"""TAS-93: Resolver Chain - Priority-Ordered Handler Resolution.

The ResolverChain orchestrates handler resolution by trying resolvers
in priority order until one successfully resolves the handler.

Resolution Contract:
1. If HandlerDefinition has `resolver` hint, use ONLY that resolver
2. Otherwise, try resolvers in priority order (lower = first)
3. Wrap resolved handler with MethodDispatchWrapper if needed
4. Return None if no resolver can handle the definition

Default Chain (when using .default()):
- Priority 10:  ExplicitMappingResolver - registered handlers
- Priority 100: ClassLookupResolver     - class path inference

Usage:
    # Create default chain
    chain = ResolverChain.default()

    # Or build custom chain
    chain = ResolverChain()
    chain.add_resolver(MyResolver())
    chain.add_resolver(AnotherResolver())

    # Resolve a handler
    handler = chain.resolve(definition, config)

Adding Custom Resolvers:
    chain.add_resolver(CustomResolver())

Named Resolver Access:
    # Definition with resolver hint
    definition = HandlerDefinition(callable="x", resolver="payment_resolver")
    # Chain will ONLY use PaymentResolver
"""

from __future__ import annotations

import threading
from typing import TYPE_CHECKING, Any

from ..logging import log_debug, log_warn
from .method_dispatch_wrapper import MethodDispatchWrapper

if TYPE_CHECKING:
    from .base_resolver import BaseResolver
    from .handler_definition import HandlerDefinition


class ResolverNotFoundError(Exception):
    """Error raised when resolver hint points to unknown resolver."""

    pass


class ResolutionError(Exception):
    """Error raised when resolution fails completely."""

    pass


class ResolverChain:
    """Priority-ordered chain of handler resolvers.

    Orchestrates handler resolution by trying resolvers in priority order.
    Supports resolver hints to bypass the chain and use a specific resolver.

    Attributes:
        resolvers: List of resolvers in priority order.
        resolvers_by_name: Mapping of resolver names to resolvers.
    """

    def __init__(self) -> None:
        """Initialize an empty resolver chain."""
        self._resolvers: list[BaseResolver] = []
        self._resolvers_by_name: dict[str, BaseResolver] = {}
        self._lock = threading.RLock()

    @classmethod
    def default(cls) -> ResolverChain:
        """Create a chain with default resolvers.

        Returns:
            Chain with ExplicitMapping + ClassLookup resolvers.
        """
        from .resolvers import ClassLookupResolver, ExplicitMappingResolver

        chain = cls()
        chain.add_resolver(ExplicitMappingResolver())
        chain.add_resolver(ClassLookupResolver())
        return chain

    def add_resolver(self, resolver: BaseResolver) -> ResolverChain:
        """Add a resolver to the chain.

        Resolvers are automatically sorted by priority (lower = first).

        Args:
            resolver: Resolver to add.

        Returns:
            Self for method chaining.
        """
        with self._lock:
            self._resolvers.append(resolver)
            self._resolvers.sort(key=lambda r: r.priority)
            self._resolvers_by_name[resolver.name] = resolver
        return self

    def remove_resolver(self, name: str) -> BaseResolver | None:
        """Remove a resolver by name.

        Args:
            name: Resolver name to remove.

        Returns:
            Removed resolver or None if not found.
        """
        with self._lock:
            resolver = self._resolvers_by_name.pop(name, None)
            if resolver:
                self._resolvers.remove(resolver)
            return resolver

    def get_resolver(self, name: str) -> BaseResolver | None:
        """Get a resolver by name.

        Args:
            name: Resolver name.

        Returns:
            Resolver or None if not found.
        """
        return self._resolvers_by_name.get(name)

    @property
    def explicit_resolver(self) -> BaseResolver | None:
        """Get the explicit mapping resolver (convenience method).

        Returns:
            ExplicitMappingResolver or None.
        """
        return self._resolvers_by_name.get("explicit_mapping")

    def resolve(
        self,
        definition: HandlerDefinition,
        config: dict[str, Any] | None = None,
    ) -> Any | None:
        """Resolve a handler from the definition.

        Args:
            definition: Handler configuration.
            config: Additional context.

        Returns:
            Resolved handler or None.

        Raises:
            ResolverNotFoundError: If resolver hint points to unknown resolver.
        """
        config = config or {}

        # Contract: If resolver hint is present, use ONLY that resolver
        if definition.has_resolver_hint():
            return self._resolve_with_hint(definition, config)

        # Otherwise: Try inferential chain resolution
        return self._resolve_with_chain(definition, config)

    def can_resolve(
        self,
        definition: HandlerDefinition,
        config: dict[str, Any] | None = None,
    ) -> bool:
        """Check if any resolver can handle this definition.

        Args:
            definition: Handler configuration.
            config: Additional context.

        Returns:
            True if definition can be resolved.
        """
        config = config or {}

        if definition.has_resolver_hint():
            resolver = self._resolvers_by_name.get(definition.resolver or "")
            return resolver.can_resolve(definition, config) if resolver else False

        return any(r.can_resolve(definition, config) for r in self._resolvers)

    def registered_callables(self) -> list[str]:
        """Get all registered callables across all resolvers.

        Returns:
            List of all registered callable identifiers.
        """
        callables: list[str] = []
        for resolver in self._resolvers:
            callables.extend(resolver.registered_callables())
        return list(set(callables))

    def register(self, key: str, handler: Any) -> ResolverChain:
        """Register a handler directly (convenience method for ExplicitMappingResolver).

        Args:
            key: Handler key.
            handler: Handler class or instance.

        Returns:
            Self for method chaining.

        Raises:
            RuntimeError: If no ExplicitMappingResolver in chain.
        """
        explicit = self.explicit_resolver
        if explicit is None:
            raise RuntimeError("No ExplicitMappingResolver in chain")

        # Access the register method - we know it exists on ExplicitMappingResolver
        explicit.register(key, handler)  # type: ignore[attr-defined]
        return self

    def chain_info(self) -> list[dict[str, Any]]:
        """Get chain info for debugging.

        Returns:
            List of resolver info dicts.
        """
        return [
            {
                "name": resolver.name,
                "priority": resolver.priority,
                "callables": len(resolver.registered_callables()),
            }
            for resolver in self._resolvers
        ]

    def __len__(self) -> int:
        """Return number of resolvers in chain."""
        return len(self._resolvers)

    @property
    def resolver_names(self) -> list[str]:
        """Get names of resolvers in priority order.

        Returns:
            List of resolver names.
        """
        return [r.name for r in self._resolvers]

    def list_resolvers(self) -> list[tuple[str, int]]:
        """List resolvers with their priorities.

        Returns:
            List of (name, priority) tuples in priority order.
        """
        return [(r.name, r.priority) for r in self._resolvers]

    def _resolve_with_hint(
        self,
        definition: HandlerDefinition,
        config: dict[str, Any],
    ) -> Any | None:
        """Resolve using explicit resolver hint (bypasses chain).

        Args:
            definition: Handler configuration.
            config: Additional context.

        Returns:
            Resolved handler or None.

        Raises:
            ResolverNotFoundError: If resolver hint points to unknown resolver.
        """
        resolver_name = definition.resolver or ""
        resolver = self._resolvers_by_name.get(resolver_name)

        if resolver is None:
            log_warn(f"ResolverChain: Unknown resolver hint '{resolver_name}'")
            raise ResolverNotFoundError(f"Unknown resolver: '{resolver_name}'")

        handler = resolver.resolve(definition, config)

        if handler is None:
            log_warn(
                f"ResolverChain: Resolver '{resolver_name}' failed to resolve '{definition.callable}'"
            )
            return None

        log_debug(f"ResolverChain: Resolved '{definition.callable}' via hint '{resolver_name}'")
        return self.wrap_for_method_dispatch(handler, definition)

    def _resolve_with_chain(
        self,
        definition: HandlerDefinition,
        config: dict[str, Any],
    ) -> Any | None:
        """Resolve using chain (tries resolvers in priority order).

        Args:
            definition: Handler configuration.
            config: Additional context.

        Returns:
            Resolved handler or None.
        """
        for resolver in self._resolvers:
            if not resolver.can_resolve(definition, config):
                continue

            handler = resolver.resolve(definition, config)
            if handler is None:
                continue

            log_debug(f"ResolverChain: Resolved '{definition.callable}' via '{resolver.name}'")
            return self.wrap_for_method_dispatch(handler, definition)

        log_debug(f"ResolverChain: No resolver could handle '{definition.callable}'")
        return None

    def wrap_for_method_dispatch(
        self,
        handler: Any,
        definition: HandlerDefinition,
    ) -> Any:
        """Wrap handler for method dispatch if needed.

        Args:
            handler: Resolved handler.
            definition: Handler configuration.

        Returns:
            Handler (possibly wrapped).
        """
        # No wrapping needed if using default .call method
        if not definition.uses_method_dispatch():
            return handler

        effective_method = definition.effective_method()

        # Check if handler has the target method
        if not hasattr(handler, effective_method):
            log_warn(
                f"ResolverChain: Handler {handler.__class__.__name__} "
                f"doesn't have method '{effective_method}'"
            )
            return None

        log_debug(
            f"ResolverChain: Wrapping {handler.__class__.__name__} "
            f"for method dispatch → {effective_method}"
        )
        return MethodDispatchWrapper(handler, effective_method)
