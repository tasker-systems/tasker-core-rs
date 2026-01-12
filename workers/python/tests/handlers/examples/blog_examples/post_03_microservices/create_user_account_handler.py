"""
Create User Account Handler for Blog Post 03.

Demonstrates idempotent user creation with conflict handling.

TAS-137 Best Practices:
- get_input() for task context access
- PermanentError for non-recoverable failures
"""

import re
import uuid
from datetime import datetime, timezone

from tasker_core import StepContext, StepHandler, StepHandlerResult

# Simulated existing users (represents "user service" database)
EXISTING_USERS = {
    "existing@example.com": {
        "id": "user_existing_001",
        "email": "existing@example.com",
        "name": "Existing User",
        "plan": "free",
        "created_at": "2025-01-01T00:00:00Z",
    }
}


class CreateUserAccountHandler(StepHandler):
    """Create user account with idempotency handling."""

    handler_name = "microservices.step_handlers.CreateUserAccountHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Execute the create user account step."""
        # TAS-137: Use get_input() for task context access
        user_info = context.get_input("user_info") or {}

        # Validate required fields
        email = user_info.get("email")
        name = user_info.get("name")

        if not email:
            return self.failure(
                "Email is required but was not provided",
                error_code="MISSING_EMAIL",
                retryable=False,
            )

        if not name:
            return self.failure(
                "Name is required but was not provided",
                error_code="MISSING_NAME",
                retryable=False,
            )

        # Validate email format
        email_pattern = r"^[\w+\-.]+@[a-z\d\-]+(\.[a-z\d\-]+)*\.[a-z]+$"
        if not re.match(email_pattern, email, re.IGNORECASE):
            return self.failure(
                f"Invalid email format: {email}",
                error_code="INVALID_EMAIL_FORMAT",
                retryable=False,
            )

        plan = user_info.get("plan", "free")
        phone = user_info.get("phone")
        source = user_info.get("source", "web")

        # Check for existing user (simulates 409 conflict)
        if email in EXISTING_USERS:
            existing_user = EXISTING_USERS[email]
            # Idempotent: if data matches, return success
            if self._user_matches(existing_user, email, name, plan):
                return self.success(
                    {
                        "user_id": existing_user["id"],
                        "email": existing_user["email"],
                        "plan": existing_user["plan"],
                        "status": "already_exists",
                        "created_at": existing_user["created_at"],
                    },
                    {
                        "operation": "create_user",
                        "service": "user_service",
                        "idempotent": True,
                    },
                )
            else:
                return self.failure(
                    f"User with email {email} already exists with different data",
                    error_code="USER_CONFLICT",
                    retryable=False,
                )

        # Simulate successful user creation
        now = datetime.now(timezone.utc).isoformat()
        user_id = f"user_{uuid.uuid4().hex[:12]}"

        return self.success(
            {
                "user_id": user_id,
                "email": email,
                "name": name,
                "plan": plan,
                "phone": phone,
                "source": source,
                "status": "created",
                "created_at": now,
            },
            {
                "operation": "create_user",
                "service": "user_service",
                "idempotent": False,
            },
        )

    def _user_matches(self, existing: dict, email: str, name: str, plan: str) -> bool:
        """Check if existing user matches new user data for idempotency."""
        return existing["email"] == email and existing["name"] == name and existing["plan"] == plan
