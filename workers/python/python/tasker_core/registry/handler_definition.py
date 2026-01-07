"""TAS-93: Handler definition type for resolver chain.

This module defines the HandlerDefinition dataclass that carries all
information needed to resolve and dispatch a step handler.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any


@dataclass
class HandlerDefinition:
    """Handler definition for resolver chain resolution.

    Contains all information needed to resolve and invoke a step handler:
    - callable: The handler identifier (class name, key, or custom format)
    - handler_method: Optional method to invoke instead of .call()
    - resolver: Optional resolver hint to bypass chain traversal
    - initialization: Configuration passed to handler constructor

    Attributes:
        callable: Handler identifier string (e.g., "PaymentHandler" or "module.ClassName")
        handler_method: Method to invoke instead of default .call() (TAS-93)
        resolver: Resolver hint to bypass chain and use specific resolver (TAS-93)
        initialization: Configuration dictionary passed to handler constructor

    Example:
        >>> definition = HandlerDefinition(
        ...     callable="PaymentHandler",
        ...     handler_method="refund",
        ...     initialization={"api_key": "secret"},
        ... )
        >>> definition.uses_method_dispatch()
        True
        >>> definition.effective_method()
        'refund'
    """

    callable: str
    handler_method: str | None = None
    resolver: str | None = None
    initialization: dict[str, Any] = field(default_factory=dict)

    def effective_method(self) -> str:
        """Get the effective method name to invoke.

        Returns the configured method name or defaults to 'call'.

        Returns:
            The method name to invoke on the handler.

        Example:
            >>> HandlerDefinition(callable="X").effective_method()
            'call'
            >>> HandlerDefinition(callable="X", handler_method="process").effective_method()
            'process'
        """
        return self.handler_method or "call"

    def uses_method_dispatch(self) -> bool:
        """Check if this handler uses custom method dispatch.

        Returns True if a non-default method is configured.

        Returns:
            True if handler_method is something other than 'call' or None.

        Example:
            >>> HandlerDefinition(callable="X").uses_method_dispatch()
            False
            >>> HandlerDefinition(callable="X", handler_method="call").uses_method_dispatch()
            False
            >>> HandlerDefinition(callable="X", handler_method="refund").uses_method_dispatch()
            True
        """
        return self.handler_method is not None and self.handler_method != "call"

    def has_resolver_hint(self) -> bool:
        """Check if a resolver hint is specified.

        When True, the resolver chain should skip inferential resolution
        and directly use the named resolver.

        Returns:
            True if resolver hint is present.

        Example:
            >>> HandlerDefinition(callable="X").has_resolver_hint()
            False
            >>> HandlerDefinition(callable="X", resolver="explicit_mapping").has_resolver_hint()
            True
        """
        return self.resolver is not None and len(self.resolver) > 0

    @classmethod
    def from_callable(cls, callable_str: str) -> HandlerDefinition:
        """Create a minimal definition from just a callable string.

        Args:
            callable_str: Handler callable identifier.

        Returns:
            HandlerDefinition with only callable set.

        Example:
            >>> definition = HandlerDefinition.from_callable("PaymentHandler")
            >>> definition.callable
            'PaymentHandler'
        """
        return cls(callable=callable_str)

    @classmethod
    def from_dict(cls, data: dict[str, Any] | None) -> HandlerDefinition:
        """Create a HandlerDefinition from FFI dictionary data.

        Handles the conversion from Rust FFI where the field is named
        'method' (to avoid Rust keyword issues) to 'handler_method'.

        Args:
            data: Handler definition dictionary from FFI.

        Returns:
            HandlerDefinition instance.

        Example:
            >>> data = {"callable": "X", "method": "refund", "resolver": "explicit"}
            >>> definition = HandlerDefinition.from_dict(data)
            >>> definition.handler_method
            'refund'
        """
        if data is None:
            data = {}
        return cls(
            callable=data.get("callable", ""),
            # TAS-93: Rust uses 'method' field, we use 'handler_method' internally
            handler_method=data.get("method"),
            resolver=data.get("resolver"),
            initialization=data.get("initialization") or {},
        )
