"""Example handlers demonstrating workflow patterns.

This package contains example handlers for:
- Linear workflows (sequential step execution)
- Diamond workflows (parallel branches that merge)
- DAG workflows (complex directed acyclic graphs)
"""

from .diamond_handlers import (
    DiamondInitHandler,
    DiamondMergeHandler,
    DiamondPathAHandler,
    DiamondPathBHandler,
)
from .linear_handlers import FetchDataHandler, StoreDataHandler, TransformDataHandler

__all__ = [
    # Linear workflow handlers
    "FetchDataHandler",
    "TransformDataHandler",
    "StoreDataHandler",
    # Diamond workflow handlers
    "DiamondInitHandler",
    "DiamondPathAHandler",
    "DiamondPathBHandler",
    "DiamondMergeHandler",
]
