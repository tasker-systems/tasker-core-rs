"""Example handlers demonstrating workflow patterns.

This package contains example handlers for:
- Linear workflows (sequential step execution)
- Diamond workflows (parallel branches that merge)
- Test scenarios (success, retryable errors, permanent errors)

Unit test handlers (simple patterns):
- FetchDataHandler, TransformDataHandler, StoreDataHandler
- DiamondInitHandler, DiamondPathAHandler, DiamondPathBHandler, DiamondMergeHandler

E2E test handlers (matching Ruby patterns for integration testing):
- linear_workflow: LinearStep1Handler through LinearStep4Handler
- diamond_workflow: DiamondStartHandler, DiamondBranchBHandler, DiamondBranchCHandler, DiamondEndHandler
- test_scenarios: SuccessStepHandler, RetryableErrorStepHandler, PermanentErrorStepHandler
"""

# Unit test handlers (simple patterns)
from .diamond_handlers import (
    DiamondInitHandler,
    DiamondMergeHandler,
    DiamondPathAHandler,
    DiamondPathBHandler,
)
from .diamond_workflow import (
    DiamondBranchBHandler,
    DiamondBranchCHandler,
    DiamondEndHandler,
    DiamondStartHandler,
)
from .linear_handlers import FetchDataHandler, StoreDataHandler, TransformDataHandler

# E2E test handlers (matching Ruby patterns)
from .linear_workflow import (
    LinearStep1Handler,
    LinearStep2Handler,
    LinearStep3Handler,
    LinearStep4Handler,
)
from .test_scenarios import (
    PermanentErrorStepHandler,
    RetryableErrorStepHandler,
    SuccessStepHandler,
)

__all__ = [
    # Unit test - Linear workflow handlers
    "FetchDataHandler",
    "TransformDataHandler",
    "StoreDataHandler",
    # Unit test - Diamond workflow handlers
    "DiamondInitHandler",
    "DiamondPathAHandler",
    "DiamondPathBHandler",
    "DiamondMergeHandler",
    # E2E test - Linear workflow handlers
    "LinearStep1Handler",
    "LinearStep2Handler",
    "LinearStep3Handler",
    "LinearStep4Handler",
    # E2E test - Diamond workflow handlers
    "DiamondStartHandler",
    "DiamondBranchBHandler",
    "DiamondBranchCHandler",
    "DiamondEndHandler",
    # E2E test - Test scenario handlers
    "SuccessStepHandler",
    "RetryableErrorStepHandler",
    "PermanentErrorStepHandler",
]
