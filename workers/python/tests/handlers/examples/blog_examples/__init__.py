"""Blog example handlers demonstrating real-world workflow patterns.

This package contains handlers from the Tasker blog series:

- post_01_ecommerce: E-commerce order processing (linear workflow)
- post_02_data_pipeline: Analytics data pipeline (DAG workflow)
- post_03_microservices: User registration with multi-service coordination
- post_04_team_scaling: Refund processing with namespaces (two namespaces)

Each post demonstrates progressively more complex workflow patterns
using idiomatic Python with TAS-137 StepContext best practices.
"""

# Post 01: E-commerce Order Processing (Linear Workflow)
from .post_01_ecommerce import (
    CreateOrderHandler,
    ProcessPaymentHandler,
    SendConfirmationHandler,
    UpdateInventoryHandler,
    ValidateCartHandler,
)

# Post 02: Data Pipeline Analytics (DAG Workflow)
from .post_02_data_pipeline import (
    AggregateMetricsHandler,
    ExtractCustomerDataHandler,
    ExtractInventoryDataHandler,
    ExtractSalesDataHandler,
    GenerateInsightsHandler,
    TransformCustomersHandler,
    TransformInventoryHandler,
    TransformSalesHandler,
)

# Post 03: Microservices Coordination (User Registration)
from .post_03_microservices import (
    CreateUserAccountHandler,
    InitializePreferencesHandler,
    SendWelcomeSequenceHandler,
    SetupBillingProfileHandler,
    UpdateUserStatusHandler,
)

# Post 04: Team Scaling - Customer Success namespace
from .post_04_team_scaling.customer_success import (
    CheckRefundPolicyHandler,
    ExecuteRefundWorkflowHandler,
    GetManagerApprovalHandler,
    UpdateTicketStatusHandler,
    ValidateRefundRequestHandler,
)

# Post 04: Team Scaling - Payments namespace
from .post_04_team_scaling.payments import (
    NotifyCustomerHandler,
    ProcessGatewayRefundHandler,
    UpdatePaymentRecordsHandler,
    ValidatePaymentEligibilityHandler,
)

__all__ = [
    # Post 01 - E-commerce
    "ValidateCartHandler",
    "ProcessPaymentHandler",
    "UpdateInventoryHandler",
    "CreateOrderHandler",
    "SendConfirmationHandler",
    # Post 02 - Data Pipeline
    "ExtractSalesDataHandler",
    "ExtractInventoryDataHandler",
    "ExtractCustomerDataHandler",
    "TransformSalesHandler",
    "TransformInventoryHandler",
    "TransformCustomersHandler",
    "AggregateMetricsHandler",
    "GenerateInsightsHandler",
    # Post 03 - Microservices
    "CreateUserAccountHandler",
    "SetupBillingProfileHandler",
    "InitializePreferencesHandler",
    "SendWelcomeSequenceHandler",
    "UpdateUserStatusHandler",
    # Post 04 - Team Scaling (Customer Success namespace)
    "ValidateRefundRequestHandler",
    "CheckRefundPolicyHandler",
    "GetManagerApprovalHandler",
    "ExecuteRefundWorkflowHandler",
    "UpdateTicketStatusHandler",
    # Post 04 - Team Scaling (Payments namespace)
    "ValidatePaymentEligibilityHandler",
    "ProcessGatewayRefundHandler",
    "UpdatePaymentRecordsHandler",
    "NotifyCustomerHandler",
]
