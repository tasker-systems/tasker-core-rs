"""
Update User Status Handler for Blog Post 03.

Final step that marks user as active after all services coordinated.

TAS-137 Best Practices:
- get_dependency_result() for upstream step data
- get_dependency_field() for nested field extraction
- Workflow completion validation
"""

from datetime import datetime, timezone

from tasker_core import StepContext, StepHandler, StepHandlerResult


class UpdateUserStatusHandler(StepHandler):
    """Update user status to active after workflow completion."""

    handler_name = "microservices.step_handlers.UpdateUserStatusHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Execute the update user status step."""
        # Collect results from all prior steps
        user_data = context.get_dependency_result("create_user_account")
        billing_data = context.get_dependency_result("setup_billing_profile")
        preferences_data = context.get_dependency_result("initialize_preferences")
        welcome_data = context.get_dependency_result("send_welcome_sequence")

        # Validate all prior steps completed
        missing = []
        if not user_data:
            missing.append("create_user_account")
        if not billing_data:
            missing.append("setup_billing_profile")
        if not preferences_data:
            missing.append("initialize_preferences")
        if not welcome_data:
            missing.append("send_welcome_sequence")

        if missing:
            return self.failure(
                f"Cannot complete registration: missing results from steps: {', '.join(missing)}",
                error_code="INCOMPLETE_WORKFLOW",
                retryable=False,
            )

        # TAS-137: Use get_dependency_field() for nested extraction
        user_id = context.get_dependency_field("create_user_account", "user_id")
        plan = context.get_dependency_field("create_user_account", "plan") or "free"
        email = context.get_dependency_field("create_user_account", "email")

        # Build registration summary
        registration_summary = self._build_registration_summary(
            user_id, email, plan, user_data, billing_data, preferences_data, welcome_data
        )

        now = datetime.now(timezone.utc).isoformat()

        return self.success(
            {
                "user_id": user_id,
                "status": "active",
                "plan": plan,
                "registration_summary": registration_summary,
                "activation_timestamp": now,
                "all_services_coordinated": True,
                "services_completed": [
                    "user_service",
                    "billing_service",
                    "preferences_service",
                    "notification_service",
                ],
            },
            {
                "operation": "update_user_status",
                "service": "user_service",
                "plan": plan,
                "workflow_complete": True,
            },
        )

    def _build_registration_summary(
        self,
        user_id: str,
        email: str,
        plan: str,
        user_data: dict,
        billing_data: dict,
        preferences_data: dict,
        welcome_data: dict,
    ) -> dict:
        """Build comprehensive registration summary."""
        summary = {
            "user_id": user_id,
            "email": email,
            "plan": plan,
            "registration_status": "complete",
        }

        # Add billing summary if applicable
        if plan != "free" and billing_data.get("billing_id"):
            summary["billing_id"] = billing_data.get("billing_id")
            summary["next_billing_date"] = billing_data.get("next_billing_date")

        # Add preferences summary
        prefs = preferences_data.get("preferences", {})
        summary["preferences_count"] = len(prefs) if isinstance(prefs, dict) else 0

        # Add welcome sequence summary
        summary["welcome_sent"] = True
        summary["notification_channels"] = welcome_data.get("channels_used", [])

        # Add timestamps
        summary["user_created_at"] = user_data.get("created_at")
        summary["registration_completed_at"] = datetime.now(timezone.utc).isoformat()

        return summary
