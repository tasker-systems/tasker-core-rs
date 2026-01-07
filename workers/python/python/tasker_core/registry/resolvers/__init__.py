"""TAS-93: Built-in resolver implementations.

This module provides the default resolvers for the resolver chain:
- ExplicitMappingResolver (priority 10): Explicit key → handler mappings
- ClassLookupResolver (priority 100): Class path inference via importlib
"""

from __future__ import annotations

from .class_lookup import ClassLookupResolver
from .explicit_mapping import ExplicitMappingResolver

__all__ = [
    "ExplicitMappingResolver",
    "ClassLookupResolver",
]
