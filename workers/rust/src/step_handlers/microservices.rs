//! # Microservices Coordination Step Handlers for Blog Post 03
//!
//! Implements the complete user registration workflow demonstrating multi-service coordination:
//! 1. CreateUserAccountHandler: Create user with idempotency (root)
//! 2. SetupBillingProfileHandler: Setup billing (parallel with preferences)
//! 3. InitializePreferencesHandler: Set user preferences (parallel with billing)
//! 4. SendWelcomeSequenceHandler: Send welcome messages (convergence)
//! 5. UpdateUserStatusHandler: Mark user as active (final)
//!
//! TAS-137 Best Practices Demonstrated:
//! - get_input() for task context access
//! - get_dependency_field() for nested field extraction
//! - get_dependency_result() for upstream step data
//!
//! TAS-91: Blog Post 03 - Rust implementation

use anyhow::Result;
use async_trait::async_trait;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::TaskSequenceStep;
use tracing::{error, info};
use uuid::Uuid;

use super::{error_result, success_result, RustStepHandler, StepHandlerConfig};

// =============================================================================
// Data Types
// =============================================================================

/// User information from task input
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserInfo {
    pub email: String,
    pub name: String,
    #[serde(default)]
    pub plan: Option<String>,
    #[serde(default)]
    pub phone: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub preferences: Option<Value>,
}

// =============================================================================
// Helper Functions
// =============================================================================

fn generate_id(prefix: &str) -> String {
    format!(
        "{}_{}",
        prefix,
        &Uuid::new_v4().to_string().replace('-', "")[..12]
    )
}

/// Validates email format (basic validation)
fn is_valid_email(email: &str) -> bool {
    let email = email.to_lowercase();
    email.contains('@') && email.contains('.') && email.len() >= 5
}

/// Billing tiers configuration
fn get_billing_tier(plan: &str) -> (f64, Vec<&'static str>, bool) {
    match plan {
        "pro" => (29.99, vec!["basic_features", "advanced_analytics"], true),
        "enterprise" => (
            299.99,
            vec![
                "basic_features",
                "advanced_analytics",
                "priority_support",
                "custom_integrations",
            ],
            true,
        ),
        _ => (0.0, vec!["basic_features"], false), // free
    }
}

/// Default preferences by plan
fn get_default_preferences(plan: &str) -> Value {
    match plan {
        "pro" => json!({
            "email_notifications": true,
            "marketing_emails": true,
            "product_updates": true,
            "weekly_digest": true,
            "theme": "dark",
            "language": "en",
            "timezone": "UTC",
            "api_notifications": true
        }),
        "enterprise" => json!({
            "email_notifications": true,
            "marketing_emails": true,
            "product_updates": true,
            "weekly_digest": true,
            "theme": "dark",
            "language": "en",
            "timezone": "UTC",
            "api_notifications": true,
            "audit_logs": true,
            "advanced_reports": true
        }),
        _ => json!({
            "email_notifications": true,
            "marketing_emails": false,
            "product_updates": true,
            "weekly_digest": false,
            "theme": "light",
            "language": "en",
            "timezone": "UTC"
        }),
    }
}

/// Welcome message templates
fn get_welcome_template(plan: &str) -> (String, String, Vec<String>, Option<String>) {
    match plan {
        "pro" => (
            "Welcome to Pro!".to_string(),
            "Thanks for upgrading to Pro".to_string(),
            vec![
                "Access advanced analytics".to_string(),
                "Priority support".to_string(),
                "API access".to_string(),
                "Custom integrations".to_string(),
            ],
            Some("Consider Enterprise for dedicated support".to_string()),
        ),
        "enterprise" => (
            "Welcome to Enterprise!".to_string(),
            "Welcome to your Enterprise account".to_string(),
            vec![
                "Dedicated account manager".to_string(),
                "Custom SLA".to_string(),
                "Advanced security features".to_string(),
                "Priority support 24/7".to_string(),
            ],
            None,
        ),
        _ => (
            "Welcome to Our Platform!".to_string(),
            "Thanks for joining us".to_string(),
            vec![
                "Get started with basic features".to_string(),
                "Explore your dashboard".to_string(),
                "Join our community".to_string(),
            ],
            Some("Upgrade to Pro for advanced features".to_string()),
        ),
    }
}

// =============================================================================
// Step 1: Create User Account Handler
// =============================================================================

/// Create user account with idempotency handling.
///
/// Root step - no dependencies. Demonstrates idempotent operations.
#[derive(Debug)]
pub struct CreateUserAccountHandler {
    #[expect(
        dead_code,
        reason = "Required for API compatibility with handler config pattern"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for CreateUserAccountHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // TAS-137: Use get_input() for task context access
        let user_info: UserInfo = match step_data.get_input("user_info") {
            Ok(info) => info,
            Err(e) => {
                error!("Missing user_info in task context: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "User info is required".to_string(),
                    Some("MISSING_USER_INFO".to_string()),
                    Some("ValidationError".to_string()),
                    false,
                    start_time.elapsed().as_millis() as i64,
                    None,
                ));
            }
        };

        // Validate required fields
        if user_info.email.trim().is_empty() {
            return Ok(error_result(
                step_uuid,
                "Email is required but was not provided".to_string(),
                Some("MISSING_EMAIL".to_string()),
                Some("ValidationError".to_string()),
                false,
                start_time.elapsed().as_millis() as i64,
                None,
            ));
        }

        if user_info.name.trim().is_empty() {
            return Ok(error_result(
                step_uuid,
                "Name is required but was not provided".to_string(),
                Some("MISSING_NAME".to_string()),
                Some("ValidationError".to_string()),
                false,
                start_time.elapsed().as_millis() as i64,
                None,
            ));
        }

        // Validate email format
        if !is_valid_email(&user_info.email) {
            return Ok(error_result(
                step_uuid,
                format!("Invalid email format: {}", user_info.email),
                Some("INVALID_EMAIL_FORMAT".to_string()),
                Some("ValidationError".to_string()),
                false,
                start_time.elapsed().as_millis() as i64,
                None,
            ));
        }

        let plan = user_info.plan.as_deref().unwrap_or("free");
        let source = user_info.source.as_deref().unwrap_or("web");

        // Check for existing user (idempotency - simulated)
        // In a real system, this would be a database lookup
        if user_info.email == "existing@example.com" {
            // Simulated idempotent success
            info!(
                "User {} already exists - returning idempotent success",
                user_info.email
            );
            return Ok(success_result(
                step_uuid,
                json!({
                    "user_id": "user_existing_001",
                    "email": user_info.email,
                    "plan": plan,
                    "status": "already_exists",
                    "created_at": "2025-01-01T00:00:00Z"
                }),
                start_time.elapsed().as_millis() as i64,
                Some(HashMap::from([
                    ("operation".to_string(), json!("create_user")),
                    ("service".to_string(), json!("user_service")),
                    ("idempotent".to_string(), json!(true)),
                ])),
            ));
        }

        // Create new user
        let now = Utc::now().to_rfc3339();
        let user_id = generate_id("user");

        let mut result = json!({
            "user_id": user_id,
            "email": user_info.email,
            "name": user_info.name,
            "plan": plan,
            "source": source,
            "status": "created",
            "created_at": now
        });

        if let Some(phone) = &user_info.phone {
            result["phone"] = json!(phone);
        }

        info!(
            "User created: {} ({}) with plan {}",
            user_info.email, user_id, plan
        );

        Ok(success_result(
            step_uuid,
            result,
            start_time.elapsed().as_millis() as i64,
            Some(HashMap::from([
                ("operation".to_string(), json!("create_user")),
                ("service".to_string(), json!("user_service")),
                ("idempotent".to_string(), json!(false)),
            ])),
        ))
    }

    fn name(&self) -> &str {
        "microservices_create_user_account"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

// =============================================================================
// Step 2: Setup Billing Profile Handler
// =============================================================================

/// Setup billing profile with graceful degradation for free plans.
///
/// Parallel step - depends on create_user_account.
#[derive(Debug)]
pub struct SetupBillingProfileHandler {
    #[expect(
        dead_code,
        reason = "Required for API compatibility with handler config pattern"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for SetupBillingProfileHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // TAS-137: Get dependency field from create_user_account
        let user_id: String =
            match step_data.get_dependency_field("create_user_account", &["user_id"]) {
                Ok(id) => id,
                Err(e) => {
                    error!("Missing user_id from create_user_account: {}", e);
                    return Ok(error_result(
                        step_uuid,
                        "User data not found from create_user_account step".to_string(),
                        Some("MISSING_USER_DATA".to_string()),
                        Some("DependencyError".to_string()),
                        true,
                        start_time.elapsed().as_millis() as i64,
                        None,
                    ));
                }
            };

        let plan: String = step_data
            .get_dependency_field("create_user_account", &["plan"])
            .unwrap_or_else(|_| "free".to_string());

        let (price, features, billing_required) = get_billing_tier(&plan);

        if billing_required {
            // Paid plan - create billing profile
            let now = Utc::now();
            let next_billing = (now + Duration::days(30)).to_rfc3339();
            let billing_id = generate_id("billing");

            info!(
                "Billing profile created for user {}: ${:.2}/month",
                user_id, price
            );

            Ok(success_result(
                step_uuid,
                json!({
                    "billing_id": billing_id,
                    "user_id": user_id,
                    "plan": plan,
                    "price": price,
                    "currency": "USD",
                    "billing_cycle": "monthly",
                    "features": features,
                    "status": "active",
                    "next_billing_date": next_billing,
                    "created_at": now.to_rfc3339()
                }),
                start_time.elapsed().as_millis() as i64,
                Some(HashMap::from([
                    ("operation".to_string(), json!("setup_billing")),
                    ("service".to_string(), json!("billing_service")),
                    ("plan".to_string(), json!(plan)),
                    ("billing_required".to_string(), json!(true)),
                ])),
            ))
        } else {
            // Free plan - graceful degradation
            info!("Billing skipped for user {} (free plan)", user_id);

            Ok(success_result(
                step_uuid,
                json!({
                    "user_id": user_id,
                    "plan": plan,
                    "billing_required": false,
                    "status": "skipped_free_plan",
                    "message": "Free plan users do not require billing setup"
                }),
                start_time.elapsed().as_millis() as i64,
                Some(HashMap::from([
                    ("operation".to_string(), json!("setup_billing")),
                    ("service".to_string(), json!("billing_service")),
                    ("plan".to_string(), json!(plan)),
                    ("billing_required".to_string(), json!(false)),
                ])),
            ))
        }
    }

    fn name(&self) -> &str {
        "microservices_setup_billing_profile"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

// =============================================================================
// Step 3: Initialize Preferences Handler
// =============================================================================

/// Initialize user preferences with plan-based defaults.
///
/// Parallel step - depends on create_user_account.
#[derive(Debug)]
pub struct InitializePreferencesHandler {
    #[expect(
        dead_code,
        reason = "Required for API compatibility with handler config pattern"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for InitializePreferencesHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // TAS-137: Get dependency fields from create_user_account
        let user_id: String =
            match step_data.get_dependency_field("create_user_account", &["user_id"]) {
                Ok(id) => id,
                Err(e) => {
                    error!("Missing user_id from create_user_account: {}", e);
                    return Ok(error_result(
                        step_uuid,
                        "User data not found from create_user_account step".to_string(),
                        Some("MISSING_USER_DATA".to_string()),
                        Some("DependencyError".to_string()),
                        true,
                        start_time.elapsed().as_millis() as i64,
                        None,
                    ));
                }
            };

        let plan: String = step_data
            .get_dependency_field("create_user_account", &["plan"])
            .unwrap_or_else(|_| "free".to_string());

        // Get custom preferences from task input (optional)
        let custom_prefs: Option<Value> = step_data
            .get_input::<UserInfo>("user_info")
            .ok()
            .and_then(|u| u.preferences);

        // Merge with defaults
        let default_prefs = get_default_preferences(&plan);
        let mut final_prefs = default_prefs.as_object().unwrap().clone();

        let custom_count =
            if let Some(custom_obj) = custom_prefs.as_ref().and_then(|v| v.as_object()) {
                for (k, v) in custom_obj {
                    final_prefs.insert(k.clone(), v.clone());
                }
                custom_obj.len()
            } else {
                0
            };

        let now = Utc::now().to_rfc3339();
        let preferences_id = generate_id("prefs");

        info!(
            "Preferences initialized for user {}: {} defaults + {} customizations",
            user_id,
            final_prefs.len(),
            custom_count
        );

        Ok(success_result(
            step_uuid,
            json!({
                "preferences_id": preferences_id,
                "user_id": user_id,
                "plan": plan,
                "preferences": final_prefs,
                "defaults_applied": default_prefs.as_object().map(|o| o.len()).unwrap_or(0),
                "customizations": custom_count,
                "status": "active",
                "created_at": now,
                "updated_at": now
            }),
            start_time.elapsed().as_millis() as i64,
            Some(HashMap::from([
                ("operation".to_string(), json!("initialize_preferences")),
                ("service".to_string(), json!("preferences_service")),
                ("plan".to_string(), json!(plan)),
                ("custom_preferences_count".to_string(), json!(custom_count)),
            ])),
        ))
    }

    fn name(&self) -> &str {
        "microservices_initialize_preferences"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

// =============================================================================
// Step 4: Send Welcome Sequence Handler
// =============================================================================

/// Send welcome sequence via multiple notification channels.
///
/// Convergence step - depends on both setup_billing_profile and initialize_preferences.
#[derive(Debug)]
pub struct SendWelcomeSequenceHandler {
    #[expect(
        dead_code,
        reason = "Required for API compatibility with handler config pattern"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for SendWelcomeSequenceHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // TAS-137: Get results from prior steps (convergence point)
        let user_id: String =
            match step_data.get_dependency_field("create_user_account", &["user_id"]) {
                Ok(id) => id,
                Err(e) => {
                    error!("Missing user_id from create_user_account: {}", e);
                    return Ok(error_result(
                        step_uuid,
                        "Missing user data from create_user_account".to_string(),
                        Some("MISSING_DEPENDENCY_DATA".to_string()),
                        Some("DependencyError".to_string()),
                        true,
                        start_time.elapsed().as_millis() as i64,
                        None,
                    ));
                }
            };

        let email: String = step_data
            .get_dependency_field("create_user_account", &["email"])
            .unwrap_or_else(|_| "unknown@example.com".to_string());

        let plan: String = step_data
            .get_dependency_field("create_user_account", &["plan"])
            .unwrap_or_else(|_| "free".to_string());

        // Get billing ID if available (optional for free plans)
        let billing_id: Option<String> = step_data
            .get_dependency_field("setup_billing_profile", &["billing_id"])
            .ok();

        // Get preferences for email notification setting
        let email_notifications_enabled: bool = step_data
            .get_dependency_field(
                "initialize_preferences",
                &["preferences", "email_notifications"],
            )
            .unwrap_or(true);

        let (subject, greeting, highlights, upgrade_prompt) = get_welcome_template(&plan);
        let mut channels_used = Vec::new();
        let mut messages_sent = Vec::new();
        let now = Utc::now().to_rfc3339();

        // Email (if enabled)
        if email_notifications_enabled {
            channels_used.push("email".to_string());
            messages_sent.push(json!({
                "channel": "email",
                "to": email,
                "subject": subject,
                "sent_at": now
            }));
        }

        // In-app notification (always)
        channels_used.push("in_app".to_string());
        messages_sent.push(json!({
            "channel": "in_app",
            "user_id": user_id,
            "title": subject,
            "message": greeting,
            "sent_at": now
        }));

        // SMS (enterprise only)
        if plan == "enterprise" {
            channels_used.push("sms".to_string());
            messages_sent.push(json!({
                "channel": "sms",
                "to": "+1-555-ENTERPRISE",
                "message": "Welcome to Enterprise! Your account manager will contact you soon.",
                "sent_at": now
            }));
        }

        let welcome_sequence_id = generate_id("welcome");

        info!(
            "Welcome sequence sent to user {}: {} channels",
            user_id,
            channels_used.len()
        );

        Ok(success_result(
            step_uuid,
            json!({
                "user_id": user_id,
                "plan": plan,
                "channels_used": channels_used,
                "messages_sent": messages_sent.len(),
                "welcome_sequence_id": welcome_sequence_id,
                "status": "sent",
                "sent_at": now,
                "highlights": highlights,
                "upgrade_prompt": upgrade_prompt,
                "billing_id": billing_id
            }),
            start_time.elapsed().as_millis() as i64,
            Some(HashMap::from([
                ("operation".to_string(), json!("send_welcome_sequence")),
                ("service".to_string(), json!("notification_service")),
                ("plan".to_string(), json!(plan)),
                ("channels_used".to_string(), json!(channels_used.len())),
            ])),
        ))
    }

    fn name(&self) -> &str {
        "microservices_send_welcome_sequence"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

// =============================================================================
// Step 5: Update User Status Handler
// =============================================================================

/// Update user status to active after workflow completion.
///
/// Final step - depends on send_welcome_sequence.
#[derive(Debug)]
pub struct UpdateUserStatusHandler {
    #[expect(
        dead_code,
        reason = "Required for API compatibility with handler config pattern"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for UpdateUserStatusHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Collect results from all prior steps
        let user_id: String =
            match step_data.get_dependency_field("create_user_account", &["user_id"]) {
                Ok(id) => id,
                Err(e) => {
                    error!("Missing user_id from create_user_account: {}", e);
                    return Ok(error_result(
                        step_uuid,
                        "Cannot complete registration: missing user data".to_string(),
                        Some("INCOMPLETE_WORKFLOW".to_string()),
                        Some("WorkflowError".to_string()),
                        false,
                        start_time.elapsed().as_millis() as i64,
                        None,
                    ));
                }
            };

        let email: String = step_data
            .get_dependency_field("create_user_account", &["email"])
            .unwrap_or_else(|_| "unknown@example.com".to_string());

        let plan: String = step_data
            .get_dependency_field("create_user_account", &["plan"])
            .unwrap_or_else(|_| "free".to_string());

        let user_created_at: Option<String> = step_data
            .get_dependency_field("create_user_account", &["created_at"])
            .ok();

        // Get billing info if available
        let billing_id: Option<String> = step_data
            .get_dependency_field("setup_billing_profile", &["billing_id"])
            .ok();
        let next_billing_date: Option<String> = step_data
            .get_dependency_field("setup_billing_profile", &["next_billing_date"])
            .ok();

        // Get preferences count
        let prefs_count: usize = step_data
            .get_dependency_field::<Value>("initialize_preferences", &["preferences"])
            .ok()
            .map(|p| p.as_object().map(|o| o.len()).unwrap_or(0))
            .unwrap_or(0);

        // Get welcome info
        let notification_channels: Value = step_data
            .get_dependency_field("send_welcome_sequence", &["channels_used"])
            .unwrap_or_else(|_| json!([]));

        // Build registration summary
        let mut summary = json!({
            "user_id": user_id,
            "email": email,
            "plan": plan,
            "registration_status": "complete"
        });

        if plan != "free" {
            if let Some(bid) = &billing_id {
                summary["billing_id"] = json!(bid);
            }
            if let Some(nbd) = &next_billing_date {
                summary["next_billing_date"] = json!(nbd);
            }
        }

        summary["preferences_count"] = json!(prefs_count);
        summary["welcome_sent"] = json!(true);
        summary["notification_channels"] = notification_channels.clone();

        if let Some(created) = &user_created_at {
            summary["user_created_at"] = json!(created);
        }

        let now = Utc::now().to_rfc3339();
        summary["registration_completed_at"] = json!(now);

        info!(
            "User registration completed: {} ({}) - {} plan",
            email, user_id, plan
        );

        Ok(success_result(
            step_uuid,
            json!({
                "user_id": user_id,
                "status": "active",
                "plan": plan,
                "registration_summary": summary,
                "activation_timestamp": now,
                "all_services_coordinated": true,
                "services_completed": [
                    "user_service",
                    "billing_service",
                    "preferences_service",
                    "notification_service"
                ]
            }),
            start_time.elapsed().as_millis() as i64,
            Some(HashMap::from([
                ("operation".to_string(), json!("update_user_status")),
                ("service".to_string(), json!("user_service")),
                ("plan".to_string(), json!(plan)),
                ("workflow_complete".to_string(), json!(true)),
            ])),
        ))
    }

    fn name(&self) -> &str {
        "microservices_update_user_status"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}
