"""
Tasker Core Python Worker

This package provides Python bindings for the tasker-core workflow
orchestration system, enabling Python step handlers to integrate
with Rust-based orchestration.

Example:
    >>> import tasker_core
    >>> tasker_core.version()
    '0.1.0'
    >>> tasker_core.health_check()
    True

Phase 1 (TAS-72-P1) provides minimal functionality to verify the
build pipeline. Full functionality will be added in subsequent phases.
"""

from __future__ import annotations

# Import from the internal FFI module
from tasker_core._tasker_core import (
    __version__,
    get_rust_version,
    get_version,
    health_check,
)

__all__ = [
    "__version__",
    "get_version",
    "get_rust_version",
    "health_check",
    "version",
]


def version() -> str:
    """Return the package version.

    Returns:
        The version string (e.g., "0.1.0")

    Example:
        >>> import tasker_core
        >>> tasker_core.version()
        '0.1.0'
    """
    return get_version()
