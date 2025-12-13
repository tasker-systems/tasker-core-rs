"""pytest configuration and fixtures for tasker_core tests."""

from __future__ import annotations

import pytest


@pytest.fixture(scope="session")
def tasker_core_module():
    """Provide the tasker_core module as a fixture."""
    import tasker_core

    return tasker_core
