"""
Get Manager Approval Handler for Blog Post 04 - Customer Success namespace.

Routes refund request to manager for approval if required by policy.

TAS-137 Best Practices:
- get_input(): Access task context fields (refund_amount, refund_reason)
- get_dependency_result(): Access upstream step results (check_refund_policy, validate_refund_request)
- get_dependency_field(): Extract nested fields (requires_approval, customer_tier, ticket_id, customer_id)
"""

import uuid
from datetime import datetime, timezone

from tasker_core import StepContext, StepHandler, StepHandlerResult


class GetManagerApprovalHandler(StepHandler):
    """Get manager approval for refund if required."""

    handler_name = "team_scaling.customer_success.step_handlers.GetManagerApprovalHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Execute the get manager approval step."""
        # TAS-137: Validate dependency result exists
        policy_result = context.get_dependency_result("check_refund_policy")

        if not policy_result or not policy_result.get("policy_checked"):
            return self.failure(
                "Policy check must be completed before approval",
                error_code="MISSING_POLICY_CHECK",
                retryable=False,
            )

        # TAS-137: Use get_dependency_field() for nested extraction
        requires_approval = context.get_dependency_field("check_refund_policy", "requires_approval")
        customer_tier = context.get_dependency_field("check_refund_policy", "customer_tier")
        ticket_id = context.get_dependency_field("validate_refund_request", "ticket_id")
        customer_id = context.get_dependency_field("validate_refund_request", "customer_id")

        # TAS-137: Use get_input() for task context access
        refund_amount = context.get_input("refund_amount")
        refund_reason = context.get_input("refund_reason")

        now = datetime.now(timezone.utc).isoformat()

        if requires_approval:
            # Simulate approval process
            approval_result = self._simulate_manager_approval(
                ticket_id, customer_id, refund_amount, refund_reason
            )

            # Ensure approval was obtained
            approval_error = self._ensure_approval_obtained(approval_result)
            if approval_error:
                return approval_error

            return self.success(
                {
                    "approval_obtained": True,
                    "approval_required": True,
                    "auto_approved": False,
                    "approval_id": approval_result["approval_id"],
                    "manager_id": approval_result["manager_id"],
                    "manager_notes": approval_result.get("manager_notes"),
                    "approved_at": approval_result.get("approved_at", now),
                    "namespace": "customer_success",
                },
                {
                    "operation": "get_manager_approval",
                    "service": "approval_portal",
                    "approval_required": True,
                    "approval_id": approval_result["approval_id"],
                },
            )
        else:
            # Auto-approved based on customer tier
            return self.success(
                {
                    "approval_obtained": True,
                    "approval_required": False,
                    "auto_approved": True,
                    "approval_id": None,
                    "manager_id": None,
                    "manager_notes": f"Auto-approved for customer tier {customer_tier}",
                    "approved_at": now,
                    "namespace": "customer_success",
                },
                {
                    "operation": "get_manager_approval",
                    "service": "approval_portal",
                    "approval_required": False,
                    "auto_approved": True,
                },
            )

    def _simulate_manager_approval(
        self, ticket_id: str, customer_id: str, _refund_amount: float, _refund_reason: str
    ) -> dict:
        """Simulate manager approval process."""
        # Simulate different approval scenarios based on ticket_id patterns
        if "ticket_denied" in ticket_id:
            return {
                "status": "denied",
                "reason": "Manager denied refund request",
                "manager_id": f"mgr_{(hash(ticket_id) % 5) + 1}",
            }
        elif "ticket_pending" in ticket_id:
            return {
                "status": "pending",
                "reason": "Waiting for manager response",
            }
        else:
            # Success case - approval granted
            now = datetime.now(timezone.utc).isoformat()
            return {
                "status": "approved",
                "approval_required": True,
                "approval_id": f"appr_{uuid.uuid4().hex[:16]}",
                "manager_id": f"mgr_{(hash(ticket_id) % 5) + 1}",
                "manager_notes": f"Approved refund request for customer {customer_id}",
                "approved_at": now,
            }

    def _ensure_approval_obtained(self, approval_result: dict) -> StepHandlerResult | None:
        """Ensure approval was obtained if required."""
        status = approval_result.get("status")

        if status == "approved":
            return None  # Approval granted
        elif status == "denied":
            return self.failure(
                f"Manager denied refund request: {approval_result.get('reason')}",
                error_code="APPROVAL_DENIED",
                retryable=False,
            )
        elif status == "pending":
            return self.failure(
                "Waiting for manager approval",
                error_code="APPROVAL_PENDING",
                retryable=True,
            )
        elif status == "timeout":
            return self.failure(
                "Manager approval timeout, will retry",
                error_code="APPROVAL_TIMEOUT",
                retryable=True,
            )
        else:
            return self.failure(
                f"Unknown approval status: {status}",
                error_code="UNKNOWN_STATUS",
                retryable=True,
            )
