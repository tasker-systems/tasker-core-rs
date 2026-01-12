"""
Blog Post 03: Microservices Coordination Handlers.

User Registration workflow demonstrating multi-service coordination:
1. CreateUserAccountHandler: Create user with idempotency (root)
2. SetupBillingProfileHandler: Setup billing (parallel with preferences)
3. InitializePreferencesHandler: Set user preferences (parallel with billing)
4. SendWelcomeSequenceHandler: Send welcome messages (convergence)
5. UpdateUserStatusHandler: Mark user as active (final)

Patterns demonstrated:
- Idempotent operations
- Plan-based logic (free/pro/enterprise)
- Graceful degradation
- Multi-channel notifications
- Workflow completion validation
"""

from .create_user_account_handler import CreateUserAccountHandler
from .initialize_preferences_handler import InitializePreferencesHandler
from .send_welcome_handler import SendWelcomeSequenceHandler
from .setup_billing_handler import SetupBillingProfileHandler
from .update_user_status_handler import UpdateUserStatusHandler

__all__ = [
    "CreateUserAccountHandler",
    "SetupBillingProfileHandler",
    "InitializePreferencesHandler",
    "SendWelcomeSequenceHandler",
    "UpdateUserStatusHandler",
]
