"""
Execute Refund Workflow Handler for Blog Post 04 - Customer Success namespace.

Key demonstration of cross-namespace workflow coordination.
Customer Success team calls Payments team's workflow via internal task creation.

TAS-137 Best Practices:
- get_input(): Access task context fields (refund_amount, refund_reason, customer_email, ticket_id, correlation_id)
- get_input_or(): Access task context with defaults (refund_reason, customer_email)
- get_dependency_result(): Access upstream step results (get_manager_approval, validate_refund_request)
- get_dependency_field(): Extract nested fields (payment_id, approval_id)
"""

import uuid
from datetime import datetime, timezone

from tasker_core import StepContext, StepHandler, StepHandlerResult


class ExecuteRefundWorkflowHandler(StepHandler):
    """Execute cross-namespace refund workflow delegation."""

    handler_name = "team_scaling.customer_success.step_handlers.ExecuteRefundWorkflowHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Execute the cross-namespace refund workflow delegation step."""
        # TAS-137: Validate approval dependency exists
        approval_result = context.get_dependency_result("get_manager_approval")

        if not approval_result or not approval_result.get("approval_obtained"):
            return self.failure(
                "Manager approval must be obtained before executing refund",
                error_code="MISSING_APPROVAL",
                retryable=False,
            )

        # TAS-137: Use get_dependency_field() for nested extraction
        payment_id = context.get_dependency_field("validate_refund_request", "payment_id")
        if not payment_id:
            return self.failure(
                "Payment ID not found in validation results",
                error_code="MISSING_PAYMENT_ID",
                retryable=False,
            )

        approval_id = context.get_dependency_field("get_manager_approval", "approval_id")

        # TAS-137: Use get_input() and get_input_or() for task context access
        refund_amount = context.get_input("refund_amount")
        refund_reason = context.get_input_or("refund_reason", "customer_request")
        customer_email = context.get_input_or("customer_email", "customer@example.com")
        ticket_id = context.get_input("ticket_id")
        correlation_id = context.get_input("correlation_id") or self._generate_correlation_id()

        # Build the cross-namespace task delegation payload
        delegation_inputs = {
            "namespace": "payments",
            "workflow_name": "process_refund",
            "workflow_version": "2.1.0",
            "context": {
                "payment_id": payment_id,
                "refund_amount": refund_amount,
                "refund_reason": refund_reason,
                "customer_email": customer_email,
                "initiated_by": "customer_success",
                "approval_id": approval_id,
                "ticket_id": ticket_id,
                "correlation_id": correlation_id,
            },
        }

        # Simulate task creation in payments namespace
        delegation_result = self._create_payments_task(delegation_inputs)

        now = datetime.now(timezone.utc).isoformat()

        return self.success(
            {
                "task_delegated": True,
                "target_namespace": delegation_inputs["namespace"],
                "target_workflow": delegation_inputs["workflow_name"],
                "delegated_task_id": delegation_result["task_id"],
                "delegated_task_status": delegation_result["status"],
                "delegation_timestamp": now,
                "correlation_id": correlation_id,
                "namespace": "customer_success",
            },
            {
                "operation": "execute_refund_workflow",
                "service": "task_delegation",
                "target_namespace": delegation_inputs["namespace"],
                "target_workflow": delegation_inputs["workflow_name"],
                "delegated_task_id": delegation_result["task_id"],
            },
        )

    def _create_payments_task(self, inputs: dict) -> dict:
        """Simulate task creation in payments namespace."""
        # In a real implementation, this would make an HTTP call to the payments service
        # or use the Tasker API to create a task in the payments namespace
        task_id = f"task_{uuid.uuid4()}"
        correlation_id = inputs["context"]["correlation_id"]

        return {
            "task_id": task_id,
            "status": "created",
            "correlation_id": correlation_id,
            "namespace": inputs["namespace"],
            "workflow_name": inputs["workflow_name"],
            "workflow_version": inputs["workflow_version"],
            "created_at": datetime.now(timezone.utc).isoformat(),
        }

    def _generate_correlation_id(self) -> str:
        """Generate correlation ID for cross-team tracking."""
        return f"cs-{uuid.uuid4().hex[:16]}"
