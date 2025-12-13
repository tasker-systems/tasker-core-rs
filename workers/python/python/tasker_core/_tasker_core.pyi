"""Type stubs for the Rust FFI module.

This file provides type hints for the _tasker_core extension module
which is compiled from Rust using PyO3.
"""

# Version of the package from Cargo.toml
__version__: str

def get_version() -> str:
    """Return the package version string.

    Returns:
        The version string (e.g., "0.1.0")
    """
    ...

def get_rust_version() -> str:
    """Return the Rust library version for debugging.

    Returns:
        Version string including rustc version (e.g., "tasker-worker-py 0.1.0 (rustc 1.70.0)")
    """
    ...

def health_check() -> bool:
    """Check if the FFI module is working correctly.

    Returns:
        True if the FFI layer is functional
    """
    ...
