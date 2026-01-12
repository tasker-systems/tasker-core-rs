"""
Customer Success namespace handlers for Blog Post 04.

This namespace handles customer-facing refund requests with approval workflow.
Demonstrates cross-namespace coordination by calling into the payments namespace.

Workflow: process_refund (customer_success namespace)
Steps:
1. validate_refund_request - Validate customer refund request details
2. check_refund_policy - Verify request complies with policies
3. get_manager_approval - Route to manager for approval if needed
4. execute_refund_workflow - Call payments team's workflow (cross-namespace)
5. update_ticket_status - Update customer support ticket
"""

from .check_refund_policy_handler import CheckRefundPolicyHandler
from .execute_refund_workflow_handler import ExecuteRefundWorkflowHandler
from .get_manager_approval_handler import GetManagerApprovalHandler
from .update_ticket_status_handler import UpdateTicketStatusHandler
from .validate_refund_request_handler import ValidateRefundRequestHandler

__all__ = [
    "ValidateRefundRequestHandler",
    "CheckRefundPolicyHandler",
    "GetManagerApprovalHandler",
    "ExecuteRefundWorkflowHandler",
    "UpdateTicketStatusHandler",
]
