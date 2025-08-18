/*!
 * Serde utilities for common serialization/deserialization patterns.
 *
 * This module provides reusable serde helper functions that are used
 * across multiple structs and modules to handle common deserialization
 * scenarios consistently.
 */

use serde::{Deserialize, Deserializer};

/// Deserialize an optional numeric value that may be represented as a number or string.
///
/// This function handles flexible YAML/JSON input where numeric values might be:
/// - Missing/null (returns None)
/// - Integer numbers
/// - Floating-point numbers (truncated to i32)
/// - String representations of numbers
///
/// This is particularly useful when dealing with configuration files where
/// users might quote numeric values or when values come from different sources
/// with varying type representations.
///
/// # Examples
///
/// ```yaml
/// # All of these would be successfully parsed:
/// timeout_seconds: 30        # Direct integer
/// timeout_seconds: "30"      # String that parses to integer
/// timeout_seconds: 30.5      # Float (truncated to 30)
/// timeout_seconds: null      # Returns None
/// # timeout_seconds omitted  # Returns None
/// ```
///
/// # Usage with serde
///
/// ```rust
/// use serde::Deserialize;
/// use tasker_core::utils::serde::deserialize_optional_numeric;
///
/// #[derive(Deserialize)]
/// struct Config {
///     #[serde(default, deserialize_with = "deserialize_optional_numeric")]
///     timeout_seconds: Option<i32>,
/// }
/// ```
pub fn deserialize_optional_numeric<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<i32>, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    let value: Option<serde_yaml::Value> = Option::deserialize(deserializer)?;
    match value {
        None => Ok(None),
        Some(serde_yaml::Value::Number(n)) => {
            if let Some(i) = n.as_i64() {
                Ok(Some(i as i32))
            } else if let Some(f) = n.as_f64() {
                // Truncate floating point to integer
                Ok(Some(f as i32))
            } else {
                Err(D::Error::custom(format!("Invalid numeric value: {n}")))
            }
        }
        Some(serde_yaml::Value::String(s)) => {
            // Try to parse string as number
            s.parse::<i32>()
                .map(Some)
                .or_else(|_| s.parse::<f64>().map(|f| Some(f as i32)))
                .map_err(|_| D::Error::custom(format!("Cannot parse '{s}' as numeric")))
        }
        Some(other) => Err(D::Error::custom(format!(
            "Expected numeric value, found: {other:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_yaml;

    #[derive(Deserialize, Debug, PartialEq)]
    struct TestStruct {
        #[serde(default, deserialize_with = "deserialize_optional_numeric")]
        value: Option<i32>,
    }

    #[test]
    fn test_deserialize_optional_numeric_integer() {
        let yaml = "value: 42";
        let result: TestStruct = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(result.value, Some(42));
    }

    #[test]
    fn test_deserialize_optional_numeric_string() {
        let yaml = r#"value: "123""#;
        let result: TestStruct = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(result.value, Some(123));
    }

    #[test]
    fn test_deserialize_optional_numeric_float() {
        let yaml = "value: 42.7";
        let result: TestStruct = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(result.value, Some(42)); // Truncated
    }

    #[test]
    fn test_deserialize_optional_numeric_null() {
        let yaml = "value: null";
        let result: TestStruct = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(result.value, None);
    }

    #[test]
    fn test_deserialize_optional_numeric_missing() {
        let yaml = "other_field: something";
        let result: TestStruct = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(result.value, None);
    }

    #[test]
    fn test_deserialize_optional_numeric_invalid_string() {
        let yaml = r#"value: "not-a-number""#;
        let result: Result<TestStruct, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }
}
