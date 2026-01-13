/**
 * Microservices Coordination Step Handlers for Blog Post 03.
 *
 * Implements the complete user registration workflow:
 * 1. CreateUserAccount: Create user with idempotency (root)
 * 2. SetupBillingProfile: Setup billing (parallel with preferences)
 * 3. InitializePreferences: Set user preferences (parallel with billing)
 * 4. SendWelcomeSequence: Send welcome messages (convergence)
 * 5. UpdateUserStatus: Mark user as active (final)
 *
 * TAS-137 Best Practices Demonstrated:
 * - getInput() for task context access
 * - getDependencyResult() for upstream step results
 * - getDependencyField() for nested field extraction
 * - ErrorType.PERMANENT_ERROR for non-recoverable failures
 */

import { StepHandler } from '../../../../../../src/handler/base.js';
import { ErrorType } from '../../../../../../src/types/error-type.js';
import type { StepContext } from '../../../../../../src/types/step-context.js';
import type { StepHandlerResult } from '../../../../../../src/types/step-handler-result.js';

// =============================================================================
// Types
// =============================================================================

interface UserInfo {
  email: string;
  name: string;
  plan?: string;
  phone?: string;
  source?: string;
  preferences?: Record<string, unknown>;
}

interface UserData {
  user_id: string;
  email: string;
  name?: string;
  plan: string;
  status: string;
  created_at: string;
}

interface BillingData {
  billing_id?: string;
  user_id: string;
  plan: string;
  price?: number;
  billing_required: boolean;
  status: string;
  next_billing_date?: string;
}

interface PreferencesData {
  preferences_id: string;
  user_id: string;
  preferences: Record<string, unknown>;
}

interface WelcomeData {
  user_id: string;
  channels_used: string[];
  messages_sent: number;
  status: string;
}

// =============================================================================
// Simulated Data
// =============================================================================

// Simulated existing users for idempotency demonstration
const EXISTING_USERS: Record<string, UserData> = {
  'existing@example.com': {
    user_id: 'user_existing_001',
    email: 'existing@example.com',
    name: 'Existing User',
    plan: 'free',
    status: 'active',
    created_at: '2025-01-01T00:00:00Z',
  },
};

// Billing tiers configuration
const BILLING_TIERS: Record<
  string,
  { price: number; features: string[]; billing_required: boolean }
> = {
  free: { price: 0, features: ['basic_features'], billing_required: false },
  pro: { price: 29.99, features: ['basic_features', 'advanced_analytics'], billing_required: true },
  enterprise: {
    price: 299.99,
    features: ['basic_features', 'advanced_analytics', 'priority_support', 'custom_integrations'],
    billing_required: true,
  },
};

// Default preferences by plan
const DEFAULT_PREFERENCES: Record<string, Record<string, unknown>> = {
  free: {
    email_notifications: true,
    marketing_emails: false,
    product_updates: true,
    weekly_digest: false,
    theme: 'light',
    language: 'en',
    timezone: 'UTC',
  },
  pro: {
    email_notifications: true,
    marketing_emails: true,
    product_updates: true,
    weekly_digest: true,
    theme: 'dark',
    language: 'en',
    timezone: 'UTC',
    api_notifications: true,
  },
  enterprise: {
    email_notifications: true,
    marketing_emails: true,
    product_updates: true,
    weekly_digest: true,
    theme: 'dark',
    language: 'en',
    timezone: 'UTC',
    api_notifications: true,
    audit_logs: true,
    advanced_reports: true,
  },
};

// Welcome message templates
const WELCOME_TEMPLATES: Record<
  string,
  { subject: string; greeting: string; highlights: string[]; upgrade_prompt: string | null }
> = {
  free: {
    subject: 'Welcome to Our Platform!',
    greeting: 'Thanks for joining us',
    highlights: ['Get started with basic features', 'Explore your dashboard', 'Join our community'],
    upgrade_prompt: 'Upgrade to Pro for advanced features',
  },
  pro: {
    subject: 'Welcome to Pro!',
    greeting: 'Thanks for upgrading to Pro',
    highlights: [
      'Access advanced analytics',
      'Priority support',
      'API access',
      'Custom integrations',
    ],
    upgrade_prompt: 'Consider Enterprise for dedicated support',
  },
  enterprise: {
    subject: 'Welcome to Enterprise!',
    greeting: 'Welcome to your Enterprise account',
    highlights: [
      'Dedicated account manager',
      'Custom SLA',
      'Advanced security features',
      'Priority support 24/7',
    ],
    upgrade_prompt: null,
  },
};

// =============================================================================
// Helper Functions
// =============================================================================

function generateId(prefix: string): string {
  const hex = Math.random().toString(16).substring(2, 14);
  return `${prefix}_${hex}`;
}

function isValidEmail(email: string): boolean {
  const pattern = /^[\w+\-.]+@[a-z\d-]+(\.[a-z\d-]+)*\.[a-z]+$/i;
  return pattern.test(email);
}

// =============================================================================
// Handlers
// =============================================================================

/**
 * Create user account with idempotency handling.
 *
 * Root step - no dependencies. Demonstrates idempotent operations.
 */
export class CreateUserAccountHandler extends StepHandler {
  static handlerName = 'Microservices.StepHandlers.CreateUserAccountHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // TAS-137: Use getInput() for task context access
    const userInfo = (context.getInput('user_info') || {}) as UserInfo;

    // Validate required fields
    if (!userInfo.email) {
      return this.failure(
        'Email is required but was not provided',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    if (!userInfo.name) {
      return this.failure(
        'Name is required but was not provided',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    if (!isValidEmail(userInfo.email)) {
      return this.failure(
        `Invalid email format: ${userInfo.email}`,
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    const plan = userInfo.plan || 'free';
    const source = userInfo.source || 'web';

    // Check for existing user (idempotency)
    if (EXISTING_USERS[userInfo.email]) {
      const existing = EXISTING_USERS[userInfo.email];
      if (existing.name === userInfo.name && existing.plan === plan) {
        // Idempotent success
        return this.success(
          {
            user_id: existing.user_id,
            email: existing.email,
            plan: existing.plan,
            status: 'already_exists',
            created_at: existing.created_at,
          },
          { operation: 'create_user', service: 'user_service', idempotent: true }
        );
      } else {
        return this.failure(
          `User with email ${userInfo.email} already exists with different data`,
          ErrorType.PERMANENT_ERROR,
          false
        );
      }
    }

    // Create new user
    const now = new Date().toISOString();
    const userId = generateId('user');

    return this.success(
      {
        user_id: userId,
        email: userInfo.email,
        name: userInfo.name,
        plan,
        phone: userInfo.phone,
        source,
        status: 'created',
        created_at: now,
      },
      { operation: 'create_user', service: 'user_service', idempotent: false }
    );
  }
}

/**
 * Setup billing profile with graceful degradation for free plans.
 *
 * Parallel step - depends on create_user_account.
 */
export class SetupBillingProfileHandler extends StepHandler {
  static handlerName = 'Microservices.StepHandlers.SetupBillingProfileHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // TAS-137: Get dependency result
    const userData = context.getDependencyResult('create_user_account') as UserData | null;

    if (!userData) {
      return this.failure(
        'User data not found from create_user_account step',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    const userId = userData.user_id;
    const plan = userData.plan || 'free';
    const tierConfig = BILLING_TIERS[plan] || BILLING_TIERS.free;

    if (tierConfig.billing_required) {
      // Paid plan - create billing profile
      const now = new Date();
      const nextBilling = new Date(now.getTime() + 30 * 24 * 60 * 60 * 1000);

      return this.success(
        {
          billing_id: generateId('billing'),
          user_id: userId,
          plan,
          price: tierConfig.price,
          currency: 'USD',
          billing_cycle: 'monthly',
          features: tierConfig.features,
          status: 'active',
          next_billing_date: nextBilling.toISOString(),
          created_at: now.toISOString(),
        },
        { operation: 'setup_billing', service: 'billing_service', plan, billing_required: true }
      );
    } else {
      // Free plan - graceful degradation
      return this.success(
        {
          user_id: userId,
          plan,
          billing_required: false,
          status: 'skipped_free_plan',
          message: 'Free plan users do not require billing setup',
        },
        { operation: 'setup_billing', service: 'billing_service', plan, billing_required: false }
      );
    }
  }
}

/**
 * Initialize user preferences with plan-based defaults.
 *
 * Parallel step - depends on create_user_account.
 */
export class InitializePreferencesHandler extends StepHandler {
  static handlerName = 'Microservices.StepHandlers.InitializePreferencesHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // TAS-137: Get dependency result
    const userData = context.getDependencyResult('create_user_account') as UserData | null;

    if (!userData) {
      return this.failure(
        'User data not found from create_user_account step',
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    const userId = userData.user_id;
    const plan = userData.plan || 'free';

    // Get custom preferences from task input
    const userInfo = (context.getInput('user_info') || {}) as UserInfo;
    const customPrefs = userInfo.preferences || {};

    // Merge with defaults
    const defaultPrefs = DEFAULT_PREFERENCES[plan] || DEFAULT_PREFERENCES.free;
    const finalPrefs = { ...defaultPrefs, ...customPrefs };

    const now = new Date().toISOString();

    return this.success(
      {
        preferences_id: generateId('prefs'),
        user_id: userId,
        plan,
        preferences: finalPrefs,
        defaults_applied: Object.keys(defaultPrefs).length,
        customizations: Object.keys(customPrefs).length,
        status: 'active',
        created_at: now,
        updated_at: now,
      },
      {
        operation: 'initialize_preferences',
        service: 'preferences_service',
        plan,
        custom_preferences_count: Object.keys(customPrefs).length,
      }
    );
  }
}

/**
 * Send welcome sequence via multiple notification channels.
 *
 * Convergence step - depends on both setup_billing_profile and initialize_preferences.
 */
export class SendWelcomeSequenceHandler extends StepHandler {
  static handlerName = 'Microservices.StepHandlers.SendWelcomeSequenceHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // TAS-137: Get results from prior steps (convergence point)
    const userData = context.getDependencyResult('create_user_account') as UserData | null;
    const billingData = context.getDependencyResult('setup_billing_profile') as BillingData | null;
    const preferencesData = context.getDependencyResult(
      'initialize_preferences'
    ) as PreferencesData | null;

    // Validate all prior steps
    const missing: string[] = [];
    if (!userData) missing.push('create_user_account');
    if (!billingData) missing.push('setup_billing_profile');
    if (!preferencesData) missing.push('initialize_preferences');

    if (missing.length > 0) {
      return this.failure(
        `Missing results from steps: ${missing.join(', ')}`,
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    // After null checks, we know these are defined
    const validUserData = userData as UserData;
    const validBillingData = billingData as BillingData;
    const validPreferencesData = preferencesData as PreferencesData;

    const userId = validUserData.user_id;
    const email = validUserData.email;
    const plan = validUserData.plan || 'free';
    const prefs = validPreferencesData.preferences || {};

    const template = WELCOME_TEMPLATES[plan] || WELCOME_TEMPLATES.free;
    const channelsUsed: string[] = [];
    const messagesSent: Array<Record<string, unknown>> = [];
    const now = new Date().toISOString();

    // Email (if enabled)
    if (prefs.email_notifications !== false) {
      channelsUsed.push('email');
      messagesSent.push({
        channel: 'email',
        to: email,
        subject: template.subject,
        body: this.buildEmailBody(template, userId, plan, validBillingData),
        sent_at: now,
      });
    }

    // In-app notification (always)
    channelsUsed.push('in_app');
    messagesSent.push({
      channel: 'in_app',
      user_id: userId,
      title: template.subject,
      message: template.greeting,
      sent_at: now,
    });

    // SMS (enterprise only)
    if (plan === 'enterprise') {
      channelsUsed.push('sms');
      messagesSent.push({
        channel: 'sms',
        to: '+1-555-ENTERPRISE',
        message: 'Welcome to Enterprise! Your account manager will contact you soon.',
        sent_at: now,
      });
    }

    return this.success(
      {
        user_id: userId,
        plan,
        channels_used: channelsUsed,
        messages_sent: messagesSent.length,
        welcome_sequence_id: generateId('welcome'),
        status: 'sent',
        sent_at: now,
      },
      {
        operation: 'send_welcome_sequence',
        service: 'notification_service',
        plan,
        channels_used: channelsUsed.length,
      }
    );
  }

  private buildEmailBody(
    template: (typeof WELCOME_TEMPLATES)[string],
    userId: string,
    plan: string,
    billingData: BillingData
  ): string {
    const parts: string[] = [template.greeting, '', 'Here are your account highlights:'];

    for (const highlight of template.highlights) {
      parts.push(`- ${highlight}`);
    }

    if (plan !== 'free' && billingData.billing_id) {
      parts.push('', `Billing ID: ${billingData.billing_id}`);
      parts.push(`Next billing date: ${billingData.next_billing_date}`);
    }

    if (template.upgrade_prompt) {
      parts.push('', template.upgrade_prompt);
    }

    parts.push('', `User ID: ${userId}`);

    return parts.join('\n');
  }
}

/**
 * Update user status to active after workflow completion.
 *
 * Final step - depends on send_welcome_sequence.
 */
export class UpdateUserStatusHandler extends StepHandler {
  static handlerName = 'Microservices.StepHandlers.UpdateUserStatusHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // Collect results from all prior steps
    const userData = context.getDependencyResult('create_user_account') as UserData | null;
    const billingData = context.getDependencyResult('setup_billing_profile') as BillingData | null;
    const preferencesData = context.getDependencyResult(
      'initialize_preferences'
    ) as PreferencesData | null;
    const welcomeData = context.getDependencyResult('send_welcome_sequence') as WelcomeData | null;

    // Validate all prior steps completed
    const missing: string[] = [];
    if (!userData) missing.push('create_user_account');
    if (!billingData) missing.push('setup_billing_profile');
    if (!preferencesData) missing.push('initialize_preferences');
    if (!welcomeData) missing.push('send_welcome_sequence');

    if (missing.length > 0) {
      return this.failure(
        `Cannot complete registration: missing results from steps: ${missing.join(', ')}`,
        ErrorType.PERMANENT_ERROR,
        false
      );
    }

    // After null checks, we know these are defined
    const validUserData = userData as UserData;
    const validBillingData = billingData as BillingData;
    const validPreferencesData = preferencesData as PreferencesData;
    const validWelcomeData = welcomeData as WelcomeData;

    const userId = validUserData.user_id;
    const email = validUserData.email;
    const plan = validUserData.plan || 'free';

    // Build registration summary
    const summary = this.buildRegistrationSummary(
      userId,
      email,
      plan,
      validUserData,
      validBillingData,
      validPreferencesData,
      validWelcomeData
    );

    const now = new Date().toISOString();

    return this.success(
      {
        user_id: userId,
        status: 'active',
        plan,
        registration_summary: summary,
        activation_timestamp: now,
        all_services_coordinated: true,
        services_completed: [
          'user_service',
          'billing_service',
          'preferences_service',
          'notification_service',
        ],
      },
      {
        operation: 'update_user_status',
        service: 'user_service',
        plan,
        workflow_complete: true,
      }
    );
  }

  private buildRegistrationSummary(
    userId: string,
    email: string,
    plan: string,
    userData: UserData,
    billingData: BillingData,
    preferencesData: PreferencesData,
    welcomeData: WelcomeData
  ): Record<string, unknown> {
    const summary: Record<string, unknown> = {
      user_id: userId,
      email,
      plan,
      registration_status: 'complete',
    };

    if (plan !== 'free' && billingData.billing_id) {
      summary.billing_id = billingData.billing_id;
      summary.next_billing_date = billingData.next_billing_date;
    }

    const prefs = preferencesData.preferences || {};
    summary.preferences_count = Object.keys(prefs).length;

    summary.welcome_sent = true;
    summary.notification_channels = welcomeData.channels_used;

    summary.user_created_at = userData.created_at;
    summary.registration_completed_at = new Date().toISOString();

    return summary;
  }
}
