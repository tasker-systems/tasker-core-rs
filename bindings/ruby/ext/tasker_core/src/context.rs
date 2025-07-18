//! # Context Wrappers for Rails Migration
//!
//! Provides Ruby-friendly wrappers around the existing Rust client context types,
//! enabling Rails handlers to gradually migrate to the Rust handler foundation.

use magnus::r_hash::ForEach;
use magnus::value::ReprValue;
use magnus::{Error, IntoValue, RHash, RString, TryConvert, Value};
use tasker_core::client::context::{
    StepContext as RustStepContext, TaskContext as RustTaskContext,
};

/// Ruby wrapper for the Rust StepContext
///
/// This enables Rails step handlers to gradually migrate to inheriting from
/// BaseStepHandler while maintaining compatibility with existing patterns.
#[derive(Debug, Clone)]
pub struct StepContext {
    inner: RustStepContext,
}

impl StepContext {
    /// Create a new StepContext from Ruby arguments
    pub fn new(
        step_id: i64,
        task_id: i64,
        step_name: RString,
        input_data: Value,
    ) -> Result<Self, Error> {
        let step_name_str = unsafe { step_name.as_str() }?;
        let input_json = ruby_value_to_json(input_data)?;

        let inner = RustStepContext::new(step_id, task_id, step_name_str.to_string(), input_json);

        Ok(Self { inner })
    }

    /// Create from an existing Rust StepContext (for orchestration integration)
    pub fn from_rust(rust_context: RustStepContext) -> Self {
        Self {
            inner: rust_context,
        }
    }

    /// Get the inner Rust StepContext (for performance operations)
    pub fn into_rust(self) -> RustStepContext {
        self.inner
    }

    /// Get a reference to the inner Rust StepContext
    pub fn as_rust(&self) -> &RustStepContext {
        &self.inner
    }

    // Ruby-friendly accessor methods
    pub fn step_id(&self) -> i64 {
        self.inner.step_id
    }

    pub fn task_id(&self) -> i64 {
        self.inner.task_id
    }

    pub fn step_name(&self) -> String {
        self.inner.step_name.clone()
    }

    pub fn input_data(&self) -> Result<Value, Error> {
        json_to_ruby_value(self.inner.input_data.clone())
    }

    pub fn previous_results(&self) -> Result<RHash, Error> {
        let hash = RHash::new();
        for (key, value) in &self.inner.previous_results {
            let ruby_value = json_to_ruby_value(value.clone())?;
            hash.aset(key.clone(), ruby_value)?;
        }
        Ok(hash)
    }

    pub fn attempt_number(&self) -> u32 {
        self.inner.attempt_number
    }

    pub fn is_retry(&self) -> bool {
        self.inner.is_retry()
    }

    pub fn can_retry(&self) -> bool {
        self.inner.can_retry()
    }

    pub fn get_previous_result(&self, step_name: RString) -> Result<Option<Value>, Error> {
        let step_name_str = unsafe { step_name.as_str() }?;
        match self.inner.get_previous_result(step_name_str) {
            Some(value) => Ok(Some(json_to_ruby_value(value.clone())?)),
            None => Ok(None),
        }
    }

    pub fn get_config_value(&self, key: RString) -> Result<Option<Value>, Error> {
        let key_str = unsafe { key.as_str() }?;
        match self.inner.get_config_value(key_str) {
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
    inner: RustTaskContext,
}

impl TaskContext {
    /// Create a new TaskContext from Ruby arguments
    pub fn new(
        task_id: i64,
        task_name: RString,
        namespace: RString,
        input_data: Value,
    ) -> Result<Self, Error> {
        let task_name_str = unsafe { task_name.as_str() }?;
        let namespace_str = unsafe { namespace.as_str() }?;
        let input_json = ruby_value_to_json(input_data)?;

        let inner = RustTaskContext::new(
            task_id,
            task_name_str.to_string(),
            namespace_str.to_string(),
            input_json,
        );

        Ok(Self { inner })
    }

    /// Create from an existing Rust TaskContext (for orchestration integration)
    pub fn from_rust(rust_context: RustTaskContext) -> Self {
        Self {
            inner: rust_context,
        }
    }

    /// Get the inner Rust TaskContext (for performance operations)
    pub fn into_rust(self) -> RustTaskContext {
        self.inner
    }

    /// Get a reference to the inner Rust TaskContext
    pub fn as_rust(&self) -> &RustTaskContext {
        &self.inner
    }

    // Ruby-friendly accessor methods
    pub fn task_id(&self) -> i64 {
        self.inner.task_id
    }

    pub fn task_name(&self) -> String {
        self.inner.task_name.clone()
    }

    pub fn namespace(&self) -> String {
        self.inner.namespace.clone()
    }

    pub fn input_data(&self) -> Result<Value, Error> {
        json_to_ruby_value(self.inner.input_data.clone())
    }

    pub fn status(&self) -> String {
        self.inner.status.clone()
    }

    pub fn is_retry(&self) -> bool {
        self.inner.is_retry()
    }

    pub fn can_retry(&self) -> bool {
        self.inner.can_retry()
    }

    pub fn get_config_value(&self, key: RString) -> Result<Option<Value>, Error> {
        let key_str = unsafe { key.as_str() }?;
        match self.inner.get_config_value(key_str) {
            Some(value) => Ok(Some(json_to_ruby_value(value.clone())?)),
            None => Ok(None),
        }
    }
}

/// Convert Ruby Value to serde_json::Value
pub fn ruby_value_to_json(ruby_value: Value) -> Result<serde_json::Value, Error> {

    if ruby_value.is_nil() {
        Ok(serde_json::Value::Null)
    } else if let Some(string) = RString::from_value(ruby_value) {
        let s = unsafe { string.as_str() }?;
        Ok(serde_json::Value::String(s.to_string()))
    } else if let Some(hash) = RHash::from_value(ruby_value) {
        let mut map = serde_json::Map::new();
        println!("🔍 CONTEXT: Processing hash with {} entries", hash.len());
        hash.foreach(|key: Value, value: Value| -> Result<ForEach, Error> {
            println!("🔍 CONTEXT: Processing key: {:?}, value: {:?}", key, value);
            // Handle both string keys and symbol keys
            let key_string = if let Some(key_str) = RString::from_value(key) {
                // String key
                let s = unsafe { key_str.as_str() }?.to_string();
                println!("🔍 CONTEXT: Found string key: {}", s);
                s
            } else {
                // Try to call to_s on any key (symbols, etc)
                match key.funcall::<&str, (), String>("to_s", ()) {
                    Ok(s) => {
                        println!("🔍 CONTEXT: Converted key to string: {}", s);
                        s
                    },
                    Err(e) => {
                        println!("🔍 CONTEXT: Failed to convert key to string: {:?}, skipping", e);
                        // Skip unknown key types
                        return Ok(ForEach::Continue);
                    }
                }
            };

            let json_value = ruby_value_to_json(value)?;
            println!("🔍 CONTEXT: Inserting key '{}' with value: {:?}", key_string, json_value);
            map.insert(key_string, json_value);
            Ok(ForEach::Continue)
        })?;
        println!("🔍 CONTEXT: Final map has {} entries", map.len());
        Ok(serde_json::Value::Object(map))
    } else if let Some(array) = magnus::RArray::from_value(ruby_value) {
        let mut vec = Vec::new();
        for item in array.into_iter() {
            let json_item = ruby_value_to_json(item)?;
            vec.push(json_item);
        }
        Ok(serde_json::Value::Array(vec))
    } else if let Ok(float) = f64::try_convert(ruby_value) {
        // Try float conversion first to preserve precision for decimals
        if let Some(num) = serde_json::Number::from_f64(float) {
            Ok(serde_json::Value::Number(num))
        } else {
            Ok(serde_json::Value::Null)
        }
    } else if let Ok(int) = i64::try_convert(ruby_value) {
        Ok(serde_json::Value::Number(serde_json::Number::from(int)))
    } else if let Ok(bool_val) = bool::try_convert(ruby_value) {
        Ok(serde_json::Value::Bool(bool_val))
    } else {
        // Default to string representation for unsupported types
        Ok(serde_json::Value::String(format!("{ruby_value:?}")))
    }
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
