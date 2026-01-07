//! # Method Dispatch Wrapper
//!
//! TAS-93: Framework-level method dispatch for resolved handlers.
//!
//! This module provides the method dispatch layer that sits between resolved
//! handlers and step execution. The wrapper ensures that the correct method
//! is invoked based on the handler definition's `method` field.
//!
//! ## Separation of Concerns
//!
//! ```text
//! HandlerDefinition
//! ├── callable: "MyHandler"     ─── Resolver interprets this
//! ├── method: "validate"        ─── Framework handles this (MethodDispatchWrapper)
//! └── resolver: "payments"      ─── Framework routes this (ResolverChain)
//! ```
//!
//! ## Why Framework Handles Method Dispatch
//!
//! Resolvers have a single responsibility: interpret `callable` and return a
//! handler instance. Method dispatch is a cross-cutting concern that applies
//! to ALL resolved handlers regardless of which resolver provided them.
//!
//! ## Usage Flow
//!
//! ```text
//! ResolverChain                 MethodDispatchWrapper           Execution
//!     │                              │                              │
//!     │  resolve(definition)         │                              │
//!     ├─────────────────────────────►│                              │
//!     │         handler              │                              │
//!     │◄─────────────────────────────┤                              │
//!     │                              │                              │
//!     │  wrap(handler, method)       │                              │
//!     ├─────────────────────────────►│                              │
//!     │    wrapped_handler           │                              │
//!     │◄─────────────────────────────┤                              │
//!     │                              │                              │
//!     │                              │  invoke(...)                 │
//!     │                              ├─────────────────────────────►│
//!     │                              │        result                │
//!     │                              │◄─────────────────────────────┤
//! ```

use super::step_handler_resolver::ResolvedHandler;
use std::fmt;
use std::sync::Arc;

/// Information about method dispatch for a wrapped handler.
#[derive(Debug, Clone)]
pub struct MethodDispatchInfo {
    /// The target method to invoke
    pub method: String,

    /// The underlying handler name
    pub handler_name: String,

    /// Whether this is using the default "call" method
    pub is_default_method: bool,
}

impl fmt::Display for MethodDispatchInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_default_method {
            write!(f, "{}#call (default)", self.handler_name)
        } else {
            write!(f, "{}#{}", self.handler_name, self.method)
        }
    }
}

/// A wrapper that associates a resolved handler with its target method.
///
/// The `MethodDispatchWrapper` is created by the framework after a handler
/// is resolved. It carries the method name that should be invoked, enabling
/// deferred method dispatch at execution time.
///
/// ## Design Philosophy
///
/// This wrapper doesn't perform the actual method invocation - that's the
/// responsibility of the language-specific worker (Rust, Ruby, Python).
/// Instead, it carries the dispatch information so workers can invoke
/// the correct method.
///
/// ## Thread Safety
///
/// `MethodDispatchWrapper` is `Send + Sync` since it holds only an `Arc`
/// reference to the handler and owned strings.
#[derive(Debug)]
pub struct MethodDispatchWrapper {
    /// The underlying resolved handler
    handler: Arc<dyn ResolvedHandler>,

    /// The method to invoke on the handler
    method: String,

    /// Whether this is the default "call" method
    is_default_method: bool,
}

impl MethodDispatchWrapper {
    /// Create a new method dispatch wrapper.
    ///
    /// # Arguments
    ///
    /// * `handler` - The resolved handler to wrap
    /// * `method` - The method name to invoke (if None, defaults to "call")
    #[must_use]
    pub fn new(handler: Arc<dyn ResolvedHandler>, method: Option<&str>) -> Self {
        let effective_method = method.unwrap_or("call");
        let is_default_method = effective_method == "call";

        Self {
            handler,
            method: effective_method.to_string(),
            is_default_method,
        }
    }

    /// Create a wrapper for the default "call" method.
    #[must_use]
    pub fn with_default_method(handler: Arc<dyn ResolvedHandler>) -> Self {
        Self::new(handler, None)
    }

    /// Create a wrapper for a specific method.
    #[must_use]
    pub fn with_method(handler: Arc<dyn ResolvedHandler>, method: &str) -> Self {
        Self::new(handler, Some(method))
    }

    /// Get a reference to the underlying handler.
    #[must_use]
    pub fn handler(&self) -> &Arc<dyn ResolvedHandler> {
        &self.handler
    }

    /// Get the target method name.
    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }

    /// Check if this is using the default "call" method.
    #[must_use]
    pub fn is_default_method(&self) -> bool {
        self.is_default_method
    }

    /// Get the handler name.
    #[must_use]
    pub fn handler_name(&self) -> &str {
        self.handler.name()
    }

    /// Check if the underlying handler supports the target method.
    #[must_use]
    pub fn is_method_supported(&self) -> bool {
        self.handler.supports_method(&self.method)
    }

    /// Get dispatch information for logging/debugging.
    #[must_use]
    pub fn dispatch_info(&self) -> MethodDispatchInfo {
        MethodDispatchInfo {
            method: self.method.clone(),
            handler_name: self.handler.name().to_string(),
            is_default_method: self.is_default_method,
        }
    }
}

impl fmt::Display for MethodDispatchWrapper {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.dispatch_info())
    }
}

/// Error returned when method dispatch fails.
#[derive(Debug, Clone)]
pub struct MethodDispatchError {
    /// The handler name
    pub handler_name: String,

    /// The method that failed
    pub method: String,

    /// Error message
    pub message: String,

    /// Supported methods (for hint)
    pub supported_methods: Vec<String>,
}

impl fmt::Display for MethodDispatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Method '{}' not supported by handler '{}': {}",
            self.method, self.handler_name, self.message
        )?;
        if !self.supported_methods.is_empty() {
            write!(f, " (supported: {})", self.supported_methods.join(", "))?;
        }
        Ok(())
    }
}

impl std::error::Error for MethodDispatchError {}

impl MethodDispatchError {
    /// Create an error for an unsupported method.
    #[must_use]
    pub fn unsupported_method(handler: &Arc<dyn ResolvedHandler>, method: &str) -> Self {
        Self {
            handler_name: handler.name().to_string(),
            method: method.to_string(),
            message: "Method not found".to_string(),
            supported_methods: handler
                .supported_methods()
                .into_iter()
                .map(String::from)
                .collect(),
        }
    }
}

/// Validate that a handler supports the requested method.
///
/// This is a convenience function for validation before wrapping.
///
/// # Arguments
///
/// * `handler` - The handler to validate
/// * `method` - The method to check (if None, uses default "call")
///
/// # Returns
///
/// `Ok(())` if the method is supported, or an error with details.
pub fn validate_method_support(
    handler: &Arc<dyn ResolvedHandler>,
    method: Option<&str>,
) -> Result<(), MethodDispatchError> {
    let effective_method = method.unwrap_or("call");

    if handler.supports_method(effective_method) {
        Ok(())
    } else {
        Err(MethodDispatchError::unsupported_method(
            handler,
            effective_method,
        ))
    }
}

/// Create a method dispatch wrapper, validating method support first.
///
/// This is the recommended way to create wrappers when validation is needed.
///
/// # Arguments
///
/// * `handler` - The handler to wrap
/// * `method` - The method to invoke (if None, uses default "call")
///
/// # Returns
///
/// A wrapper, or an error if the method is not supported.
pub fn wrap_with_method(
    handler: Arc<dyn ResolvedHandler>,
    method: Option<&str>,
) -> Result<MethodDispatchWrapper, MethodDispatchError> {
    validate_method_support(&handler, method)?;
    Ok(MethodDispatchWrapper::new(handler, method))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock handler for testing
    #[derive(Debug)]
    struct MockHandler {
        name: String,
        supported_methods: Vec<String>,
    }

    impl MockHandler {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
                supported_methods: vec!["call".to_string()],
            }
        }

        fn with_methods(name: &str, methods: Vec<&str>) -> Self {
            Self {
                name: name.to_string(),
                supported_methods: methods.into_iter().map(String::from).collect(),
            }
        }
    }

    impl ResolvedHandler for MockHandler {
        fn name(&self) -> &str {
            &self.name
        }

        fn supports_method(&self, method: &str) -> bool {
            self.supported_methods.iter().any(|m| m == method)
        }

        fn supported_methods(&self) -> Vec<&str> {
            self.supported_methods.iter().map(|s| s.as_str()).collect()
        }
    }

    #[test]
    fn test_wrapper_default_method() {
        let handler: Arc<dyn ResolvedHandler> = Arc::new(MockHandler::new("TestHandler"));
        let wrapper = MethodDispatchWrapper::with_default_method(handler);

        assert_eq!(wrapper.method(), "call");
        assert!(wrapper.is_default_method());
        assert_eq!(wrapper.handler_name(), "TestHandler");
    }

    #[test]
    fn test_wrapper_custom_method() {
        let handler: Arc<dyn ResolvedHandler> = Arc::new(MockHandler::with_methods(
            "TestHandler",
            vec!["call", "validate"],
        ));
        let wrapper = MethodDispatchWrapper::with_method(handler, "validate");

        assert_eq!(wrapper.method(), "validate");
        assert!(!wrapper.is_default_method());
        assert_eq!(wrapper.handler_name(), "TestHandler");
    }

    #[test]
    fn test_wrapper_none_defaults_to_call() {
        let handler: Arc<dyn ResolvedHandler> = Arc::new(MockHandler::new("TestHandler"));
        let wrapper = MethodDispatchWrapper::new(handler, None);

        assert_eq!(wrapper.method(), "call");
        assert!(wrapper.is_default_method());
    }

    #[test]
    fn test_wrapper_explicit_call_is_default() {
        let handler: Arc<dyn ResolvedHandler> = Arc::new(MockHandler::new("TestHandler"));
        let wrapper = MethodDispatchWrapper::new(handler, Some("call"));

        assert_eq!(wrapper.method(), "call");
        assert!(wrapper.is_default_method());
    }

    #[test]
    fn test_is_method_supported() {
        let handler: Arc<dyn ResolvedHandler> = Arc::new(MockHandler::with_methods(
            "TestHandler",
            vec!["call", "validate"],
        ));

        let wrapper = MethodDispatchWrapper::with_method(Arc::clone(&handler), "validate");
        assert!(wrapper.is_method_supported());

        let wrapper = MethodDispatchWrapper::with_method(handler, "unknown");
        assert!(!wrapper.is_method_supported());
    }

    #[test]
    fn test_dispatch_info() {
        let handler: Arc<dyn ResolvedHandler> = Arc::new(MockHandler::new("TestHandler"));

        let wrapper = MethodDispatchWrapper::with_default_method(Arc::clone(&handler));
        let info = wrapper.dispatch_info();
        assert_eq!(info.method, "call");
        assert_eq!(info.handler_name, "TestHandler");
        assert!(info.is_default_method);

        let wrapper = MethodDispatchWrapper::with_method(handler, "validate");
        let info = wrapper.dispatch_info();
        assert_eq!(info.method, "validate");
        assert!(!info.is_default_method);
    }

    #[test]
    fn test_display_formatting() {
        let handler: Arc<dyn ResolvedHandler> = Arc::new(MockHandler::new("TestHandler"));

        let wrapper = MethodDispatchWrapper::with_default_method(Arc::clone(&handler));
        assert_eq!(wrapper.to_string(), "TestHandler#call (default)");

        let wrapper = MethodDispatchWrapper::with_method(handler, "validate");
        assert_eq!(wrapper.to_string(), "TestHandler#validate");
    }

    #[test]
    fn test_validate_method_support_success() {
        let handler: Arc<dyn ResolvedHandler> = Arc::new(MockHandler::with_methods(
            "TestHandler",
            vec!["call", "validate"],
        ));

        assert!(validate_method_support(&handler, None).is_ok());
        assert!(validate_method_support(&handler, Some("call")).is_ok());
        assert!(validate_method_support(&handler, Some("validate")).is_ok());
    }

    #[test]
    fn test_validate_method_support_failure() {
        let handler: Arc<dyn ResolvedHandler> = Arc::new(MockHandler::new("TestHandler"));

        let result = validate_method_support(&handler, Some("unknown"));
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert_eq!(error.handler_name, "TestHandler");
        assert_eq!(error.method, "unknown");
        assert!(error.supported_methods.contains(&"call".to_string()));
    }

    #[test]
    fn test_wrap_with_method_success() {
        let handler: Arc<dyn ResolvedHandler> = Arc::new(MockHandler::with_methods(
            "TestHandler",
            vec!["call", "validate"],
        ));

        let result = wrap_with_method(Arc::clone(&handler), Some("validate"));
        assert!(result.is_ok());

        let wrapper = result.unwrap();
        assert_eq!(wrapper.method(), "validate");
    }

    #[test]
    fn test_wrap_with_method_failure() {
        let handler: Arc<dyn ResolvedHandler> = Arc::new(MockHandler::new("TestHandler"));

        let result = wrap_with_method(handler, Some("unknown"));
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert!(error.to_string().contains("unknown"));
        assert!(error.to_string().contains("TestHandler"));
    }

    #[test]
    fn test_error_display() {
        let error = MethodDispatchError {
            handler_name: "TestHandler".to_string(),
            method: "unknown".to_string(),
            message: "Method not found".to_string(),
            supported_methods: vec!["call".to_string(), "validate".to_string()],
        };

        let display = error.to_string();
        assert!(display.contains("unknown"));
        assert!(display.contains("TestHandler"));
        assert!(display.contains("call"));
        assert!(display.contains("validate"));
    }

    #[test]
    fn test_handler_access() {
        let handler: Arc<dyn ResolvedHandler> = Arc::new(MockHandler::new("TestHandler"));
        let wrapper = MethodDispatchWrapper::with_default_method(Arc::clone(&handler));

        // Verify we can access the underlying handler
        assert_eq!(wrapper.handler().name(), "TestHandler");
    }

    #[test]
    fn test_wrapper_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MethodDispatchWrapper>();
        assert_send_sync::<MethodDispatchInfo>();
        assert_send_sync::<MethodDispatchError>();
    }
}
