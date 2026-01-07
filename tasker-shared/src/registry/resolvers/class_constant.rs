//! # Class Constant Resolver
//!
//! TAS-93: Fallback resolver for class-path style callables.
//!
//! This resolver is the last in the chain and attempts to resolve callables
//! that look like class paths (e.g., "MyApp::Handlers::PaymentHandler").
//!
//! ## Rust Implementation
//!
//! In Rust, runtime class lookup is not possible. This resolver serves as:
//! - A fallback that logs warnings for unresolved class-path callables
//! - A placeholder for consistency with Ruby/Python workers
//! - Documentation of the expected callable format
//!
//! ## Priority
//!
//! Priority: **100** (checked last in the resolver chain)
//!
//! ## When This Resolver Runs
//!
//! This resolver runs when:
//! 1. No explicit mapping was found for the callable
//! 2. No custom resolver matched the callable format
//!
//! ## Class Path Formats
//!
//! Recognized class path formats (for logging purposes):
//! - Ruby style: `MyApp::Handlers::PaymentHandler`
//! - Python style: `myapp.handlers.PaymentHandler`
//! - Rust style: `myapp::handlers::PaymentHandler`
//!
//! ## What to Do When Resolution Fails
//!
//! If you're seeing warnings from this resolver, you need to:
//! 1. Register the handler explicitly with `ExplicitMappingResolver`
//! 2. Create a custom resolver for your callable format
//! 3. Use a resolver hint in the YAML to bypass the chain

use crate::models::core::task_template::HandlerDefinition;
use crate::registry::{ResolutionContext, ResolvedHandler, StepHandlerResolver};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::warn;

/// Resolver for class-path style callables.
///
/// In Rust, this resolver always returns `None` since runtime class
/// instantiation is not possible. It logs warnings when callables
/// look like class paths to help developers debug resolution failures.
#[derive(Debug, Clone, Copy, Default)]
pub struct ClassConstantResolver;

impl ClassConstantResolver {
    /// Create a new class constant resolver.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// Check if a callable looks like a class path.
    ///
    /// Returns `true` for callables that match common class path patterns:
    /// - Contains `::` (Ruby/Rust style)
    /// - Contains `.` and starts with lowercase (Python style)
    /// - Starts with uppercase letter (likely a class name)
    #[must_use]
    pub fn looks_like_class_path(callable: &str) -> bool {
        // Empty or simple names don't look like class paths
        if callable.is_empty() {
            return false;
        }

        // Ruby/Rust style: Foo::Bar::Handler
        if callable.contains("::") {
            return true;
        }

        // Python style: foo.bar.Handler
        if callable.contains('.') {
            return true;
        }

        // Starts with uppercase and contains no special characters
        // (likely intended to be a class name)
        if callable
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_uppercase())
            && callable.chars().all(|c| c.is_alphanumeric() || c == '_')
        {
            return true;
        }

        false
    }

    /// Get the likely language based on callable format.
    #[must_use]
    pub fn detect_callable_style(callable: &str) -> &'static str {
        if callable.contains("::") {
            if callable
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_uppercase())
            {
                "Ruby"
            } else {
                "Rust"
            }
        } else if callable.contains('.') {
            "Python"
        } else {
            "Unknown"
        }
    }
}

#[async_trait]
impl StepHandlerResolver for ClassConstantResolver {
    fn can_resolve(&self, definition: &HandlerDefinition) -> bool {
        // In Rust, we can never actually resolve class paths at runtime
        // But we return true for class-path-like callables so we can log a warning
        Self::looks_like_class_path(&definition.callable)
    }

    async fn resolve(
        &self,
        definition: &HandlerDefinition,
        _context: &ResolutionContext,
    ) -> Option<Arc<dyn ResolvedHandler>> {
        // Log a warning to help developers understand why resolution failed
        let style = Self::detect_callable_style(&definition.callable);
        warn!(
            callable = %definition.callable,
            style = style,
            "ClassConstantResolver cannot resolve class paths in Rust. \
             Register the handler explicitly with ExplicitMappingResolver \
             or use a custom resolver."
        );

        // Rust cannot do runtime class instantiation
        None
    }

    fn resolver_name(&self) -> &str {
        "ClassConstantResolver"
    }

    fn priority(&self) -> u32 {
        100 // Lowest priority - checked last
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_resolver() {
        let resolver = ClassConstantResolver::new();
        assert_eq!(resolver.resolver_name(), "ClassConstantResolver");
        assert_eq!(resolver.priority(), 100);
    }

    #[test]
    fn test_default_resolver() {
        let resolver: ClassConstantResolver = Default::default();
        assert_eq!(resolver.resolver_name(), "ClassConstantResolver");
    }

    #[test]
    fn test_looks_like_class_path_ruby_style() {
        assert!(ClassConstantResolver::looks_like_class_path(
            "MyApp::Handlers::PaymentHandler"
        ));
        assert!(ClassConstantResolver::looks_like_class_path("Foo::Bar"));
    }

    #[test]
    fn test_looks_like_class_path_rust_style() {
        assert!(ClassConstantResolver::looks_like_class_path(
            "myapp::handlers::payment"
        ));
        assert!(ClassConstantResolver::looks_like_class_path(
            "crate::module::Type"
        ));
    }

    #[test]
    fn test_looks_like_class_path_python_style() {
        assert!(ClassConstantResolver::looks_like_class_path(
            "myapp.handlers.PaymentHandler"
        ));
        assert!(ClassConstantResolver::looks_like_class_path("foo.bar.Baz"));
    }

    #[test]
    fn test_looks_like_class_path_simple_class_name() {
        assert!(ClassConstantResolver::looks_like_class_path(
            "PaymentHandler"
        ));
        assert!(ClassConstantResolver::looks_like_class_path("MyHandler"));
    }

    #[test]
    fn test_does_not_look_like_class_path() {
        assert!(!ClassConstantResolver::looks_like_class_path(""));
        assert!(!ClassConstantResolver::looks_like_class_path("my_handler"));
        assert!(!ClassConstantResolver::looks_like_class_path(
            "payments:refund"
        ));
        assert!(!ClassConstantResolver::looks_like_class_path("simple-key"));
    }

    #[test]
    fn test_detect_callable_style_ruby() {
        assert_eq!(
            ClassConstantResolver::detect_callable_style("MyApp::Handlers::Payment"),
            "Ruby"
        );
    }

    #[test]
    fn test_detect_callable_style_rust() {
        assert_eq!(
            ClassConstantResolver::detect_callable_style("myapp::handlers::payment"),
            "Rust"
        );
    }

    #[test]
    fn test_detect_callable_style_python() {
        assert_eq!(
            ClassConstantResolver::detect_callable_style("myapp.handlers.Payment"),
            "Python"
        );
    }

    #[test]
    fn test_detect_callable_style_unknown() {
        assert_eq!(
            ClassConstantResolver::detect_callable_style("simple_key"),
            "Unknown"
        );
    }

    #[test]
    fn test_can_resolve_class_path() {
        let resolver = ClassConstantResolver::new();

        let class_path_def = HandlerDefinition::builder()
            .callable("MyApp::Handlers::Payment".to_string())
            .build();
        assert!(resolver.can_resolve(&class_path_def));

        let python_def = HandlerDefinition::builder()
            .callable("myapp.handlers.Payment".to_string())
            .build();
        assert!(resolver.can_resolve(&python_def));
    }

    #[test]
    fn test_can_resolve_simple_key() {
        let resolver = ClassConstantResolver::new();

        let simple_def = HandlerDefinition::builder()
            .callable("my_handler".to_string())
            .build();
        assert!(!resolver.can_resolve(&simple_def));

        let prefixed_def = HandlerDefinition::builder()
            .callable("payments:refund".to_string())
            .build();
        assert!(!resolver.can_resolve(&prefixed_def));
    }

    #[tokio::test]
    async fn test_resolve_always_returns_none() {
        let resolver = ClassConstantResolver::new();

        let definition = HandlerDefinition::builder()
            .callable("MyApp::Handlers::Payment".to_string())
            .build();
        let context = ResolutionContext::default();

        let resolved = resolver.resolve(&definition, &context).await;
        assert!(resolved.is_none());
    }

    #[test]
    fn test_priority_is_100() {
        let resolver = ClassConstantResolver::new();
        assert_eq!(resolver.priority(), 100);
    }

    #[test]
    fn test_resolver_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ClassConstantResolver>();
    }

    #[test]
    fn test_resolver_is_copy() {
        fn assert_copy<T: Copy>() {}
        assert_copy::<ClassConstantResolver>();
    }

    #[test]
    fn test_debug_formatting() {
        let resolver = ClassConstantResolver::new();
        let debug_str = format!("{:?}", resolver);
        assert_eq!(debug_str, "ClassConstantResolver");
    }
}
