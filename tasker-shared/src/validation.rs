//! Input validation for Tasker Core
//!
//! Provides secure validation functions for user inputs, with special focus
//! on JSONB field validation to prevent injection attacks and malformed data.

use crate::errors::{TaskerError, TaskerResult};
use serde_json::{Map, Value};
use validator::ValidationError;

// =============================================================================
// PGMQ Queue Name Constraints
// =============================================================================
// PGMQ has a 47 character limit on queue names.
// Queue names are constructed as: {namespace}_{queue_type}
// The longest suffix is "_domain_events_dlq" (18 characters)
// Therefore: MAX_NAMESPACE_LENGTH = 47 - 18 = 29

/// Maximum length for PGMQ queue names (PostgreSQL identifier limit)
pub const MAX_PGMQ_QUEUE_NAME_LENGTH: usize = 47;

/// Longest queue suffix: "_domain_events_dlq"
pub const MAX_QUEUE_SUFFIX_LENGTH: usize = 18;

/// Maximum namespace length to ensure queue names fit within PGMQ limits
/// Calculated as: MAX_PGMQ_QUEUE_NAME_LENGTH - MAX_QUEUE_SUFFIX_LENGTH = 29
pub const MAX_NAMESPACE_LENGTH: usize = MAX_PGMQ_QUEUE_NAME_LENGTH - MAX_QUEUE_SUFFIX_LENGTH;

/// Validates namespace length for PGMQ queue name compatibility
///
/// PGMQ has a 47 character limit on queue names. Since queue names are
/// constructed as `{namespace}_{queue_type}` and the longest suffix is
/// `_domain_events_dlq` (18 characters), namespaces must be <= 29 characters.
pub fn validate_namespace_name(namespace: &str) -> TaskerResult<()> {
    if namespace.is_empty() {
        return Err(TaskerError::InvalidInput(
            "Namespace name cannot be empty".to_string(),
        ));
    }

    if namespace.len() > MAX_NAMESPACE_LENGTH {
        return Err(TaskerError::InvalidInput(format!(
            "Namespace name '{}' is too long: {} chars (max: {}). \
             PGMQ queue names are limited to {} chars, and the longest \
             queue suffix is {} chars.",
            namespace,
            namespace.len(),
            MAX_NAMESPACE_LENGTH,
            MAX_PGMQ_QUEUE_NAME_LENGTH,
            MAX_QUEUE_SUFFIX_LENGTH
        )));
    }

    // Validate characters (alphanumeric and underscore only for PostgreSQL identifiers)
    if !namespace
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(TaskerError::InvalidInput(format!(
            "Namespace name '{}' contains invalid characters. \
             Only alphanumeric characters and underscores are allowed.",
            namespace
        )));
    }

    // Must start with a letter or underscore (PostgreSQL identifier rules)
    if let Some(first_char) = namespace.chars().next() {
        if !first_char.is_ascii_alphabetic() && first_char != '_' {
            return Err(TaskerError::InvalidInput(format!(
                "Namespace name '{}' must start with a letter or underscore.",
                namespace
            )));
        }
    }

    Ok(())
}

/// Validator-compatible wrapper for namespace validation.
///
/// Use with the `validator` crate's custom validation:
/// ```ignore
/// #[validate(custom(function = "crate::validation::validate_namespace_for_validator"))]
/// pub namespace_name: String,
/// ```
pub fn validate_namespace_for_validator(namespace: &str) -> Result<(), ValidationError> {
    validate_namespace_name(namespace).map_err(|e| {
        let mut error = ValidationError::new("namespace_length");
        error.message = Some(e.to_string().into());
        error
    })
}

/// Maximum allowed size for JSONB payloads (1MB)
const MAX_JSON_SIZE_BYTES: usize = 1024 * 1024;

/// Maximum nesting depth for JSON objects/arrays
const MAX_JSON_DEPTH: usize = 10;

/// Maximum number of keys in a JSON object
const MAX_JSON_KEYS: usize = 1000;

/// Maximum string length for JSON string values
const MAX_JSON_STRING_LENGTH: usize = 10000;

/// Validates JSONB input for security and size constraints
pub fn validate_jsonb_input(value: &Value) -> TaskerResult<()> {
    // Check serialized size
    let serialized = serde_json::to_string(value)
        .map_err(|e| TaskerError::InvalidInput(format!("Invalid JSON structure: {e}")))?;

    if serialized.len() > MAX_JSON_SIZE_BYTES {
        return Err(TaskerError::InvalidInput(format!(
            "JSON payload too large: {} bytes (max: {})",
            serialized.len(),
            MAX_JSON_SIZE_BYTES
        )));
    }

    // Check nesting depth and structure
    validate_json_depth(value, 0)?;

    Ok(())
}

/// Validates JSON depth recursively
fn validate_json_depth(value: &Value, current_depth: usize) -> TaskerResult<()> {
    if current_depth > MAX_JSON_DEPTH {
        return Err(TaskerError::InvalidInput(format!(
            "JSON nesting too deep: {current_depth} (max: {MAX_JSON_DEPTH})"
        )));
    }

    match value {
        Value::Object(map) => {
            if map.len() > MAX_JSON_KEYS {
                return Err(TaskerError::InvalidInput(format!(
                    "Too many JSON keys: {} (max: {})",
                    map.len(),
                    MAX_JSON_KEYS
                )));
            }

            for (key, val) in map {
                // Validate key length
                if key.len() > MAX_JSON_STRING_LENGTH {
                    return Err(TaskerError::InvalidInput(format!(
                        "JSON key too long: {} chars (max: {})",
                        key.len(),
                        MAX_JSON_STRING_LENGTH
                    )));
                }

                // Recursively validate value
                validate_json_depth(val, current_depth + 1)?;
            }
        }
        Value::Array(arr) => {
            if arr.len() > MAX_JSON_KEYS {
                return Err(TaskerError::InvalidInput(format!(
                    "JSON array too large: {} items (max: {})",
                    arr.len(),
                    MAX_JSON_KEYS
                )));
            }

            for item in arr {
                validate_json_depth(item, current_depth + 1)?;
            }
        }
        Value::String(s) => {
            if s.len() > MAX_JSON_STRING_LENGTH {
                return Err(TaskerError::InvalidInput(format!(
                    "JSON string too long: {} chars (max: {})",
                    s.len(),
                    MAX_JSON_STRING_LENGTH
                )));
            }
        }
        _ => {} // Numbers, booleans, null are always safe
    }

    Ok(())
}

/// Validates task context JSON
pub fn validate_task_context(context: &Value) -> TaskerResult<()> {
    validate_jsonb_input(context)?;

    // Additional context-specific validation
    if let Value::Object(_) = context {
        // Context should be an object, which is good
        Ok(())
    } else {
        Err(TaskerError::InvalidInput(
            "Task context must be a JSON object".to_string(),
        ))
    }
}

/// Validates task tags JSON
pub fn validate_task_tags(tags: &Value) -> TaskerResult<()> {
    validate_jsonb_input(tags)?;

    // Tags should be an array or object
    match tags {
        Value::Array(_) | Value::Object(_) => Ok(()),
        _ => Err(TaskerError::InvalidInput(
            "Task tags must be a JSON array or object".to_string(),
        )),
    }
}

/// Validates workflow step inputs JSON
pub fn validate_step_inputs(inputs: &Value) -> TaskerResult<()> {
    validate_jsonb_input(inputs)?;

    // Inputs should be an object
    if let Value::Object(_) = inputs {
        Ok(())
    } else {
        Err(TaskerError::InvalidInput(
            "Step inputs must be a JSON object".to_string(),
        ))
    }
}

/// Validates workflow step results JSON
pub fn validate_step_results(results: &Value) -> TaskerResult<()> {
    validate_jsonb_input(results)?;

    // Results should be an object
    if let Value::Object(_) = results {
        Ok(())
    } else {
        Err(TaskerError::InvalidInput(
            "Step results must be a JSON object".to_string(),
        ))
    }
}

/// Validates transition metadata JSON
pub fn validate_transition_metadata(metadata: &Value) -> TaskerResult<()> {
    validate_jsonb_input(metadata)?;

    // Metadata should be an object
    if let Value::Object(_) = metadata {
        Ok(())
    } else {
        Err(TaskerError::InvalidInput(
            "Transition metadata must be a JSON object".to_string(),
        ))
    }
}

/// Validates configuration JSON (for NamedTask)
pub fn validate_configuration(config: &Value) -> TaskerResult<()> {
    validate_jsonb_input(config)?;

    // Configuration should be an object
    if let Value::Object(_) = config {
        Ok(())
    } else {
        Err(TaskerError::InvalidInput(
            "Configuration must be a JSON object".to_string(),
        ))
    }
}

/// Sanitizes a JSON value by removing potentially dangerous content
pub fn sanitize_json(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut sanitized_map = Map::new();
            for (key, val) in map {
                // Skip keys that look like dangerous script injection attempts
                let lower_key = key.to_lowercase();
                if lower_key == "script"
                    || lower_key == "eval"
                    || lower_key == "function"
                    || lower_key.starts_with("on")
                // onclick, onload, etc.
                {
                    continue;
                }

                sanitized_map.insert(key, sanitize_json(val));
            }
            Value::Object(sanitized_map)
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(sanitize_json).collect()),
        Value::String(s) => {
            // Remove potential script content from strings
            let sanitized = s
                .replace("<script>", "&lt;script&gt;")
                .replace("</script>", "&lt;/script&gt;")
                .replace("javascript:", "")
                .replace("data:text/html", "data:text/plain");
            Value::String(sanitized)
        }
        other => other, // Numbers, booleans, null are safe as-is
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_valid_json() {
        let valid_json = json!({
            "key": "value",
            "number": 42,
            "array": [1, 2, 3],
            "nested": {
                "inner": "value"
            }
        });

        assert!(validate_jsonb_input(&valid_json).is_ok());
    }

    #[test]
    fn test_json_too_deep() {
        let mut deep_json = json!({});
        let mut current = &mut deep_json;

        // Create nesting deeper than allowed
        for i in 0..15 {
            let key = format!("level_{i}");
            *current = json!({ key.clone(): {} });
            current = current.get_mut(&key).unwrap();
        }

        assert!(validate_jsonb_input(&deep_json).is_err());
    }

    #[test]
    fn test_string_too_long() {
        let long_string = "x".repeat(MAX_JSON_STRING_LENGTH + 1);
        let json_with_long_string = json!({
            "long_key": long_string
        });

        assert!(validate_jsonb_input(&json_with_long_string).is_err());
    }

    #[test]
    fn test_task_context_validation() {
        let valid_context = json!({
            "order_id": 123,
            "customer_email": "test@example.com"
        });

        assert!(validate_task_context(&valid_context).is_ok());

        let invalid_context = json!("not an object");
        assert!(validate_task_context(&invalid_context).is_err());
    }

    #[test]
    fn test_sanitization() {
        let dangerous_json = json!({
            "script": "<script>alert('xss')</script>",
            "onclick": "alert('xss')",
            "javascript_url": "javascript:alert('xss')",
            "html_content": "<script>alert('evil')</script>",
            "safe_key": "safe_value"
        });

        let sanitized = sanitize_json(dangerous_json);

        // Dangerous keys should be removed
        let obj = sanitized.as_object().unwrap();
        assert!(!obj.contains_key("script"));
        assert!(!obj.contains_key("onclick"));

        // Safe keys should remain, with dangerous content sanitized
        assert!(obj.contains_key("javascript_url"));
        assert!(obj.contains_key("html_content"));
        assert!(obj.contains_key("safe_key"));

        // Content should be sanitized
        assert_eq!(sanitized["javascript_url"], "alert('xss')"); // javascript: removed
        assert_eq!(
            sanitized["html_content"],
            "&lt;script&gt;alert('evil')&lt;/script&gt;"
        ); // <script sanitized
        assert_eq!(sanitized["safe_key"], "safe_value"); // unchanged
    }

    // =============================================================================
    // Namespace Validation Tests
    // =============================================================================

    #[test]
    fn test_valid_namespace() {
        // Valid namespaces within the 29 character limit
        assert!(validate_namespace_name("test").is_ok());
        assert!(validate_namespace_name("my_namespace").is_ok());
        assert!(validate_namespace_name("ts_e2e_checkpoint").is_ok());
        assert!(validate_namespace_name("_private").is_ok());
        assert!(validate_namespace_name("a").is_ok());

        // Exactly 29 characters (the limit)
        let max_length_namespace = "a".repeat(MAX_NAMESPACE_LENGTH);
        assert!(validate_namespace_name(&max_length_namespace).is_ok());
    }

    #[test]
    fn test_namespace_too_long() {
        // 30 characters - one over the limit
        let too_long = "a".repeat(MAX_NAMESPACE_LENGTH + 1);
        let result = validate_namespace_name(&too_long);
        assert!(result.is_err());

        // The original problematic namespace from the bug
        let problematic = "typescript_e2e_checkpoint_yield"; // 31 chars
        assert!(validate_namespace_name(problematic).is_err());
    }

    #[test]
    fn test_namespace_empty() {
        assert!(validate_namespace_name("").is_err());
    }

    #[test]
    fn test_namespace_invalid_characters() {
        // Hyphens not allowed (PostgreSQL identifier restriction)
        assert!(validate_namespace_name("my-namespace").is_err());

        // Spaces not allowed
        assert!(validate_namespace_name("my namespace").is_err());

        // Special characters not allowed
        assert!(validate_namespace_name("my@namespace").is_err());
        assert!(validate_namespace_name("my.namespace").is_err());
    }

    #[test]
    fn test_namespace_must_start_with_letter_or_underscore() {
        // Cannot start with a number
        assert!(validate_namespace_name("123namespace").is_err());
        assert!(validate_namespace_name("1_test").is_err());

        // Can start with letter
        assert!(validate_namespace_name("namespace123").is_ok());

        // Can start with underscore
        assert!(validate_namespace_name("_namespace").is_ok());
    }

    #[test]
    fn test_pgmq_queue_name_calculation() {
        // Verify our constants are correct
        assert_eq!(MAX_PGMQ_QUEUE_NAME_LENGTH, 47);
        assert_eq!(MAX_QUEUE_SUFFIX_LENGTH, 18); // "_domain_events_dlq"
        assert_eq!(MAX_NAMESPACE_LENGTH, 29); // 47 - 18

        // Verify that a max-length namespace + max suffix fits
        let max_namespace = "a".repeat(MAX_NAMESPACE_LENGTH);
        let max_suffix = "_domain_events_dlq";
        let full_queue_name = format!("{}{}", max_namespace, max_suffix);
        assert_eq!(full_queue_name.len(), MAX_PGMQ_QUEUE_NAME_LENGTH);
    }
}
