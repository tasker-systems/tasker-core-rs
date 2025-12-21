"""Step handlers for conditional approval workflow.

This module implements step handlers for a decision-point workflow that routes
approval requests based on amount thresholds using the DecisionHandler base class.

Thresholds:
- < $1,000: auto_approve only
- $1,000-$4,999: manager_approval only
- >= $5,000: manager_approval + finance_review
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from tasker_core import StepHandler, StepHandlerResult
from tasker_core.step_handler import DecisionHandler

if TYPE_CHECKING:
    from tasker_core import StepContext


class ValidateRequestHandler(StepHandler):
    """Validate approval request data.

    Validates:
    - amount is positive
    - requester is provided
    - purpose is provided
    """

    handler_name = "conditional_approval.step_handlers.ValidateRequestHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Validate the approval request."""
        input_data = context.input_data

        # Check required fields
        amount = input_data.get("amount")
        requester = input_data.get("requester")
        purpose = input_data.get("purpose")

        errors: list[str] = []

        if amount is None:
            errors.append("amount is required")
        elif not isinstance(amount, (int, float)) or amount <= 0:
            errors.append("amount must be a positive number")

        if not requester:
            errors.append("requester is required")

        if not purpose:
            errors.append("purpose is required")

        if errors:
            return StepHandlerResult.failure(
                message=f"Validation failed: {', '.join(errors)}",
                error_type="validation_error",
                retryable=False,
            )

        return StepHandlerResult.success(
            {
                "validated": True,
                "amount": amount,
                "requester": requester,
                "purpose": purpose,
            }
        )


class RoutingDecisionHandler(DecisionHandler):
    """Decision point handler for routing approvals based on amount.

    Routes to:
    - < $1,000: auto_approve
    - $1,000-$4,999: manager_approval
    - >= $5,000: manager_approval + finance_review
    """

    handler_name = "conditional_approval.step_handlers.RoutingDecisionHandler"
    handler_version = "1.0.0"

    SMALL_THRESHOLD = 1000.0
    LARGE_THRESHOLD = 5000.0

    def call(self, context: StepContext) -> StepHandlerResult:
        """Determine the approval routing based on amount."""
        # Get validated amount from dependency
        validate_result = context.get_dependency_result("validate_request_py")

        if validate_result is None:
            return self.decision_failure(
                message="Missing validation result from validate_request_py",
                error_type="dependency_error",
            )

        amount = validate_result.get("amount")
        if amount is None:
            return self.decision_failure(
                message="Missing amount in validation result",
                error_type="dependency_error",
            )

        # Determine routing based on amount thresholds
        if amount < self.SMALL_THRESHOLD:
            # Small amount: auto-approve only
            return self.decision_success(
                ["auto_approve_py"],
                routing_context={
                    "approval_path": "auto",
                    "amount": amount,
                    "threshold_used": "small",
                },
            )

        elif amount < self.LARGE_THRESHOLD:
            # Medium amount: manager approval only
            return self.decision_success(
                ["manager_approval_py"],
                routing_context={
                    "approval_path": "manager",
                    "amount": amount,
                    "threshold_used": "medium",
                },
            )

        else:
            # Large amount: dual approval (manager + finance)
            return self.decision_success(
                ["manager_approval_py", "finance_review_py"],
                routing_context={
                    "approval_path": "dual",
                    "amount": amount,
                    "threshold_used": "large",
                },
            )


class AutoApproveHandler(StepHandler):
    """Auto-approval handler for small amounts.

    Automatically approves requests under the small threshold.
    """

    handler_name = "conditional_approval.step_handlers.AutoApproveHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Auto-approve the request."""
        # Get routing context from decision
        routing_result = context.get_dependency_result("routing_decision_py")

        if routing_result is None:
            return StepHandlerResult.failure(
                message="Missing routing decision result",
                error_type="dependency_error",
                retryable=True,
            )

        routing_context = routing_result.get("routing_context", {})
        amount = routing_context.get("amount", 0)

        return StepHandlerResult.success(
            {
                "approved": True,
                "approval_type": "auto",
                "approved_amount": amount,
                "approver": "system",
                "notes": "Auto-approved for amounts under $1,000",
            }
        )


class ManagerApprovalHandler(StepHandler):
    """Manager approval handler for medium and large amounts.

    Simulates manager review and approval.
    """

    handler_name = "conditional_approval.step_handlers.ManagerApprovalHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Process manager approval."""
        routing_result = context.get_dependency_result("routing_decision_py")

        if routing_result is None:
            return StepHandlerResult.failure(
                message="Missing routing decision result",
                error_type="dependency_error",
                retryable=True,
            )

        routing_context = routing_result.get("routing_context", {})
        amount = routing_context.get("amount", 0)

        return StepHandlerResult.success(
            {
                "approved": True,
                "approval_type": "manager",
                "approved_amount": amount,
                "approver": "manager@example.com",
                "notes": "Manager approved after review",
            }
        )


class FinanceReviewHandler(StepHandler):
    """Finance review handler for large amounts.

    Additional financial review for amounts >= $5,000.
    """

    handler_name = "conditional_approval.step_handlers.FinanceReviewHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Process finance review."""
        routing_result = context.get_dependency_result("routing_decision_py")

        if routing_result is None:
            return StepHandlerResult.failure(
                message="Missing routing decision result",
                error_type="dependency_error",
                retryable=True,
            )

        routing_context = routing_result.get("routing_context", {})
        amount = routing_context.get("amount", 0)

        return StepHandlerResult.success(
            {
                "approved": True,
                "approval_type": "finance",
                "approved_amount": amount,
                "approver": "finance@example.com",
                "budget_code": "CAPEX-2024",
                "notes": "Finance review completed for large amount",
            }
        )


class FinalizeApprovalHandler(StepHandler):
    """Finalize approval by collecting all approval results.

    This is a deferred convergence step that waits for all
    dynamically created approval steps to complete.
    """

    handler_name = "conditional_approval.step_handlers.FinalizeApprovalHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Finalize the approval process."""
        # Collect approval results from whichever steps were created
        approvals: list[dict[str, Any]] = []

        # Check for auto_approve result
        auto_result = context.get_dependency_result("auto_approve_py")
        if auto_result:
            approvals.append(
                {
                    "type": "auto",
                    "approved": auto_result.get("approved", False),
                    "approver": auto_result.get("approver"),
                }
            )

        # Check for manager_approval result
        manager_result = context.get_dependency_result("manager_approval_py")
        if manager_result:
            approvals.append(
                {
                    "type": "manager",
                    "approved": manager_result.get("approved", False),
                    "approver": manager_result.get("approver"),
                }
            )

        # Check for finance_review result
        finance_result = context.get_dependency_result("finance_review_py")
        if finance_result:
            approvals.append(
                {
                    "type": "finance",
                    "approved": finance_result.get("approved", False),
                    "approver": finance_result.get("approver"),
                }
            )

        # Determine final approval status
        all_approved = all(a.get("approved", False) for a in approvals)

        return StepHandlerResult.success(
            {
                "final_status": "approved" if all_approved else "rejected",
                "approval_count": len(approvals),
                "approvals": approvals,
                "all_approved": all_approved,
            }
        )


__all__ = [
    "ValidateRequestHandler",
    "RoutingDecisionHandler",
    "AutoApproveHandler",
    "ManagerApprovalHandler",
    "FinanceReviewHandler",
    "FinalizeApprovalHandler",
]
