//! # Context Wrappers for Rails Migration
//!
//! Provides Ruby-friendly wrappers around the existing Rust client context types,
//! enabling Rails handlers to gradually migrate to the Rust handler foundation.

use magnus::r_hash::ForEach;
use magnus::value::ReprValue;
use magnus::{Error, IntoValue, RHash, RString, TryConvert, Value};
use std::collections::HashMap;
use tracing::{warn};

/// Ruby wrapper for the Rust StepContext
///
/// This enables Rails step handlers to gradually migrate to inheriting from
/// BaseStepHandler while maintaining compatibility with existing patterns.
#[derive(Debug, Clone)]
pub struct StepContext {
    step_id: i64,
    task_id: i64,
    step_name: String,
    input_data: serde_json::Value,
    previous_results: HashMap<String, serde_json::Value>,
    attempt_number: u32,
    is_retry: bool,
    can_retry: bool,
}

impl StepContext {
    pub fn new(
        step_id: i64,
        task_id: i64,
        step_name: String,
        input_data: serde_json::Value,
    ) -> Self {
        Self {
            step_id,
            task_id,
            step_name,
            input_data,
            previous_results: HashMap::new(),
            attempt_number: 0,
            is_retry: false,
            can_retry: false,
        }
    }

    // Ruby-friendly accessor methods
    pub fn step_id(&self) -> i64 {
        self.step_id
    }

    pub fn task_id(&self) -> i64 {
        self.task_id
    }

    pub fn step_name(&self) -> String {
        self.step_name.clone()
    }

    pub fn input_data(&self) -> Result<Value, Error> {
        json_to_ruby_value(self.input_data.clone())
    }

    pub fn previous_results(&self) -> Result<RHash, Error> {
        let hash = RHash::new();
        for (key, value) in &self.previous_results {
            let ruby_value = json_to_ruby_value(value.clone())?;
            hash.aset(key.clone(), ruby_value)?;
        }
        Ok(hash)
    }

    pub fn attempt_number(&self) -> u32 {
        self.attempt_number
    }

    pub fn is_retry(&self) -> bool {
        self.is_retry
    }

    pub fn can_retry(&self) -> bool {
        self.can_retry
    }

    pub fn get_previous_result(&self, step_name: RString) -> Result<Option<Value>, Error> {
        let step_name_str = unsafe { step_name.as_str() }?;
        match self.previous_results.get(step_name_str) {
            Some(value) => Ok(Some(json_to_ruby_value(value.clone())?)),
            None => Ok(None),
        }
    }

    pub fn get_config_value(&self, key: RString) -> Result<Option<Value>, Error> {
        let key_str = unsafe { key.as_str() }?;
        match self.previous_results.get(key_str) {
            Some(value) => Ok(Some(json_to_ruby_value(value.clone())?)),
            None => Ok(None),
        }
    }
}

/// Ruby wrapper for the Rust TaskContext
///
/// This enables Rails task handlers to gradually migrate to inheriting from
/// BaseTaskHandler while maintaining compatibility with existing patterns.
#[derive(Debug, Clone)]
pub struct TaskContext {
    task_id: i64,
    task_name: String,
    namespace: String,
    input_data: serde_json::Value,
    status: String,
    is_retry: bool,
    can_retry: bool,
}

impl TaskContext {
    pub fn new(
        task_id: i64,
        task_name: String,
        namespace: String,
        input_data: serde_json::Value,
        status: String,
    ) -> Self {
        Self {
            task_id,
            task_name,
            namespace,
            input_data,
            status,
            is_retry: false,
            can_retry: false,
        }
    }

    // Ruby-friendly accessor methods
    pub fn task_id(&self) -> i64 {
        self.task_id
    }

    pub fn task_name(&self) -> String {
        self.task_name.clone()
    }

    pub fn namespace(&self) -> String {
        self.namespace.clone()
    }

    pub fn input_data(&self) -> Result<Value, Error> {
        json_to_ruby_value(self.input_data.clone())
    }

    pub fn status(&self) -> String {
        self.status.clone()
    }

    pub fn is_retry(&self) -> bool {
        self.is_retry
    }

    pub fn can_retry(&self) -> bool {
        self.can_retry
    }
}

/// Input validation for JSON inputs
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    pub max_string_length: usize,
    pub max_array_length: usize,
    pub max_object_depth: usize,
    pub max_object_keys: usize,
    pub max_numeric_value: f64,
    pub min_numeric_value: f64,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            max_string_length: 10_000,
            max_array_length: 1_000,
            max_object_depth: 10,
            max_object_keys: 100,
            max_numeric_value: 1e15,
            min_numeric_value: -1e15,
        }
    }
}

/// Validate and sanitize JSON inputs
fn validate_json_value(
    value: &serde_json::Value,
    config: &ValidationConfig,
    current_depth: usize,
) -> Result<(), String> {
    if current_depth > config.max_object_depth {
        return Err(format!(
            "JSON depth exceeds maximum of {}",
            config.max_object_depth
        ));
    }

    match value {
        serde_json::Value::String(s) => {
            if s.len() > config.max_string_length {
                return Err(format!(
                    "String length {} exceeds maximum of {}",
                    s.len(),
                    config.max_string_length
                ));
            }
            // Check for potentially malicious content
            if s.contains("\x00") || s.contains("\r\n\r\n") {
                return Err("String contains potentially malicious content".to_string());
            }
        }
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                if f > config.max_numeric_value || f < config.min_numeric_value {
                    return Err(format!(
                        "Numeric value {} outside allowed range [{}, {}]",
                        f, config.min_numeric_value, config.max_numeric_value
                    ));
                }
                if f.is_infinite() || f.is_nan() {
                    return Err("Numeric value is infinite or NaN".to_string());
                }
            }
        }
        serde_json::Value::Array(arr) => {
            if arr.len() > config.max_array_length {
                return Err(format!(
                    "Array length {} exceeds maximum of {}",
                    arr.len(),
                    config.max_array_length
                ));
            }
            for item in arr {
                validate_json_value(item, config, current_depth + 1)?;
            }
        }
        serde_json::Value::Object(obj) => {
            if obj.len() > config.max_object_keys {
                return Err(format!(
                    "Object key count {} exceeds maximum of {}",
                    obj.len(),
                    config.max_object_keys
                ));
            }
            for (key, val) in obj {
                if key.len() > config.max_string_length {
                    return Err(format!(
                        "Object key length {} exceeds maximum of {}",
                        key.len(),
                        config.max_string_length
                    ));
                }
                validate_json_value(val, config, current_depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Convert Ruby Value to serde_json::Value with validation
pub fn ruby_value_to_json(ruby_value: Value) -> Result<serde_json::Value, Error> {
    ruby_value_to_json_with_validation(ruby_value, &ValidationConfig::default())
}

/// Convert Ruby Value to serde_json::Value with custom validation
pub fn ruby_value_to_json_with_validation(
    ruby_value: Value,
    config: &ValidationConfig,
) -> Result<serde_json::Value, Error> {
    let json_value = if ruby_value.is_nil() {
        serde_json::Value::Null
    } else if let Some(string) = RString::from_value(ruby_value) {
        let s = unsafe { string.as_str() }?;
        // Validate string length immediately
        if s.len() > config.max_string_length {
            return Err(Error::new(
                magnus::exception::arg_error(),
                format!(
                    "String length {} exceeds maximum of {}",
                    s.len(),
                    config.max_string_length
                ),
            ));
        }
        serde_json::Value::String(s.to_string())
    } else if let Some(hash) = RHash::from_value(ruby_value) {
        // Validate hash size
        if hash.len() > config.max_object_keys {
            return Err(Error::new(
                magnus::exception::arg_error(),
                format!(
                    "Hash key count {} exceeds maximum of {}",
                    hash.len(),
                    config.max_object_keys
                ),
            ));
        }

        let mut map = serde_json::Map::new();
        hash.foreach(|key: Value, value: Value| -> Result<ForEach, Error> {
            // Handle both string keys and symbol keys
            let key_string = if let Some(key_str) = RString::from_value(key) {
                // String key
                let s = unsafe { key_str.as_str() }?.to_string();
                s
            } else {
                // Try to call to_s on any key (symbols, etc)
                match key.funcall::<&str, (), String>("to_s", ()) {
                    Ok(s) => {
                        s
                    }
                    Err(e) => {
                        // Skip unknown key types
                        return Ok(ForEach::Continue);
                    }
                }
            };

            // Validate key length
            if key_string.len() > config.max_string_length {
                return Err(Error::new(
                    magnus::exception::arg_error(),
                    format!(
                        "Hash key length {} exceeds maximum of {}",
                        key_string.len(),
                        config.max_string_length
                    ),
                ));
            }

            let json_value = ruby_value_to_json_with_validation(value, config)?;
            map.insert(key_string, json_value);
            Ok(ForEach::Continue)
        })?;
        serde_json::Value::Object(map)
    } else if let Some(array) = magnus::RArray::from_value(ruby_value) {
        // Validate array length
        if array.len() > config.max_array_length {
            return Err(Error::new(
                magnus::exception::arg_error(),
                format!(
                    "Array length {} exceeds maximum of {}",
                    array.len(),
                    config.max_array_length
                ),
            ));
        }

        let mut vec = Vec::new();
        for item in array.into_iter() {
            let json_item = ruby_value_to_json_with_validation(item, config)?;
            vec.push(json_item);
        }
        serde_json::Value::Array(vec)
    } else if let Ok(int) = i64::try_convert(ruby_value) {
        // Validate integer range (cast to f64 for range check)
        let int_as_float = int as f64;
        if int_as_float > config.max_numeric_value || int_as_float < config.min_numeric_value {
            return Err(Error::new(
                magnus::exception::arg_error(),
                format!(
                    "Integer value {} outside allowed range [{}, {}]",
                    int, config.min_numeric_value, config.max_numeric_value
                ),
            ));
        }

        serde_json::Value::Number(serde_json::Number::from(int))
    } else if let Ok(float) = f64::try_convert(ruby_value) {
        // Validate numeric range
        if float > config.max_numeric_value || float < config.min_numeric_value {
            return Err(Error::new(
                magnus::exception::arg_error(),
                format!(
                    "Numeric value {} outside allowed range [{}, {}]",
                    float, config.min_numeric_value, config.max_numeric_value
                ),
            ));
        }
        if float.is_infinite() || float.is_nan() {
            return Err(Error::new(
                magnus::exception::arg_error(),
                "Numeric value is infinite or NaN".to_string(),
            ));
        }

        // Try float conversion to preserve precision for decimals
        if let Some(num) = serde_json::Number::from_f64(float) {
            serde_json::Value::Number(num)
        } else {
            serde_json::Value::Null
        }
    } else if let Ok(bool_val) = bool::try_convert(ruby_value) {
        serde_json::Value::Bool(bool_val)
    } else {
        // Default to string representation for unsupported types, but validate length
        let string_repr = format!("{ruby_value:?}");
        if string_repr.len() > config.max_string_length {
            warn!("Unsupported Ruby value representation exceeds length limit, truncating");
            let truncated = string_repr
                .chars()
                .take(config.max_string_length)
                .collect::<String>();
            serde_json::Value::String(format!("{truncated}...[truncated]"))
        } else {
            serde_json::Value::String(string_repr)
        }
    };

    // Final validation of the complete JSON structure
    if let Err(validation_error) = validate_json_value(&json_value, config, 0) {
        return Err(Error::new(
            magnus::exception::arg_error(),
            format!("JSON validation failed: {validation_error}"),
        ));
    }

    Ok(json_value)
}

/// Convert serde_json::Value to Ruby Value
pub fn json_to_ruby_value(json_value: serde_json::Value) -> Result<Value, Error> {
    match json_value {
        serde_json::Value::Null => Ok(().into_value()), // () converts to nil in Ruby
        serde_json::Value::Bool(b) => Ok(b.into_value()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.into_value())
            } else if let Some(f) = n.as_f64() {
                Ok(f.into_value())
            } else {
                Ok(().into_value()) // () converts to nil in Ruby
            }
        }
        serde_json::Value::String(s) => Ok(RString::new(&s).into_value()),
        serde_json::Value::Array(arr) => {
            let ruby_array = magnus::RArray::new();
            for item in arr {
                let ruby_item = json_to_ruby_value(item)?;
                ruby_array.push(ruby_item)?;
            }
            Ok(ruby_array.into_value())
        }
        serde_json::Value::Object(obj) => {
            let ruby_hash = RHash::new();
            for (key, value) in obj {
                let ruby_value = json_to_ruby_value(value)?;
                ruby_hash.aset(key, ruby_value)?;
            }
            Ok(ruby_hash.into_value())
        }
    }
}
