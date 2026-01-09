r"""TAS-93: Step Handler Resolver Infrastructure.

This module provides the resolver chain pattern for step handler resolution.
Resolvers are tried in priority order until one successfully resolves.

Built-in Resolvers:
- ExplicitMappingResolver (priority 10): Explicit key → handler mappings
- ClassLookupResolver (priority 100): Class path inference via importlib

Custom Resolvers:
Extend RegistryResolver for developer-friendly custom resolution:

    from tasker_core.registry import RegistryResolver

    class PaymentResolver(RegistryResolver):
        name = "payment_resolver"
        priority = 50
        pattern = r"^payments:(?P<provider>\w+):(?P<action>\w+)$"

        def resolve_handler(self, definition, config):
            # Custom resolution logic
            pass

Method Dispatch:
When HandlerDefinition specifies `handler_method: refund`, the chain
automatically wraps the handler with MethodDispatchWrapper to redirect
.call() → .refund()
"""

from __future__ import annotations

from .base_resolver import BaseResolver
from .handler_definition import HandlerDefinition
from .method_dispatch_wrapper import MethodDispatchWrapper
from .registry_resolver import RegistryResolver
from .resolver_chain import ResolutionError, ResolverChain, ResolverNotFoundError
from .resolvers import ClassLookupResolver, ExplicitMappingResolver

__all__ = [
    # Core types
    "HandlerDefinition",
    # Resolver base classes
    "BaseResolver",
    "RegistryResolver",
    # Resolver chain
    "ResolverChain",
    "ResolverNotFoundError",
    "ResolutionError",
    # Built-in resolvers
    "ExplicitMappingResolver",
    "ClassLookupResolver",
    # Method dispatch
    "MethodDispatchWrapper",
]
