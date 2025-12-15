"""Linear workflow handlers for Python E2E testing.

This package contains step handlers implementing a linear mathematical sequence:
1. Step 1: Square the initial even number (n -> n**2)
2. Step 2: Add constant to squared result (n**2 -> n**2 + 10)
3. Step 3: Multiply by factor ((n**2 + 10) -> (n**2 + 10) * 3)
4. Step 4: Divide for final result (((n**2 + 10) * 3) -> ((n**2 + 10) * 3) / 2)

Example with input 6:
- Step 1: 6**2 = 36
- Step 2: 36 + 10 = 46
- Step 3: 46 * 3 = 138
- Step 4: 138 / 2 = 69
"""

from .step_handlers import (
    LinearStep1Handler,
    LinearStep2Handler,
    LinearStep3Handler,
    LinearStep4Handler,
)

__all__ = [
    "LinearStep1Handler",
    "LinearStep2Handler",
    "LinearStep3Handler",
    "LinearStep4Handler",
]
