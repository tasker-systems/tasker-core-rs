"""Example handlers demonstrating workflow patterns.

This package contains example handlers for:
- Linear workflows (sequential step execution)
- Diamond workflows (parallel branches that merge)
- Test scenarios (success, retryable errors, permanent errors)
- Conditional approval workflows (decision points with dynamic routing)
- Batch processing workflows (cursor-based parallel batch workers)
- Blog examples (real-world scenarios from the Tasker blog series)

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

Blog example handlers (TAS-91):
- post_01_ecommerce: ValidateCartHandler, ProcessPaymentHandler, etc.
"""

# Phase 6b handlers - Batch processing
from .batch_processing_handlers import (
    CsvAnalyzerHandler,
    CsvBatchProcessorHandler,
    CsvResultsAggregatorHandler,
)

# TAS-91 - Blog example handlers (Post 01: E-commerce)
# Using prefixed names to avoid conflicts with domain_event_handlers
from .blog_examples.post_01_ecommerce import (
    CreateOrderHandler as EcommerceCreateOrderHandler,
)
from .blog_examples.post_01_ecommerce import (
    ProcessPaymentHandler as EcommerceProcessPaymentHandler,
)
from .blog_examples.post_01_ecommerce import (
    SendConfirmationHandler as EcommerceSendConfirmationHandler,
)
from .blog_examples.post_01_ecommerce import (
    UpdateInventoryHandler as EcommerceUpdateInventoryHandler,
)
from .blog_examples.post_01_ecommerce import (
    ValidateCartHandler as EcommerceValidateCartHandler,
)

# TAS-91 - Blog example handlers (Post 02: Data Pipeline)
from .blog_examples.post_02_data_pipeline import (
    AggregateMetricsHandler as DataPipelineAggregateMetricsHandler,
)
from .blog_examples.post_02_data_pipeline import (
    ExtractCustomerDataHandler as DataPipelineExtractCustomerDataHandler,
)
from .blog_examples.post_02_data_pipeline import (
    ExtractInventoryDataHandler as DataPipelineExtractInventoryDataHandler,
)
from .blog_examples.post_02_data_pipeline import (
    ExtractSalesDataHandler as DataPipelineExtractSalesDataHandler,
)
from .blog_examples.post_02_data_pipeline import (
    GenerateInsightsHandler as DataPipelineGenerateInsightsHandler,
)
from .blog_examples.post_02_data_pipeline import (
    TransformCustomersHandler as DataPipelineTransformCustomersHandler,
)
from .blog_examples.post_02_data_pipeline import (
    TransformInventoryHandler as DataPipelineTransformInventoryHandler,
)
from .blog_examples.post_02_data_pipeline import (
    TransformSalesHandler as DataPipelineTransformSalesHandler,
)

# TAS-91 - Blog example handlers (Post 03: Microservices)
from .blog_examples.post_03_microservices import (
    CreateUserAccountHandler as MicroservicesCreateUserAccountHandler,
)
from .blog_examples.post_03_microservices import (
    InitializePreferencesHandler as MicroservicesInitializePreferencesHandler,
)
from .blog_examples.post_03_microservices import (
    SendWelcomeSequenceHandler as MicroservicesSendWelcomeSequenceHandler,
)
from .blog_examples.post_03_microservices import (
    SetupBillingProfileHandler as MicroservicesSetupBillingProfileHandler,
)
from .blog_examples.post_03_microservices import (
    UpdateUserStatusHandler as MicroservicesUpdateUserStatusHandler,
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

# TAS-93 Phase 5 - Resolver tests handlers
from .resolver_tests_handlers import (
    AlternateMethodHandler,
    MultiMethodHandler,
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
    # TAS-93 Phase 5 - Resolver tests handlers
    "MultiMethodHandler",
    "AlternateMethodHandler",
    # TAS-91 - Blog examples (Post 01: E-commerce)
    "EcommerceValidateCartHandler",
    "EcommerceProcessPaymentHandler",
    "EcommerceUpdateInventoryHandler",
    "EcommerceCreateOrderHandler",
    "EcommerceSendConfirmationHandler",
    # TAS-91 - Blog examples (Post 02: Data Pipeline)
    "DataPipelineExtractSalesDataHandler",
    "DataPipelineExtractInventoryDataHandler",
    "DataPipelineExtractCustomerDataHandler",
    "DataPipelineTransformSalesHandler",
    "DataPipelineTransformInventoryHandler",
    "DataPipelineTransformCustomersHandler",
    "DataPipelineAggregateMetricsHandler",
    "DataPipelineGenerateInsightsHandler",
    # TAS-91 - Blog examples (Post 03: Microservices)
    "MicroservicesCreateUserAccountHandler",
    "MicroservicesSetupBillingProfileHandler",
    "MicroservicesInitializePreferencesHandler",
    "MicroservicesSendWelcomeSequenceHandler",
    "MicroservicesUpdateUserStatusHandler",
]
