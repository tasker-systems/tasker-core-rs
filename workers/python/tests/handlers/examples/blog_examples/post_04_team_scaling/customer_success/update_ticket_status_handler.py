"""
Update Ticket Status Handler for Blog Post 04 - Customer Success namespace.

Final step in the customer success refund workflow.
Updates the support ticket with refund completion status.

TAS-137 Best Practices:
- get_input(): Access task context fields (refund_amount, refund_reason)
- get_dependency_result(): Access upstream step results (execute_refund_workflow, validate_refund_request)
- get_dependency_field(): Extract nested fields (ticket_id, customer_id, delegated_task_id, correlation_id)
"""

from datetime import datetime, timezone

from tasker_core import StepContext, StepHandler, StepHandlerResult


class UpdateTicketStatusHandler(StepHandler):
    """Update customer support ticket status."""

    handler_name = "team_scaling.customer_success.step_handlers.UpdateTicketStatusHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Execute the update ticket status step."""
        # TAS-137: Validate delegation dependency exists
        delegation_result = context.get_dependency_result("execute_refund_workflow")

        if not delegation_result or not delegation_result.get("task_delegated"):
            return self.failure(
                "Refund workflow must be executed before updating ticket",
                error_code="MISSING_DELEGATION",
                retryable=False,
            )

        # TAS-137: Use get_dependency_field() for nested extraction
        ticket_id = context.get_dependency_field("validate_refund_request", "ticket_id")
        customer_id = context.get_dependency_field("validate_refund_request", "customer_id")
        delegated_task_id = context.get_dependency_field(
            "execute_refund_workflow", "delegated_task_id"
        )
        correlation_id = context.get_dependency_field("execute_refund_workflow", "correlation_id")

        # TAS-137: Use get_input() for task context access
        refund_amount = context.get_input("refund_amount")
        _refund_reason = context.get_input("refund_reason")  # For audit logging

        # Simulate ticket status update
        update_result = self._simulate_ticket_update(
            ticket_id, customer_id, refund_amount, delegated_task_id, correlation_id
        )

        # Ensure update was successful
        update_error = self._ensure_update_successful(update_result)
        if update_error:
            return update_error

        return self.success(
            {
                "ticket_updated": True,
                "ticket_id": ticket_id,
                "previous_status": update_result["previous_status"],
                "new_status": update_result["new_status"],
                "resolution_note": update_result["resolution_note"],
                "updated_at": update_result["updated_at"],
                "refund_completed": True,
                "delegated_task_id": delegated_task_id,
                "namespace": "customer_success",
            },
            {
                "operation": "update_ticket_status",
                "service": "customer_service_platform",
                "ticket_id": ticket_id,
                "new_status": update_result["new_status"],
            },
        )

    def _simulate_ticket_update(
        self,
        ticket_id: str,
        _customer_id: str,
        refund_amount: float,
        delegated_task_id: str,
        correlation_id: str,
    ) -> dict:
        """Simulate ticket status update."""
        now = datetime.now(timezone.utc).isoformat()

        # Simulate different update scenarios based on ticket_id patterns
        if "ticket_locked" in ticket_id:
            return {
                "status": "locked",
                "error": "Ticket locked by another agent",
            }
        elif "ticket_update_error" in ticket_id:
            return {
                "status": "error",
                "error": "System error updating ticket",
            }
        else:
            # Success case
            return {
                "status": "updated",
                "ticket_id": ticket_id,
                "previous_status": "in_progress",
                "new_status": "resolved",
                "resolution_note": (
                    f"Refund of ${refund_amount / 100:.2f} processed successfully. "
                    f"Delegated task ID: {delegated_task_id}. "
                    f"Correlation ID: {correlation_id}"
                ),
                "resolution_type": "refund_processed",
                "updated_at": now,
                "updated_by": "automated_workflow",
                "customer_notified": True,
            }

    def _ensure_update_successful(self, update_result: dict) -> StepHandlerResult | None:
        """Ensure ticket update was successful."""
        status = update_result.get("status")

        if status in ("updated", "success"):
            return None  # Success
        elif status == "locked":
            return self.failure(
                "Ticket locked by another agent, will retry",
                error_code="TICKET_LOCKED",
                retryable=True,
            )
        elif status == "not_found":
            return self.failure(
                "Ticket not found in customer service system",
                error_code="TICKET_NOT_FOUND",
                retryable=False,
            )
        elif status in ("error", "failed"):
            return self.failure(
                f"Ticket update failed: {update_result.get('error')}",
                error_code="UPDATE_FAILED",
                retryable=True,
            )
        elif status == "unauthorized":
            return self.failure(
                "Not authorized to update ticket",
                error_code="UNAUTHORIZED",
                retryable=False,
            )
        else:
            return self.failure(
                f"Unknown update status: {status}",
                error_code="UNKNOWN_STATUS",
                retryable=True,
            )
