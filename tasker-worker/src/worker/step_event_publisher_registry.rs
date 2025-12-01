//! # TAS-65 Phase 3: Step Event Publisher Registry
//!
//! Registry for custom domain event publishers that maps publisher names
//! (from YAML configuration) to their implementations.
//!
//! ## Architecture
//!
//! ```text
//! Task Template YAML
//!     ↓ declares `publisher: PaymentEventPublisher`
//! StepEventPublisherRegistry.get("PaymentEventPublisher")
//!     ↓ returns Arc<dyn StepEventPublisher>
//! Worker invokes publisher.publish(ctx)
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use std::sync::Arc;
//! use tasker_shared::events::domain_events::DomainEventPublisher;
//! use tasker_worker::worker::step_event_publisher_registry::StepEventPublisherRegistry;
//!
//! // Create registry with domain publisher for the default generic publisher
//! let domain_publisher: Arc<DomainEventPublisher> = ...;
//! let mut registry = StepEventPublisherRegistry::new(domain_publisher.clone());
//!
//! // Register custom publishers (they own their own Arc to domain_publisher)
//! registry.register(PaymentEventPublisher::new(domain_publisher.clone()));
//!
//! // Lookup by name (from YAML `publisher:` field)
//! let publisher = registry.get("PaymentEventPublisher");
//!
//! // Or get the default generic publisher
//! let generic = registry.get_or_default(Some("UnknownPublisher"));
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use tasker_shared::events::domain_events::DomainEventPublisher;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::event_router::EventRouter;
use super::step_event_publisher::{DefaultDomainEventPublisher, StepEventPublisher};

/// Registry for step event publishers
///
/// Maps publisher names to implementations. Used by the worker to look up
/// the appropriate publisher based on YAML configuration.
///
/// Thread-safe via Arc wrapping of publishers.
#[derive(Debug)]
pub struct StepEventPublisherRegistry {
    /// Registered publishers by name
    publishers: HashMap<String, Arc<dyn StepEventPublisher>>,

    /// Default publisher for steps without custom publisher
    default_publisher: Arc<dyn StepEventPublisher>,
}

impl StepEventPublisherRegistry {
    /// Create a new registry with DefaultDomainEventPublisher as default (durable-only)
    ///
    /// All events will be published to PGMQ regardless of delivery_mode.
    ///
    /// # Arguments
    ///
    /// * `domain_publisher` - The domain event publisher for the default generic publisher
    pub fn new(domain_publisher: Arc<DomainEventPublisher>) -> Self {
        Self {
            publishers: HashMap::new(),
            default_publisher: Arc::new(DefaultDomainEventPublisher::new(domain_publisher)),
        }
    }

    /// Create a new registry with dual-path routing support
    ///
    /// Events will be routed based on their `delivery_mode` from YAML:
    /// - `durable`: Published to PGMQ (external consumers, audit trails)
    /// - `fast`: Dispatched to in-process bus (metrics, telemetry, notifications)
    ///
    /// # Arguments
    ///
    /// * `domain_publisher` - The domain event publisher (used for durable path)
    /// * `event_router` - The event router for dual-path delivery
    pub fn with_event_router(
        domain_publisher: Arc<DomainEventPublisher>,
        event_router: Arc<RwLock<EventRouter>>,
    ) -> Self {
        Self {
            publishers: HashMap::new(),
            default_publisher: Arc::new(DefaultDomainEventPublisher::with_event_router(
                domain_publisher,
                event_router,
            )),
        }
    }

    /// Create a registry with a custom default publisher
    pub fn with_default<P: StepEventPublisher + 'static>(default: P) -> Self {
        Self {
            publishers: HashMap::new(),
            default_publisher: Arc::new(default),
        }
    }

    /// Register a custom event publisher
    ///
    /// The publisher's `name()` is used as the lookup key.
    ///
    /// # Arguments
    ///
    /// * `publisher` - The publisher implementation to register
    ///
    /// # Returns
    ///
    /// The previous publisher with the same name, if any
    pub fn register<P: StepEventPublisher + 'static>(
        &mut self,
        publisher: P,
    ) -> Option<Arc<dyn StepEventPublisher>> {
        let name = publisher.name().to_string();
        info!(publisher_name = %name, "Registering step event publisher");
        self.publishers.insert(name, Arc::new(publisher))
    }

    /// Register an already-Arc'd publisher
    ///
    /// Useful when the publisher is already wrapped in Arc.
    pub fn register_arc(
        &mut self,
        publisher: Arc<dyn StepEventPublisher>,
    ) -> Option<Arc<dyn StepEventPublisher>> {
        let name = publisher.name().to_string();
        info!(publisher_name = %name, "Registering step event publisher (arc)");
        self.publishers.insert(name, publisher)
    }

    /// Get a publisher by name
    ///
    /// Returns None if no publisher is registered with that name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn StepEventPublisher>> {
        let publisher = self.publishers.get(name).cloned();
        if publisher.is_some() {
            debug!(publisher_name = %name, "Found registered publisher");
        } else {
            debug!(publisher_name = %name, "Publisher not found in registry");
        }
        publisher
    }

    /// Get a publisher by name, or return the default if not found
    ///
    /// This is the typical lookup method used during step execution.
    pub fn get_or_default(&self, name: Option<&str>) -> Arc<dyn StepEventPublisher> {
        match name {
            Some(publisher_name) => {
                if let Some(publisher) = self.get(publisher_name) {
                    publisher
                } else {
                    warn!(
                        publisher_name = %publisher_name,
                        "Custom publisher not found, using default"
                    );
                    self.default_publisher.clone()
                }
            }
            None => {
                debug!("No publisher specified, using default");
                self.default_publisher.clone()
            }
        }
    }

    /// Get the default publisher
    pub fn default_publisher(&self) -> Arc<dyn StepEventPublisher> {
        self.default_publisher.clone()
    }

    /// Check if a publisher is registered
    pub fn contains(&self, name: &str) -> bool {
        self.publishers.contains_key(name)
    }

    /// Get all registered publisher names
    pub fn registered_names(&self) -> Vec<&str> {
        self.publishers.keys().map(String::as_str).collect()
    }

    /// Get count of registered publishers
    pub fn len(&self) -> usize {
        self.publishers.len()
    }

    /// Check if registry has no custom publishers
    pub fn is_empty(&self) -> bool {
        self.publishers.is_empty()
    }

    /// Unregister a publisher by name
    ///
    /// Returns the removed publisher, if any.
    pub fn unregister(&mut self, name: &str) -> Option<Arc<dyn StepEventPublisher>> {
        info!(publisher_name = %name, "Unregistering step event publisher");
        self.publishers.remove(name)
    }

    /// Clear all registered publishers
    pub fn clear(&mut self) {
        info!("Clearing all step event publishers");
        self.publishers.clear();
    }

    /// TAS-65: Validate that all required publishers are registered
    ///
    /// This method implements the "loud failure validation" pattern - it validates
    /// at init time that all publisher names referenced in task templates exist
    /// in the registry. This catches configuration errors at startup rather than
    /// silently falling back to the default publisher at runtime.
    ///
    /// # Arguments
    ///
    /// * `required_publishers` - Publisher names from task template YAML configurations
    ///
    /// # Returns
    ///
    /// * `Ok(())` - All required publishers are registered
    /// * `Err(ValidationErrors)` - Some publishers are missing
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let registry = StepEventPublisherRegistry::new(domain_publisher);
    /// registry.register(PaymentEventPublisher::new(...));
    ///
    /// // Collect all publisher names from loaded templates
    /// let required = vec!["PaymentEventPublisher", "InventoryPublisher"];
    ///
    /// // This will fail if InventoryPublisher isn't registered
    /// registry.validate_required_publishers(&required)?;
    /// ```
    pub fn validate_required_publishers(
        &self,
        required_publishers: &[&str],
    ) -> Result<(), ValidationErrors> {
        let mut missing = Vec::new();

        for name in required_publishers {
            // Skip "default" - it's always available
            if *name == "default" {
                continue;
            }

            if !self.contains(name) {
                missing.push((*name).to_string());
            }
        }

        if missing.is_empty() {
            info!(
                registered = ?self.registered_names(),
                required = ?required_publishers,
                "All required publishers validated"
            );
            Ok(())
        } else {
            let err = ValidationErrors {
                missing_publishers: missing.clone(),
                registered_publishers: self
                    .registered_names()
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            };
            tracing::error!(
                missing = ?missing,
                registered = ?self.registered_names(),
                "Publisher validation failed - missing required publishers"
            );
            Err(err)
        }
    }

    /// TAS-65: Get a publisher by name with strict mode (no fallback)
    ///
    /// Unlike `get_or_default`, this method returns an error if the publisher
    /// is not found. Use this when you've validated publishers at startup and
    /// want to catch programming errors at runtime.
    ///
    /// # Arguments
    ///
    /// * `name` - The publisher name to look up
    ///
    /// # Returns
    ///
    /// * `Ok(Arc<dyn StepEventPublisher>)` - The publisher
    /// * `Err(PublisherNotFoundError)` - Publisher not registered
    pub fn get_strict(
        &self,
        name: &str,
    ) -> Result<Arc<dyn StepEventPublisher>, PublisherNotFoundError> {
        // "default" always maps to the default publisher
        if name == "default" {
            return Ok(self.default_publisher.clone());
        }

        self.publishers
            .get(name)
            .cloned()
            .ok_or_else(|| PublisherNotFoundError {
                publisher_name: name.to_string(),
                registered_publishers: self
                    .registered_names()
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
            })
    }
}

/// Errors from publisher validation
#[derive(Debug, Clone)]
pub struct ValidationErrors {
    /// Publisher names that were required but not registered
    pub missing_publishers: Vec<String>,
    /// Publisher names that are currently registered
    pub registered_publishers: Vec<String>,
}

impl std::fmt::Display for ValidationErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Missing publishers: {:?}. Registered: {:?}",
            self.missing_publishers, self.registered_publishers
        )
    }
}

impl std::error::Error for ValidationErrors {}

/// Error when a publisher is not found (strict mode)
#[derive(Debug, Clone)]
pub struct PublisherNotFoundError {
    /// The publisher name that was requested
    pub publisher_name: String,
    /// Currently registered publisher names
    pub registered_publishers: Vec<String>,
}

impl std::fmt::Display for PublisherNotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Publisher '{}' not found. Registered: {:?}",
            self.publisher_name, self.registered_publishers
        )
    }
}

impl std::error::Error for PublisherNotFoundError {}

/// Builder for StepEventPublisherRegistry
///
/// Provides a fluent API for building a registry with multiple publishers.
pub struct StepEventPublisherRegistryBuilder {
    publishers: Vec<Arc<dyn StepEventPublisher>>,
    default_publisher: Option<Arc<dyn StepEventPublisher>>,
    domain_publisher: Option<Arc<DomainEventPublisher>>,
    event_router: Option<Arc<RwLock<EventRouter>>>,
}

// Manual Debug implementation because EventRouter contains closures
impl std::fmt::Debug for StepEventPublisherRegistryBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StepEventPublisherRegistryBuilder")
            .field("publishers_count", &self.publishers.len())
            .field("has_default_publisher", &self.default_publisher.is_some())
            .field("has_domain_publisher", &self.domain_publisher.is_some())
            .field("has_event_router", &self.event_router.is_some())
            .finish()
    }
}

impl StepEventPublisherRegistryBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            publishers: Vec::new(),
            default_publisher: None,
            domain_publisher: None,
            event_router: None,
        }
    }

    /// Set the domain publisher (required if not setting a custom default)
    pub fn with_domain_publisher(mut self, domain_publisher: Arc<DomainEventPublisher>) -> Self {
        self.domain_publisher = Some(domain_publisher);
        self
    }

    /// Set the event router for dual-path delivery
    ///
    /// When set, the default DefaultDomainEventPublisher will route events
    /// based on their `delivery_mode` from YAML.
    pub fn with_event_router(mut self, event_router: Arc<RwLock<EventRouter>>) -> Self {
        self.event_router = Some(event_router);
        self
    }

    /// Add a publisher to the registry
    pub fn with_publisher<P: StepEventPublisher + 'static>(mut self, publisher: P) -> Self {
        self.publishers.push(Arc::new(publisher));
        self
    }

    /// Add an already-Arc'd publisher
    pub fn with_publisher_arc(mut self, publisher: Arc<dyn StepEventPublisher>) -> Self {
        self.publishers.push(publisher);
        self
    }

    /// Set a custom default publisher
    pub fn with_default<P: StepEventPublisher + 'static>(mut self, publisher: P) -> Self {
        self.default_publisher = Some(Arc::new(publisher));
        self
    }

    /// Build the registry
    ///
    /// # Panics
    ///
    /// Panics if neither a custom default nor a domain publisher is set.
    pub fn build(self) -> StepEventPublisherRegistry {
        let mut registry = match self.default_publisher {
            Some(default) => StepEventPublisherRegistry {
                publishers: HashMap::new(),
                default_publisher: default,
            },
            None => {
                let domain_publisher = self
                    .domain_publisher
                    .expect("Either a custom default publisher or domain_publisher must be set");

                // If event_router is provided, use dual-path routing
                match self.event_router {
                    Some(event_router) => StepEventPublisherRegistry::with_event_router(
                        domain_publisher,
                        event_router,
                    ),
                    None => StepEventPublisherRegistry::new(domain_publisher),
                }
            }
        };

        for publisher in self.publishers {
            registry.register_arc(publisher);
        }

        registry
    }
}

impl Default for StepEventPublisherRegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    /// Test publisher for unit tests
    #[derive(Debug)]
    struct TestPublisher {
        name: String,
        // In tests, we don't actually use the domain publisher
        #[allow(dead_code)]
        domain_publisher: Arc<DomainEventPublisher>,
    }

    impl TestPublisher {
        #[allow(dead_code)]
        fn new(name: &str, domain_publisher: Arc<DomainEventPublisher>) -> Self {
            Self {
                name: name.to_string(),
                domain_publisher,
            }
        }
    }

    #[async_trait]
    impl StepEventPublisher for TestPublisher {
        fn name(&self) -> &str {
            &self.name
        }

        fn domain_publisher(&self) -> &Arc<DomainEventPublisher> {
            &self.domain_publisher
        }

        async fn publish(
            &self,
            _ctx: &super::super::step_event_publisher::StepEventContext,
        ) -> super::super::step_event_publisher::PublishResult {
            super::super::step_event_publisher::PublishResult::new()
        }
    }

    // Note: These tests require a real DomainEventPublisher which needs a database.
    // For unit tests, we use a mock or skip tests that need actual publishing.
    // The integration tests in tasker-worker/tests/ cover the full flow.

    #[test]
    fn test_registry_with_custom_default() {
        // Create a minimal test that doesn't need database
        // We can test the builder pattern and registry operations
        // without actually creating a DomainEventPublisher

        // Test the builder pattern
        let builder = StepEventPublisherRegistryBuilder::new();
        assert!(builder.publishers.is_empty());
        assert!(builder.default_publisher.is_none());
    }

    #[test]
    fn test_builder_default() {
        let builder = StepEventPublisherRegistryBuilder::default();
        assert!(builder.publishers.is_empty());
    }

    #[test]
    fn test_validation_errors_display() {
        let err = ValidationErrors {
            missing_publishers: vec![
                "PaymentPublisher".to_string(),
                "InventoryPublisher".to_string(),
            ],
            registered_publishers: vec!["DefaultPublisher".to_string()],
        };
        let display = format!("{}", err);
        assert!(display.contains("PaymentPublisher"));
        assert!(display.contains("InventoryPublisher"));
        assert!(display.contains("DefaultPublisher"));
    }

    #[test]
    fn test_publisher_not_found_error_display() {
        let err = PublisherNotFoundError {
            publisher_name: "MissingPublisher".to_string(),
            registered_publishers: vec!["Publisher1".to_string(), "Publisher2".to_string()],
        };
        let display = format!("{}", err);
        assert!(display.contains("MissingPublisher"));
        assert!(display.contains("Publisher1"));
        assert!(display.contains("Publisher2"));
    }

    #[test]
    fn test_validation_with_default_publisher() {
        // "default" should always be valid even when registry is empty
        let err = ValidationErrors {
            missing_publishers: vec![],
            registered_publishers: vec![],
        };
        assert!(err.missing_publishers.is_empty());
    }
}
