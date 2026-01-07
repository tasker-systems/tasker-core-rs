"""Example handlers demonstrating workflow patterns.

This package contains example handlers for:
- Linear workflows (sequential step execution)
- Diamond workflows (parallel branches that merge)
- Test scenarios (success, retryable errors, permanent errors)
- Conditional approval workflows (decision points with dynamic routing)
- Batch processing workflows (cursor-based parallel batch workers)

Unit test handlers (simple patterns):
- FetchDataHandler, TransformDataHandler, StoreDataHandler
- DiamondInitHandler, DiamondPathAHandler, DiamondPathBHandler, DiamondMergeHandler

E2E test handlers (matching Ruby patterns for integration testing):
- linear_workflow: LinearStep1Handler through LinearStep4Handler
- diamond_workflow: DiamondStartHandler, DiamondBranchBHandler, DiamondBranchCHandler, DiamondEndHandler
- test_scenarios: SuccessStepHandler, RetryableErrorStepHandler, PermanentErrorStepHandler
- conditional_approval: ValidateRequestHandler, RoutingDecisionHandler, etc. (Phase 6b)
- batch_processing: CsvAnalyzerHandler, CsvBatchProcessorHandler, etc. (Phase 6b)
- domain_events: ValidateOrderHandler, ProcessPaymentHandler, etc. (TAS-65/TAS-69)
"""

# Phase 6b handlers - Batch processing
from .batch_processing_handlers import (
    CsvAnalyzerHandler,
    CsvBatchProcessorHandler,
    CsvResultsAggregatorHandler,
)

# TAS-125 - Checkpoint yield handlers
from .checkpoint_yield_handlers import (
    CheckpointYieldAggregatorHandler,
    CheckpointYieldAnalyzerHandler,
    CheckpointYieldWorkerHandler,
)

# Phase 6b handlers - Conditional approval (decision points)
from .conditional_approval_handlers import (
    AutoApproveHandler,
    FinalizeApprovalHandler,
    FinanceReviewHandler,
    ManagerApprovalHandler,
    RoutingDecisionHandler,
    ValidateRequestHandler,
)

# Unit test handlers (simple patterns)
from .diamond_handlers import (
    DiamondInitHandler,
    DiamondMergeHandler,
    DiamondPathAHandler,
    DiamondPathBHandler,
)

# E2E test handlers (matching Ruby patterns)
from .diamond_workflow_handlers import (
    DiamondBranchBHandler,
    DiamondBranchCHandler,
    DiamondEndHandler,
    DiamondStartHandler,
)

# TAS-65/TAS-69 - Domain event handlers
from .domain_event_handlers import (
    ProcessPaymentHandler,
    SendNotificationHandler,
    UpdateInventoryHandler,
    ValidateOrderHandler,
)
from .linear_handlers import FetchDataHandler, StoreDataHandler, TransformDataHandler
from .linear_workflow_handlers import (
    LinearStep1Handler,
    LinearStep2Handler,
    LinearStep3Handler,
    LinearStep4Handler,
)
from .test_scenarios_handlers import (
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
    # Phase 6b - Conditional approval handlers
    "ValidateRequestHandler",
    "RoutingDecisionHandler",
    "AutoApproveHandler",
    "ManagerApprovalHandler",
    "FinanceReviewHandler",
    "FinalizeApprovalHandler",
    # Phase 6b - Batch processing handlers
    "CsvAnalyzerHandler",
    "CsvBatchProcessorHandler",
    "CsvResultsAggregatorHandler",
    # TAS-125 - Checkpoint yield handlers
    "CheckpointYieldAnalyzerHandler",
    "CheckpointYieldWorkerHandler",
    "CheckpointYieldAggregatorHandler",
    # TAS-65/TAS-69 - Domain event handlers
    "ValidateOrderHandler",
    "ProcessPaymentHandler",
    "UpdateInventoryHandler",
    "SendNotificationHandler",
]
