"""
Check Refund Policy Handler for Blog Post 04 - Customer Success namespace.

Checks if the refund request complies with policy rules based on customer tier.

TAS-137 Best Practices:
- get_input(): Access task context fields (refund_amount, refund_reason)
- get_dependency_result(): Access upstream step results (validate_refund_request)
- get_dependency_field(): Extract nested fields (customer_tier, original_purchase_date)
"""

from datetime import datetime, timezone

from tasker_core import StepContext, StepHandler, StepHandlerResult

# Refund policy rules by customer tier
REFUND_POLICIES = {
    "standard": {"window_days": 30, "requires_approval": True, "max_amount": 10_000},
    "gold": {"window_days": 60, "requires_approval": False, "max_amount": 50_000},
    "premium": {"window_days": 90, "requires_approval": False, "max_amount": 100_000},
}


class CheckRefundPolicyHandler(StepHandler):
    """Check if refund request complies with policy rules."""

    handler_name = "team_scaling.customer_success.step_handlers.CheckRefundPolicyHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Execute the check refund policy step."""
        # TAS-137: Validate dependency result exists
        validation_result = context.get_dependency_result("validate_refund_request")

        if not validation_result or not validation_result.get("request_validated"):
            return self.failure(
                "Request validation must be completed before policy check",
                error_code="MISSING_VALIDATION",
                retryable=False,
            )

        # TAS-137: Use get_dependency_field() for nested extraction
        customer_tier = (
            context.get_dependency_field("validate_refund_request", "customer_tier") or "standard"
        )
        purchase_date_str = context.get_dependency_field(
            "validate_refund_request", "original_purchase_date"
        )

        # TAS-137: Use get_input() for task context access
        refund_amount = context.get_input("refund_amount")
        _refund_reason = context.get_input("refund_reason")  # For audit logging

        # Check policy compliance
        policy_result = self._check_policy_compliance(
            customer_tier, refund_amount, purchase_date_str
        )

        # Ensure policy allows refund
        policy_error = self._ensure_policy_compliant(policy_result)
        if policy_error:
            return policy_error

        now = datetime.now(timezone.utc).isoformat()

        return self.success(
            {
                "policy_checked": True,
                "policy_compliant": True,
                "customer_tier": customer_tier,
                "refund_window_days": policy_result["refund_window_days"],
                "days_since_purchase": policy_result["days_since_purchase"],
                "within_refund_window": policy_result["within_window"],
                "requires_approval": policy_result["requires_approval"],
                "max_allowed_amount": policy_result["max_allowed_amount"],
                "policy_checked_at": now,
                "namespace": "customer_success",
            },
            {
                "operation": "check_refund_policy",
                "service": "policy_engine",
                "customer_tier": customer_tier,
                "requires_approval": policy_result["requires_approval"],
            },
        )

    def _check_policy_compliance(
        self, customer_tier: str, refund_amount: float, purchase_date_str: str
    ) -> dict:
        """Check if refund complies with policy rules."""
        policy = REFUND_POLICIES.get(customer_tier, REFUND_POLICIES["standard"])

        # Calculate days since purchase
        purchase_date = datetime.fromisoformat(purchase_date_str.replace("Z", "+00:00"))
        now = datetime.now(timezone.utc)
        days_since_purchase = (now - purchase_date).days

        # Check if within refund window
        within_window = days_since_purchase <= policy["window_days"]

        # Check if amount is within limits
        within_amount_limit = refund_amount <= policy["max_amount"]

        return {
            "customer_tier": customer_tier,
            "refund_window_days": policy["window_days"],
            "days_since_purchase": days_since_purchase,
            "within_window": within_window,
            "requires_approval": policy["requires_approval"],
            "max_allowed_amount": policy["max_amount"],
            "requested_amount": refund_amount,
            "within_amount_limit": within_amount_limit,
            "policy_compliant": within_window and within_amount_limit,
        }

    def _ensure_policy_compliant(self, policy_result: dict) -> StepHandlerResult | None:
        """Ensure policy allows this refund."""
        if not policy_result["within_window"]:
            return self.failure(
                f"Refund request outside policy window: {policy_result['days_since_purchase']} days "
                f"(max: {policy_result['refund_window_days']} days)",
                error_code="OUTSIDE_REFUND_WINDOW",
                retryable=False,
            )

        if not policy_result["within_amount_limit"]:
            return self.failure(
                f"Refund amount exceeds policy limit: ${policy_result['requested_amount'] / 100:.2f} "
                f"(max: ${policy_result['max_allowed_amount'] / 100:.2f})",
                error_code="EXCEEDS_AMOUNT_LIMIT",
                retryable=False,
            )

        return None  # Policy compliant
