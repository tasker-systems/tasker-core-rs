//! # Error Classification Integration Demo
//!
//! This example demonstrates how the new ErrorClassifier integrates with
//! StepExecutor and BaseStepHandler to provide centralized error handling.

use std::collections::HashMap;
use std::time::Duration;
use tasker_core::orchestration::{
    ErrorClassification, ErrorClassifier, ErrorContext, OrchestrationError,
    StandardErrorClassifier, StepExecutionError,
};
use uuid::Uuid;

fn main() {
    println!("=== Step Execution Error Classification Demo ===\n");

    // Create the error classifier
    let classifier = StandardErrorClassifier::new();

    // Create execution context
    let context = ErrorContext {
        step_uuid: Uuid::now_v7(),
        task_uuid: Uuid::now_v7(),
        attempt_number: 2,
        max_attempts: 5,
        execution_duration: Duration::from_secs(30),
        step_name: "process_payment".to_string(),
        error_source: "payment_gateway".to_string(),
        metadata: HashMap::new(),
    };

    // Demo 1: Timeout Error Classification
    println!("1. Timeout Error Classification");
    println!("   Original Error: Timeout during payment processing");

    let timeout_error = OrchestrationError::TimeoutError {
        operation: "payment_processing".to_string(),
        timeout_duration: Duration::from_secs(30),
    };

    let classification = classifier.classify_error(&timeout_error, &context);
    demonstrate_classification("Timeout", &classification);

    // Demo 2: Database Connection Error
    println!("\n2. Database Connection Error Classification");
    println!("   Original Error: Database connection timeout");

    let db_error = OrchestrationError::DatabaseError {
        operation: "update_payment_status".to_string(),
        reason: "connection timeout after 5000ms".to_string(),
    };

    let classification = classifier.classify_error(&db_error, &context);
    demonstrate_classification("Database Connection", &classification);

    // Demo 3: Configuration Error
    println!("\n3. Configuration Error Classification");
    println!("   Original Error: Invalid payment gateway config");

    let config_error = OrchestrationError::ConfigurationError {
        source: "payment_config.yaml".to_string(),
        reason: "missing required field 'api_key'".to_string(),
    };

    let classification = classifier.classify_error(&config_error, &context);
    demonstrate_classification("Configuration", &classification);

    // Demo 4: Framework Integration - Convert to StepExecutionError
    println!("\n4. Framework Integration Example");
    println!("   Converting classification to StepExecutionError for compatibility:");

    let step_error: StepExecutionError = classification.into();
    println!("   StepExecutionError: {step_error}");

    // Demo 5: Final Attempt Handling
    println!("\n5. Final Attempt Handling");
    println!("   Testing behavior when max attempts reached:");

    let final_context = ErrorContext {
        step_uuid: Uuid::now_v7(),
        task_uuid: Uuid::now_v7(),
        attempt_number: 5, // Equal to max_attempts
        max_attempts: context.max_attempts,
        execution_duration: context.execution_duration,
        step_name: context.step_name.clone(),
        error_source: context.error_source.clone(),
        metadata: HashMap::new(),
    };

    let transient_error = OrchestrationError::DatabaseError {
        operation: "insert".to_string(),
        reason: "temporary lock timeout".to_string(),
    };

    let final_classification = classifier.classify_error(&transient_error, &final_context);
    demonstrate_classification("Final Attempt", &final_classification);

    println!("\n=== Integration Benefits ===");
    println!("✅ Centralized error classification logic");
    println!("✅ Consistent retry strategies across components");
    println!("✅ Actionable error messages and remediation suggestions");
    println!("✅ Context-aware error handling decisions");
    println!("✅ Framework-agnostic error classification");
    println!("✅ Comprehensive error categorization");
}

fn demonstrate_classification(error_type: &str, classification: &ErrorClassification) {
    println!("   ┌─ {error_type} Classification Results:");
    println!("   │ Category: {:?}", classification.error_category);
    println!("   │ Retryable: {}", classification.is_retryable);
    if let Some(delay) = classification.retry_delay {
        println!("   │ Retry Delay: {}s", delay.as_secs());
    } else {
        println!("   │ Retry Delay: None");
    }
    println!("   │ Error Code: {}", classification.error_code);
    println!("   │ Final Attempt: {}", classification.is_final_attempt);
    println!("   │ Confidence: {:.1}%", classification.confidence * 100.0);
    if !classification.remediation_suggestions.is_empty() {
        println!("   │ Remediation Suggestions:");
        for suggestion in &classification.remediation_suggestions {
            println!("   │   • {suggestion}");
        }
    }
    println!("   └─ Message: {}", classification.error_message);
}
