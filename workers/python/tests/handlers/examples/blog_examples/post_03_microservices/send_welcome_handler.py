"""
Send Welcome Sequence Handler for Blog Post 03.

Demonstrates multi-step coordination with convergence from parallel steps.
Sends personalized welcome messages via multiple channels.

TAS-137 Best Practices:
- get_dependency_result() for upstream step data
- get_dependency_field() for nested field extraction
- Multiple dependency access (billing + preferences)
"""

import uuid
from datetime import datetime, timezone

from tasker_core import StepContext, StepHandler, StepHandlerResult

# Welcome message templates by plan
WELCOME_TEMPLATES = {
    "free": {
        "subject": "Welcome to Our Platform!",
        "greeting": "Thanks for joining us",
        "highlights": [
            "Get started with basic features",
            "Explore your dashboard",
            "Join our community",
        ],
        "upgrade_prompt": "Upgrade to Pro for advanced features",
    },
    "pro": {
        "subject": "Welcome to Pro!",
        "greeting": "Thanks for upgrading to Pro",
        "highlights": [
            "Access advanced analytics",
            "Priority support",
            "API access",
            "Custom integrations",
        ],
        "upgrade_prompt": "Consider Enterprise for dedicated support",
    },
    "enterprise": {
        "subject": "Welcome to Enterprise!",
        "greeting": "Welcome to your Enterprise account",
        "highlights": [
            "Dedicated account manager",
            "Custom SLA",
            "Advanced security features",
            "Priority support 24/7",
        ],
        "upgrade_prompt": None,
    },
}


class SendWelcomeSequenceHandler(StepHandler):
    """Send welcome sequence via multiple notification channels."""

    handler_name = "microservices.step_handlers.SendWelcomeSequenceHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Execute the send welcome sequence step."""
        # TAS-137: Get results from prior steps (convergence point)
        user_data = context.get_dependency_result("create_user_account")
        billing_data = context.get_dependency_result("setup_billing_profile")
        preferences_data = context.get_dependency_result("initialize_preferences")

        # Validate all prior steps completed
        missing = []
        if not user_data:
            missing.append("create_user_account")
        if not billing_data:
            missing.append("setup_billing_profile")
        if not preferences_data:
            missing.append("initialize_preferences")

        if missing:
            return self.failure(
                f"Missing results from steps: {', '.join(missing)}",
                error_code="MISSING_DEPENDENCY_DATA",
                retryable=False,
            )

        # TAS-137: Use get_dependency_field() for nested extraction
        user_id = context.get_dependency_field("create_user_account", "user_id")
        email = context.get_dependency_field("create_user_account", "email")
        plan = context.get_dependency_field("create_user_account", "plan") or "free"

        # Get user preferences for notification channels
        prefs = preferences_data.get("preferences", {})

        # Build and send notifications
        template = WELCOME_TEMPLATES.get(plan, WELCOME_TEMPLATES["free"])
        channels_used = []
        messages_sent = []
        now = datetime.now(timezone.utc).isoformat()

        # Email (send if email_notifications enabled)
        if prefs.get("email_notifications", True):
            channels_used.append("email")
            messages_sent.append(
                {
                    "channel": "email",
                    "to": email,
                    "subject": template["subject"],
                    "body": self._build_email_body(template, user_id, plan, billing_data),
                    "sent_at": now,
                }
            )

        # In-app notification (always send)
        channels_used.append("in_app")
        messages_sent.append(
            {
                "channel": "in_app",
                "user_id": user_id,
                "title": template["subject"],
                "message": template["greeting"],
                "sent_at": now,
            }
        )

        # SMS (only for enterprise plan)
        if plan == "enterprise":
            channels_used.append("sms")
            messages_sent.append(
                {
                    "channel": "sms",
                    "to": "+1-555-ENTERPRISE",
                    "message": "Welcome to Enterprise! Your account manager will contact you soon.",
                    "sent_at": now,
                }
            )

        welcome_sequence_id = f"welcome_{uuid.uuid4().hex[:12]}"

        return self.success(
            {
                "user_id": user_id,
                "plan": plan,
                "channels_used": channels_used,
                "messages_sent": len(messages_sent),
                "welcome_sequence_id": welcome_sequence_id,
                "status": "sent",
                "sent_at": now,
            },
            {
                "operation": "send_welcome_sequence",
                "service": "notification_service",
                "plan": plan,
                "channels_used": len(channels_used),
            },
        )

    def _build_email_body(self, template: dict, user_id: str, plan: str, billing_data: dict) -> str:
        """Build personalized email body."""
        body_parts = [
            template["greeting"],
            "",
            "Here are your account highlights:",
        ]

        for highlight in template["highlights"]:
            body_parts.append(f"- {highlight}")

        # Add billing information for paid plans
        if plan != "free" and billing_data.get("billing_id"):
            body_parts.extend(
                [
                    "",
                    f"Billing ID: {billing_data.get('billing_id')}",
                    f"Next billing date: {billing_data.get('next_billing_date')}",
                ]
            )

        # Add upgrade prompt if present
        if template.get("upgrade_prompt"):
            body_parts.extend(["", template["upgrade_prompt"]])

        body_parts.extend(["", f"User ID: {user_id}"])

        return "\n".join(body_parts)
