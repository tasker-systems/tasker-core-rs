"""Diamond workflow handlers for Python E2E testing.

This package contains step handlers implementing a diamond pattern:
1. Start: Square the initial even number (n -> n**2)
2. Branch B (parallel): Add 25 to squared result (n**2 + 25)
3. Branch C (parallel): Multiply squared result by 2 (n**2 * 2)
4. End (convergence): Average both branch results ((branch_b + branch_c) / 2)

Example with input 6:
- Start: 6**2 = 36
- Branch B: 36 + 25 = 61
- Branch C: 36 * 2 = 72
- End: (61 + 72) / 2 = 66.5
"""

from .step_handlers import (
    DiamondBranchBHandler,
    DiamondBranchCHandler,
    DiamondEndHandler,
    DiamondStartHandler,
)

__all__ = [
    "DiamondStartHandler",
    "DiamondBranchBHandler",
    "DiamondBranchCHandler",
    "DiamondEndHandler",
]
