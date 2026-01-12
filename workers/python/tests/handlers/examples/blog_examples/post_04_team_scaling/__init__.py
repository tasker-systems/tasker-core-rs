"""
Blog Post 04: Team Scaling - Namespace-based workflow organization.

This post demonstrates how namespaces enable team autonomy by allowing
multiple teams to use the same logical workflow names without conflicts.

Two namespaces demonstrated:
- customer_success: Customer-facing refund requests with approval workflow
- payments: Direct payment gateway refunds

TAS-137 Best Practices:
- get_input(): Access task context fields
- get_input_or(): Access task context with defaults
- get_dependency_result(): Access upstream step results
- get_dependency_field(): Extract nested fields from dependencies
"""

from .customer_success import (
    CheckRefundPolicyHandler,
    ExecuteRefundWorkflowHandler,
    GetManagerApprovalHandler,
    UpdateTicketStatusHandler,
    ValidateRefundRequestHandler,
)
from .payments import (
    NotifyCustomerHandler,
    ProcessGatewayRefundHandler,
    UpdatePaymentRecordsHandler,
    ValidatePaymentEligibilityHandler,
)

__all__ = [
    # Customer Success namespace handlers
    "ValidateRefundRequestHandler",
    "CheckRefundPolicyHandler",
    "GetManagerApprovalHandler",
    "ExecuteRefundWorkflowHandler",
    "UpdateTicketStatusHandler",
    # Payments namespace handlers
    "ValidatePaymentEligibilityHandler",
    "ProcessGatewayRefundHandler",
    "UpdatePaymentRecordsHandler",
    "NotifyCustomerHandler",
]
