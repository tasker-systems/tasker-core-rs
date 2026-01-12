"""
Initialize Preferences Handler for Blog Post 03.

Demonstrates default preferences with fallback values by plan tier.

TAS-137 Best Practices:
- get_dependency_result() for upstream step data
- get_dependency_field() for nested field extraction
- get_input() for task context access
"""

import uuid
from datetime import datetime, timezone

from tasker_core import StepContext, StepHandler, StepHandlerResult

# Default preference templates by plan
DEFAULT_PREFERENCES = {
    "free": {
        "email_notifications": True,
        "marketing_emails": False,
        "product_updates": True,
        "weekly_digest": False,
        "theme": "light",
        "language": "en",
        "timezone": "UTC",
    },
    "pro": {
        "email_notifications": True,
        "marketing_emails": True,
        "product_updates": True,
        "weekly_digest": True,
        "theme": "dark",
        "language": "en",
        "timezone": "UTC",
        "api_notifications": True,
    },
    "enterprise": {
        "email_notifications": True,
        "marketing_emails": True,
        "product_updates": True,
        "weekly_digest": True,
        "theme": "dark",
        "language": "en",
        "timezone": "UTC",
        "api_notifications": True,
        "audit_logs": True,
        "advanced_reports": True,
    },
}


class InitializePreferencesHandler(StepHandler):
    """Initialize user preferences with plan-based defaults."""

    handler_name = "microservices.step_handlers.InitializePreferencesHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Execute the initialize preferences step."""
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

        # TAS-137: Get custom preferences from task input
        user_info = context.get_input("user_info") or {}
        custom_prefs = user_info.get("preferences", {})

        # Get default preferences for plan
        default_prefs = DEFAULT_PREFERENCES.get(plan, DEFAULT_PREFERENCES["free"])

        # Merge custom preferences with defaults (custom takes precedence)
        final_prefs = {**default_prefs, **custom_prefs}

        now = datetime.now(timezone.utc).isoformat()
        preferences_id = f"prefs_{uuid.uuid4().hex[:12]}"

        return self.success(
            {
                "preferences_id": preferences_id,
                "user_id": user_id,
                "plan": plan,
                "preferences": final_prefs,
                "defaults_applied": len(default_prefs),
                "customizations": len(custom_prefs),
                "status": "active",
                "created_at": now,
                "updated_at": now,
            },
            {
                "operation": "initialize_preferences",
                "service": "preferences_service",
                "plan": plan,
                "custom_preferences_count": len(custom_prefs),
            },
        )
