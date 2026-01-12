"""
Setup Billing Profile Handler for Blog Post 03.

Demonstrates graceful degradation - free plan users skip billing setup.

TAS-137 Best Practices:
- get_dependency_result() for upstream step data
- get_dependency_field() for nested field extraction
"""

import uuid
from datetime import datetime, timedelta, timezone

from tasker_core import StepContext, StepHandler, StepHandlerResult

# Billing tiers with pricing
BILLING_TIERS = {
    "free": {
        "price": 0,
        "features": ["basic_features"],
        "billing_required": False,
    },
    "pro": {
        "price": 29.99,
        "features": ["basic_features", "advanced_analytics"],
        "billing_required": True,
    },
    "enterprise": {
        "price": 299.99,
        "features": [
            "basic_features",
            "advanced_analytics",
            "priority_support",
            "custom_integrations",
        ],
        "billing_required": True,
    },
}


class SetupBillingProfileHandler(StepHandler):
    """Setup billing profile with graceful degradation for free plans."""

    handler_name = "microservices.step_handlers.SetupBillingProfileHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Execute the setup billing profile step."""
        # TAS-137: Get dependency result from create_user_account
        user_data = context.get_dependency_result("create_user_account")

        if not user_data:
            return self.failure(
                "User data not found from create_user_account step",
                error_code="MISSING_USER_DATA",
                retryable=False,
            )

        # TAS-137: Use get_dependency_field() for nested extraction
        user_id = context.get_dependency_field("create_user_account", "user_id")
        plan = context.get_dependency_field("create_user_account", "plan") or "free"

        # Get tier configuration
        tier_config = BILLING_TIERS.get(plan, BILLING_TIERS["free"])

        if tier_config["billing_required"]:
            # Paid plan - create billing profile
            now = datetime.now(timezone.utc)
            next_billing = (now + timedelta(days=30)).isoformat()
            billing_id = f"billing_{uuid.uuid4().hex[:12]}"

            return self.success(
                {
                    "billing_id": billing_id,
                    "user_id": user_id,
                    "plan": plan,
                    "price": tier_config["price"],
                    "currency": "USD",
                    "billing_cycle": "monthly",
                    "features": tier_config["features"],
                    "status": "active",
                    "next_billing_date": next_billing,
                    "created_at": now.isoformat(),
                },
                {
                    "operation": "setup_billing",
                    "service": "billing_service",
                    "plan": plan,
                    "billing_required": True,
                },
            )
        else:
            # Free plan - graceful degradation, skip billing
            return self.success(
                {
                    "user_id": user_id,
                    "plan": plan,
                    "billing_required": False,
                    "status": "skipped_free_plan",
                    "message": "Free plan users do not require billing setup",
                },
                {
                    "operation": "setup_billing",
                    "service": "billing_service",
                    "plan": plan,
                    "billing_required": False,
                },
            )
