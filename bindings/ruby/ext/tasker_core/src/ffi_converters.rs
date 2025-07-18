//! # FFI Converters for Magnus Optimization
//!
//! This module provides optimized FFI conversion utilities that eliminate JSON serialization
//! overhead by using Magnus wrapped classes with `free_immediately` and direct Ruby conversions.
//!
//! ## Performance Benefits
//! - Eliminates JSON serialization/deserialization overhead
//! - Uses Magnus wrapped classes for zero-copy conversions where possible
//! - Provides proper Ruby GC integration with `free_immediately`
//! - Reduces FFI call overhead from >1ms to <100μs target

use magnus::{Error, RArray, RHash, RString, Value, IntoValue, Module};

/// Trait for converting Ruby hashes to Rust structs without JSON serialization
pub trait FromRHash: Sized {
    fn from_rhash(hash: RHash) -> Result<Self, Error>;
}

/// Trait for converting Rust structs to Ruby objects without JSON serialization
pub trait ToRubyObject {
    fn to_ruby_object(&self) -> Result<Value, Error>;
}

/// Optimized TaskRequest input structure using Magnus wrapped classes
#[derive(Clone, Debug)]
#[magnus::wrap(class = "TaskerCore::CreateTaskInput", free_immediately)]
pub struct CreateTaskInput {
    pub name: String,
    pub namespace_id: i64,
    pub context: String,  // JSON string - more efficient than HashMap<String, Value> for Send
    pub initiator: Option<String>,
}

impl CreateTaskInput {
    /// Create from Ruby hash parameters
    pub fn from_params(name: String, namespace_id: i64, context: RHash, initiator: Option<String>) -> Result<Self, Error> {
        // Convert Ruby hash to JSON string using crate::context helper
        let context_json = crate::context::ruby_value_to_json(context.into_value())
            .map_err(|e| Error::new(magnus::exception::runtime_error(), format!("Context conversion failed: {}", e)))?;
        
        Ok(CreateTaskInput {
            name,
            namespace_id,
            context: context_json.to_string(),
            initiator,
        })
    }
}

/// Optimized TaskMetadata response structure using Magnus wrapped classes
#[derive(Clone, Debug)]
#[magnus::wrap(class = "TaskerCore::TaskMetadata", free_immediately)]
pub struct TaskMetadata {
    pub found: bool,
    pub namespace: String,
    pub name: String,
    pub version: String,
    pub ruby_class_name: Option<String>,
    pub config_schema: Option<String>,
    pub registered_at: Option<String>,
    pub handle_id: Option<String>,
}

impl TaskMetadata {
    /// Define the Ruby class in the module
    pub fn define(ruby: &magnus::Ruby, module: &magnus::RModule) -> Result<(), magnus::Error> {
        let class = module.define_class("TaskMetadata", ruby.class_object())?;
        
        // Define getter methods
        class.define_method("found", magnus::method!(TaskMetadata::found_getter, 0))?;
        class.define_method("namespace", magnus::method!(TaskMetadata::namespace_getter, 0))?;
        class.define_method("name", magnus::method!(TaskMetadata::name_getter, 0))?;
        class.define_method("version", magnus::method!(TaskMetadata::version_getter, 0))?;
        class.define_method("ruby_class_name", magnus::method!(TaskMetadata::ruby_class_name_getter, 0))?;
        class.define_method("config_schema", magnus::method!(TaskMetadata::config_schema_getter, 0))?;
        class.define_method("registered_at", magnus::method!(TaskMetadata::registered_at_getter, 0))?;
        class.define_method("handle_id", magnus::method!(TaskMetadata::handle_id_getter, 0))?;
        
        Ok(())
    }
    
    // Getter methods for Ruby
    pub fn found_getter(&self) -> bool { self.found }
    pub fn namespace_getter(&self) -> String { self.namespace.clone() }
    pub fn name_getter(&self) -> String { self.name.clone() }
    pub fn version_getter(&self) -> String { self.version.clone() }
    pub fn ruby_class_name_getter(&self) -> Option<String> { self.ruby_class_name.clone() }
    pub fn config_schema_getter(&self) -> Option<String> { self.config_schema.clone() }
    pub fn registered_at_getter(&self) -> Option<String> { self.registered_at.clone() }
    pub fn handle_id_getter(&self) -> Option<String> { self.handle_id.clone() }
}

impl TaskMetadata {
    /// Create successful metadata response
    pub fn found(
        namespace: String,
        name: String,
        version: String,
        ruby_class_name: String,
        config_schema: Option<String>,
        registered_at: String,
        handle_id: String,
    ) -> Self {
        TaskMetadata {
            found: true,
            namespace,
            name,
            version,
            ruby_class_name: Some(ruby_class_name),
            config_schema,
            registered_at: Some(registered_at),
            handle_id: Some(handle_id),
        }
    }
    
    /// Create not found metadata response
    pub fn not_found(namespace: String, name: String, version: String) -> Self {
        TaskMetadata {
            found: false,
            namespace,
            name,
            version,
            ruby_class_name: None,
            config_schema: None,
            registered_at: None,
            handle_id: None,
        }
    }
}

/// Optimized FactoryResult response structure using Magnus wrapped classes
#[derive(Clone, Debug)]
#[magnus::wrap(class = "TaskerCore::FactoryResult", free_immediately)]
pub struct FactoryResult {
    pub success: bool,
    pub task_id: Option<i64>,
    pub workflow_steps: Vec<i64>,
    pub message: String,
    pub error: Option<String>,
}

impl FactoryResult {
    /// Create successful factory result
    pub fn success(task_id: i64, workflow_steps: Vec<i64>, message: String) -> Self {
        FactoryResult {
            success: true,
            task_id: Some(task_id),
            workflow_steps,
            message,
            error: None,
        }
    }
    
    /// Create failed factory result
    pub fn error(message: String, error: String) -> Self {
        FactoryResult {
            success: false,
            task_id: None,
            workflow_steps: Vec::new(),
            message,
            error: Some(error),
        }
    }
}

/// Helper function to convert Vec<i64> to Ruby array
pub fn vec_i64_to_ruby_array(vec: Vec<i64>) -> Result<RArray, Error> {
    let array = RArray::new();
    for item in vec {
        array.push(item)?;
    }
    Ok(array)
}

/// Helper function to convert Option<String> to Ruby value (nil or string)
pub fn option_string_to_ruby_value(opt: Option<String>) -> Result<Value, Error> {
    match opt {
        Some(s) => Ok(RString::new(&s).into_value()),
        None => Ok(().into_value()), // nil
    }
}

/// Optimized WorkflowStepInput structure using Magnus wrapped classes
#[derive(Clone, Debug)]
#[magnus::wrap(class = "TaskerCore::WorkflowStepInput", free_immediately)]
pub struct WorkflowStepInput {
    pub task_id: i64,
    pub name: String,
    pub dependencies: Vec<String>,
    pub handler_class: Option<String>,
    pub config: Option<String>, // JSON string for configuration
}

impl WorkflowStepInput {
    /// Create from Ruby parameters
    pub fn from_params(
        task_id: i64,
        name: String,
        dependencies: Option<Vec<String>>,
        handler_class: Option<String>,
        config: Option<RHash>,
    ) -> Result<Self, Error> {
        let config_json = if let Some(cfg) = config {
            let json_val = crate::context::ruby_value_to_json(cfg.into_value())
                .map_err(|e| Error::new(magnus::exception::runtime_error(), format!("Config conversion failed: {}", e)))?;
            Some(json_val.to_string())
        } else {
            None
        };

        Ok(WorkflowStepInput {
            task_id,
            name,
            dependencies: dependencies.unwrap_or_default(),
            handler_class,
            config: config_json,
        })
    }
}

/// Optimized ComplexWorkflowInput structure using Magnus wrapped classes
#[derive(Clone, Debug)]
#[magnus::wrap(class = "TaskerCore::ComplexWorkflowInput", free_immediately)]
pub struct ComplexWorkflowInput {
    pub pattern: String,
    pub namespace: String,
    pub task_name: String,
    pub step_count: Option<i32>,
    pub parallel_branches: Option<i32>,
    pub dependency_depth: Option<i32>,
}

impl ComplexWorkflowInput {
    /// Create from Ruby parameters
    pub fn from_params(
        pattern: String,
        namespace: String,
        task_name: String,
        step_count: Option<i32>,
        parallel_branches: Option<i32>,
        dependency_depth: Option<i32>,
    ) -> Result<Self, Error> {
        Ok(ComplexWorkflowInput {
            pattern,
            namespace,
            task_name,
            step_count,
            parallel_branches,
            dependency_depth,
        })
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_create_task_input_creation() {
        let input = CreateTaskInput {
            name: "test_task".to_string(),
            namespace_id: 1,
            context: "{}".to_string(),  // JSON string instead of HashMap
            initiator: Some("test_user".to_string()),
        };
        
        assert_eq!(input.name, "test_task");
        assert_eq!(input.namespace_id, 1);
        assert_eq!(input.context, "{}");
        assert_eq!(input.initiator, Some("test_user".to_string()));
    }
    
    #[test]
    fn test_task_metadata_found() {
        let metadata = TaskMetadata::found(
            "test_namespace".to_string(),
            "test_task".to_string(),
            "v1".to_string(),
            "TestHandler".to_string(),
            Some("schema".to_string()),
            "2023-01-01T00:00:00Z".to_string(),
            "handle_123".to_string(),
        );
        
        assert!(metadata.found);
        assert_eq!(metadata.namespace, "test_namespace");
        assert_eq!(metadata.ruby_class_name, Some("TestHandler".to_string()));
    }
    
    #[test]
    fn test_task_metadata_not_found() {
        let metadata = TaskMetadata::not_found(
            "test_namespace".to_string(),
            "test_task".to_string(),
            "v1".to_string(),
        );
        
        assert!(!metadata.found);
        assert_eq!(metadata.namespace, "test_namespace");
        assert_eq!(metadata.ruby_class_name, None);
    }
    
    #[test]
    fn test_workflow_step_input_creation() {
        let input = WorkflowStepInput::from_params(
            123,
            "test_step".to_string(),
            Some(vec!["dep1".to_string(), "dep2".to_string()]),
            Some("TestHandler".to_string()),
            None,
        ).unwrap();
        
        assert_eq!(input.task_id, 123);
        assert_eq!(input.name, "test_step");
        assert_eq!(input.dependencies.len(), 2);
        assert_eq!(input.handler_class, Some("TestHandler".to_string()));
        assert_eq!(input.config, None);
    }
    
    #[test]
    fn test_complex_workflow_input_creation() {
        let input = ComplexWorkflowInput::from_params(
            "linear".to_string(),
            "test_namespace".to_string(),
            "test_workflow".to_string(),
            Some(5),
            Some(2),
            Some(3),
        ).unwrap();
        
        assert_eq!(input.pattern, "linear");
        assert_eq!(input.namespace, "test_namespace");
        assert_eq!(input.step_count, Some(5));
        assert_eq!(input.parallel_branches, Some(2));
        assert_eq!(input.dependency_depth, Some(3));
    }
}