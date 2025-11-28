//! # TAS-65 Phase 1.2: Event Publication Validator
//!
//! Validates event declarations at template load time.
//!
//! ## Overview
//!
//! The EventPublicationValidator ensures that all event declarations in a task template
//! are valid before the template is used. This prevents runtime errors and provides
//! clear feedback during template loading.
//!
//! ## Validation Rules
//!
//! For each event declaration:
//! - Event name must follow dot notation (namespace.action)
//! - JSON Schema must be valid and compilable
//! - Custom publisher names must be valid Ruby class names
//! - No duplicate event names within a step
//!
//! For the entire template:
//! - All step event declarations must be valid
//! - Event names should be unique within a step (warning if duplicates)
//!
//! ## Usage
//!
//! ```rust,no_run
//! use tasker_shared::models::core::task_template::{TaskTemplate, EventPublicationValidator};
//!
//! # fn example(template: TaskTemplate) -> Result<(), String> {
//! // Validate all event declarations in the template
//! EventPublicationValidator::validate_template(&template)?;
//! # Ok(())
//! # }
//! ```

use super::{EventDeclaration, StepDefinition, TaskTemplate};
use std::collections::HashSet;

/// Validation result with detailed error information
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationResult {
    /// Whether validation passed
    pub valid: bool,
    /// Validation errors (if any)
    pub errors: Vec<ValidationError>,
    /// Validation warnings (non-fatal issues)
    pub warnings: Vec<ValidationWarning>,
}

impl ValidationResult {
    /// Create a successful validation result
    pub fn success() -> Self {
        Self {
            valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Create a failed validation result with errors
    pub fn failure(errors: Vec<ValidationError>) -> Self {
        Self {
            valid: false,
            errors,
            warnings: Vec::new(),
        }
    }

    /// Add a warning to the result
    pub fn with_warning(mut self, warning: ValidationWarning) -> Self {
        self.warnings.push(warning);
        self
    }

    /// Add multiple warnings to the result
    pub fn with_warnings(mut self, warnings: Vec<ValidationWarning>) -> Self {
        self.warnings.extend(warnings);
        self
    }

    /// Convert to a Result type for easier error handling
    pub fn into_result(self) -> Result<Vec<ValidationWarning>, Vec<ValidationError>> {
        if self.valid {
            Ok(self.warnings)
        } else {
            Err(self.errors)
        }
    }
}

/// Validation error with context
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationError {
    /// Step name where the error occurred
    pub step_name: String,
    /// Event name that failed validation (if applicable)
    pub event_name: Option<String>,
    /// Error message
    pub message: String,
}

impl ValidationError {
    /// Create a new validation error
    pub fn new(step_name: String, event_name: Option<String>, message: String) -> Self {
        Self {
            step_name,
            event_name,
            message,
        }
    }

    /// Create an error for a specific event
    pub fn event_error(step_name: String, event_name: String, message: String) -> Self {
        Self::new(step_name, Some(event_name), message)
    }

    /// Create an error for a step (not event-specific)
    pub fn step_error(step_name: String, message: String) -> Self {
        Self::new(step_name, None, message)
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(event_name) = &self.event_name {
            write!(
                f,
                "Step '{}', Event '{}': {}",
                self.step_name, event_name, self.message
            )
        } else {
            write!(f, "Step '{}': {}", self.step_name, self.message)
        }
    }
}

/// Validation warning (non-fatal)
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationWarning {
    /// Step name where the warning occurred
    pub step_name: String,
    /// Warning message
    pub message: String,
}

impl ValidationWarning {
    /// Create a new validation warning
    pub fn new(step_name: String, message: String) -> Self {
        Self {
            step_name,
            message,
        }
    }
}

impl std::fmt::Display for ValidationWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Step '{}': {}", self.step_name, self.message)
    }
}

/// Event publication validator
///
/// Validates event declarations at template load time to ensure all
/// events are properly configured before template execution.
#[derive(Debug)]
pub struct EventPublicationValidator;

impl EventPublicationValidator {
    /// Validate all event declarations in a task template
    ///
    /// # Arguments
    ///
    /// * `template` - The task template to validate
    ///
    /// # Returns
    ///
    /// A `ValidationResult` containing any errors or warnings
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use tasker_shared::models::core::task_template::{TaskTemplate, EventPublicationValidator};
    ///
    /// # fn example(template: TaskTemplate) -> Result<(), String> {
    /// let result = EventPublicationValidator::validate_template(&template);
    ///
    /// if !result.valid {
    ///     for error in &result.errors {
    ///         eprintln!("Error: {}", error);
    ///     }
    ///     return Err("Template validation failed".to_string());
    /// }
    ///
    /// for warning in &result.warnings {
    ///     eprintln!("Warning: {}", warning);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn validate_template(template: &TaskTemplate) -> ValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        for step in &template.steps {
            let step_result = Self::validate_step(step);
            errors.extend(step_result.errors);
            warnings.extend(step_result.warnings);
        }

        if errors.is_empty() {
            ValidationResult::success().with_warnings(warnings)
        } else {
            ValidationResult::failure(errors).with_warnings(warnings)
        }
    }

    /// Validate event declarations for a single step
    ///
    /// # Arguments
    ///
    /// * `step` - The step definition to validate
    ///
    /// # Returns
    ///
    /// A `ValidationResult` for this step's events
    pub fn validate_step(step: &StepDefinition) -> ValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();

        // Skip validation if no events declared
        if step.publishes_events.is_empty() {
            return ValidationResult::success();
        }

        // Track event names to detect duplicates
        let mut seen_events = HashSet::new();

        for event in &step.publishes_events {
            // Validate individual event
            if let Err(error_msg) = event.validate() {
                errors.push(ValidationError::event_error(
                    step.name.clone(),
                    event.name.clone(),
                    error_msg,
                ));
                continue; // Skip duplicate check if event is invalid
            }

            // Check for duplicate event names
            if !seen_events.insert(event.name.clone()) {
                warnings.push(ValidationWarning::new(
                    step.name.clone(),
                    format!(
                        "Duplicate event name '{}' - only the last declaration will be used",
                        event.name
                    ),
                ));
            }
        }

        if errors.is_empty() {
            ValidationResult::success().with_warnings(warnings)
        } else {
            ValidationResult::failure(errors).with_warnings(warnings)
        }
    }

    /// Validate a single event declaration
    ///
    /// # Arguments
    ///
    /// * `event` - The event declaration to validate
    /// * `step_name` - The name of the step containing this event
    ///
    /// # Returns
    ///
    /// `Ok(())` if valid, `Err(ValidationError)` otherwise
    pub fn validate_event(event: &EventDeclaration, step_name: &str) -> Result<(), ValidationError> {
        event.validate().map_err(|error_msg| {
            ValidationError::event_error(step_name.to_string(), event.name.clone(), error_msg)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::core::task_template::{HandlerDefinition, StepType};
    use serde_json::json;

    fn create_valid_event(name: &str) -> EventDeclaration {
        EventDeclaration::builder()
            .name(name.to_string())
            .description("Test event".to_string())
            .schema(json!({
                "type": "object",
                "properties": {"id": {"type": "string"}},
                "required": ["id"]
            }))
            .build()
    }

    fn create_invalid_event_name(name: &str) -> EventDeclaration {
        EventDeclaration::builder()
            .name(name.to_string())
            .description("Test event".to_string())
            .schema(json!({"type": "object"}))
            .build()
    }

    fn create_invalid_event_schema(name: &str) -> EventDeclaration {
        EventDeclaration::builder()
            .name(name.to_string())
            .description("Test event".to_string())
            .schema(json!({"type": "invalid_type"}))
            .build()
    }

    fn create_step_with_events(name: &str, events: Vec<EventDeclaration>) -> StepDefinition {
        StepDefinition {
            name: name.to_string(),
            description: Some("Test step".to_string()),
            handler: HandlerDefinition::builder()
                .callable("TestHandler".to_string())
                .build(),
            step_type: StepType::Standard,
            system_dependency: None,
            dependencies: Vec::new(),
            retry: Default::default(),
            timeout_seconds: None,
            publishes_events: events,
            batch_config: None,
        }
    }

    #[test]
    fn test_validate_step_no_events() {
        let step = create_step_with_events("test_step", vec![]);
        let result = EventPublicationValidator::validate_step(&step);

        assert!(result.valid);
        assert!(result.errors.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_validate_step_valid_events() {
        let events = vec![
            create_valid_event("order.created"),
            create_valid_event("order.updated"),
        ];
        let step = create_step_with_events("test_step", events);
        let result = EventPublicationValidator::validate_step(&step);

        assert!(result.valid);
        assert!(result.errors.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_validate_step_invalid_event_name() {
        let events = vec![create_invalid_event_name("invalid_no_dot")];
        let step = create_step_with_events("test_step", events);
        let result = EventPublicationValidator::validate_step(&step);

        assert!(!result.valid);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].step_name, "test_step");
        assert_eq!(result.errors[0].event_name, Some("invalid_no_dot".to_string()));
        assert!(result.errors[0].message.contains("must contain at least one dot"));
    }

    #[test]
    fn test_validate_step_invalid_event_schema() {
        let events = vec![create_invalid_event_schema("order.created")];
        let step = create_step_with_events("test_step", events);
        let result = EventPublicationValidator::validate_step(&step);

        assert!(!result.valid);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].step_name, "test_step");
        assert_eq!(result.errors[0].event_name, Some("order.created".to_string()));
        assert!(result.errors[0].message.contains("Invalid JSON Schema"));
    }

    #[test]
    fn test_validate_step_duplicate_event_names() {
        let events = vec![
            create_valid_event("order.created"),
            create_valid_event("order.created"), // Duplicate
        ];
        let step = create_step_with_events("test_step", events);
        let result = EventPublicationValidator::validate_step(&step);

        assert!(result.valid); // Duplicates are warnings, not errors
        assert!(result.errors.is_empty());
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.warnings[0].step_name, "test_step");
        assert!(result.warnings[0].message.contains("Duplicate event name"));
        assert!(result.warnings[0].message.contains("order.created"));
    }

    #[test]
    fn test_validate_step_mixed_valid_invalid() {
        let events = vec![
            create_valid_event("order.created"),
            create_invalid_event_name("invalid"),
            create_valid_event("order.updated"),
        ];
        let step = create_step_with_events("test_step", events);
        let result = EventPublicationValidator::validate_step(&step);

        assert!(!result.valid);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].event_name, Some("invalid".to_string()));
    }

    #[test]
    fn test_validate_template_all_valid() {
        let template = TaskTemplate::builder()
            .name("test_template".to_string())
            .namespace_name("test".to_string())
            .version("1.0.0".to_string())
            .steps(vec![
                create_step_with_events("step1", vec![create_valid_event("order.created")]),
                create_step_with_events("step2", vec![create_valid_event("payment.processed")]),
            ])
            .build();

        let result = EventPublicationValidator::validate_template(&template);

        assert!(result.valid);
        assert!(result.errors.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn test_validate_template_with_errors() {
        let template = TaskTemplate::builder()
            .name("test_template".to_string())
            .namespace_name("test".to_string())
            .version("1.0.0".to_string())
            .steps(vec![
                create_step_with_events("step1", vec![create_valid_event("order.created")]),
                create_step_with_events("step2", vec![create_invalid_event_name("invalid")]),
            ])
            .build();

        let result = EventPublicationValidator::validate_template(&template);

        assert!(!result.valid);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].step_name, "step2");
    }

    #[test]
    fn test_validate_template_multiple_steps_with_errors() {
        let template = TaskTemplate::builder()
            .name("test_template".to_string())
            .namespace_name("test".to_string())
            .version("1.0.0".to_string())
            .steps(vec![
                create_step_with_events("step1", vec![create_invalid_event_name("invalid1")]),
                create_step_with_events("step2", vec![create_invalid_event_schema("order.created")]),
            ])
            .build();

        let result = EventPublicationValidator::validate_template(&template);

        assert!(!result.valid);
        assert_eq!(result.errors.len(), 2);
        assert_eq!(result.errors[0].step_name, "step1");
        assert_eq!(result.errors[1].step_name, "step2");
    }

    #[test]
    fn test_validate_event_success() {
        let event = create_valid_event("order.created");
        let result = EventPublicationValidator::validate_event(&event, "test_step");

        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_event_failure() {
        let event = create_invalid_event_name("invalid");
        let result = EventPublicationValidator::validate_event(&event, "test_step");

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(error.step_name, "test_step");
        assert_eq!(error.event_name, Some("invalid".to_string()));
    }

    #[test]
    fn test_validation_result_into_result_success() {
        let result = ValidationResult::success();
        let converted = result.into_result();

        assert!(converted.is_ok());
        assert!(converted.unwrap().is_empty());
    }

    #[test]
    fn test_validation_result_into_result_with_warnings() {
        let warnings = vec![ValidationWarning::new(
            "test_step".to_string(),
            "Test warning".to_string(),
        )];
        let result = ValidationResult::success().with_warnings(warnings.clone());
        let converted = result.into_result();

        assert!(converted.is_ok());
        assert_eq!(converted.unwrap(), warnings);
    }

    #[test]
    fn test_validation_result_into_result_failure() {
        let errors = vec![ValidationError::step_error(
            "test_step".to_string(),
            "Test error".to_string(),
        )];
        let result = ValidationResult::failure(errors.clone());
        let converted = result.into_result();

        assert!(converted.is_err());
        assert_eq!(converted.unwrap_err(), errors);
    }

    #[test]
    fn test_validation_error_display() {
        let error = ValidationError::event_error(
            "test_step".to_string(),
            "order.created".to_string(),
            "Invalid event".to_string(),
        );

        let display = format!("{}", error);
        assert_eq!(display, "Step 'test_step', Event 'order.created': Invalid event");
    }

    #[test]
    fn test_validation_error_display_no_event() {
        let error = ValidationError::step_error(
            "test_step".to_string(),
            "Step error".to_string(),
        );

        let display = format!("{}", error);
        assert_eq!(display, "Step 'test_step': Step error");
    }

    #[test]
    fn test_validation_warning_display() {
        let warning = ValidationWarning::new(
            "test_step".to_string(),
            "Duplicate event".to_string(),
        );

        let display = format!("{}", warning);
        assert_eq!(display, "Step 'test_step': Duplicate event");
    }
}
