"""TAS-93: Method dispatch wrapper for step handlers.

This module provides the MethodDispatchWrapper class that redirects
.call() invocations to a specified method on the wrapped handler.

When a HandlerDefinition specifies handler_method: "refund", the
ResolverChain wraps the resolved handler with MethodDispatchWrapper
to ensure that calling handler.call(context) actually invokes
handler.refund(context).

Example:
    >>> class PaymentHandler:
    ...     def call(self, context):
    ...         return {"action": "default"}
    ...     def refund(self, context):
    ...         return {"action": "refund"}
    ...
    >>> handler = PaymentHandler()
    >>> wrapped = MethodDispatchWrapper(handler, "refund")
    >>> wrapped.call({})
    {'action': 'refund'}
    >>> wrapped.unwrap()
    <PaymentHandler object>
"""

from __future__ import annotations

from typing import Any


class MethodDispatchWrapper:
    """Wrapper that redirects .call() to a specified method.

    Used when a step template specifies handler_method to invoke
    a different method than the default .call().

    Attributes:
        handler: The wrapped handler instance.
        target_method: The method name to invoke.

    Example:
        >>> wrapped = MethodDispatchWrapper(my_handler, "process")
        >>> wrapped.call(context)  # Actually calls my_handler.process(context)
    """

    def __init__(self, handler: Any, target_method: str) -> None:
        """Initialize the wrapper.

        Args:
            handler: The handler instance to wrap.
            target_method: The method name to invoke instead of .call().

        Raises:
            AttributeError: If handler doesn't have the target method.
        """
        if not hasattr(handler, target_method):
            raise AttributeError(
                f"Handler {handler.__class__.__name__} does not have method '{target_method}'"
            )

        self._handler = handler
        self._target_method = target_method
        self._method = getattr(handler, target_method)

    @property
    def handler(self) -> Any:
        """Get the wrapped handler.

        Returns:
            The wrapped handler instance.
        """
        return self._handler

    @property
    def target_method(self) -> str:
        """Get the target method name.

        Returns:
            The method name being invoked.
        """
        return self._target_method

    def call(self, context: Any) -> Any:
        """Invoke the target method on the wrapped handler.

        This redirects the standard .call() interface to the
        configured target method.

        Args:
            context: The step context to pass to the handler.

        Returns:
            The result from the target method.
        """
        return self._method(context)

    def unwrap(self) -> Any:
        """Get the original unwrapped handler.

        Useful for testing and debugging.

        Returns:
            The wrapped handler instance.
        """
        return self._handler

    def __getattr__(self, name: str) -> Any:
        """Delegate attribute access to the wrapped handler.

        Allows transparent access to handler attributes.

        Args:
            name: Attribute name.

        Returns:
            The attribute value from the wrapped handler.
        """
        return getattr(self._handler, name)

    def __repr__(self) -> str:
        """Return string representation.

        Returns:
            String representation showing wrapped handler and method.
        """
        return (
            f"MethodDispatchWrapper("
            f"{self._handler.__class__.__name__}, "
            f"target_method={self._target_method!r})"
        )
