//! # Circuit Breaker Manager
//!
//! Manages multiple circuit breakers for different system components.
//! Provides centralized control and metrics aggregation.

use crate::config::tasker::CircuitBreakerConfig;
use crate::resilience::{CircuitBreaker, CircuitBreakerMetrics, SystemCircuitBreakerMetrics};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

/// Manager for multiple circuit breakers across system components
#[derive(Debug)]
pub struct CircuitBreakerManager {
    /// Collection of circuit breakers by component name
    circuit_breakers: Arc<RwLock<HashMap<String, Arc<CircuitBreaker>>>>,

    /// Configuration
    config: CircuitBreakerConfig,
}

impl CircuitBreakerManager {
    /// Create new circuit breaker manager from legacy configuration
    pub fn from_config(config: &CircuitBreakerConfig) -> Self {
        info!("Initializing circuit breaker manager from legacy configuration");

        Self {
            circuit_breakers: Arc::new(RwLock::new(HashMap::new())),
            config: config.clone(),
        }
    }

    /// Get or create circuit breaker for a component
    pub async fn get_circuit_breaker(&self, component_name: &str) -> Arc<CircuitBreaker> {
        // Try to get existing circuit breaker
        {
            let breakers = self.circuit_breakers.read().await;
            if let Some(breaker) = breakers.get(component_name) {
                return Arc::clone(breaker);
            }
        }

        // Create new circuit breaker
        let mut breakers = self.circuit_breakers.write().await;

        // Double-check pattern (another thread might have created it)
        if let Some(breaker) = breakers.get(component_name) {
            return Arc::clone(breaker);
        }

        // Check limits (V2 config uses u32, cast to usize for comparison)
        if breakers.len() >= self.config.global_settings.max_circuit_breakers as usize {
            warn!(
                component = component_name,
                current_count = breakers.len(),
                max_allowed = self.config.global_settings.max_circuit_breakers,
                "🚨 Maximum circuit breaker limit reached, using default config"
            );
        }

        // Get configuration for this component
        let component_config = self
            .config
            .config_for_component(component_name)
            .to_resilience_config();

        // Create new circuit breaker
        let breaker = Arc::new(CircuitBreaker::new(
            component_name.to_string(),
            component_config,
        ));

        breakers.insert(component_name.to_string(), Arc::clone(&breaker));

        info!(
            component = component_name,
            total_circuit_breakers = breakers.len(),
            "Created new circuit breaker"
        );

        breaker
    }

    /// Get all circuit breaker names
    pub async fn list_components(&self) -> Vec<String> {
        let breakers = self.circuit_breakers.read().await;
        breakers.keys().cloned().collect()
    }

    /// Get metrics for a specific circuit breaker
    pub async fn get_component_metrics(
        &self,
        component_name: &str,
    ) -> Option<CircuitBreakerMetrics> {
        let breakers = self.circuit_breakers.read().await;
        if let Some(breaker) = breakers.get(component_name) {
            Some(breaker.metrics().await)
        } else {
            None
        }
    }

    /// Get system-wide circuit breaker metrics
    pub async fn get_system_metrics(&self) -> SystemCircuitBreakerMetrics {
        let mut system_metrics = SystemCircuitBreakerMetrics::new();

        let breakers = self.circuit_breakers.read().await;
        for (name, breaker) in breakers.iter() {
            let metrics = breaker.metrics().await;
            system_metrics.add_circuit_breaker(name.clone(), metrics);
        }

        system_metrics
    }

    /// Force open all circuit breakers (emergency stop)
    pub async fn force_open_all(&self) {
        warn!("🚨 Forcing all circuit breakers open (emergency stop)");

        let breakers = self.circuit_breakers.read().await;
        for (name, breaker) in breakers.iter() {
            breaker.force_open().await;
            warn!(component = name, "🚨 Circuit breaker forced open");
        }
    }

    /// Force close all circuit breakers (emergency recovery)
    pub async fn force_close_all(&self) {
        warn!("🚨 Forcing all circuit breakers closed (emergency recovery)");

        let breakers = self.circuit_breakers.read().await;
        for (name, breaker) in breakers.iter() {
            breaker.force_closed().await;
            warn!(component = name, "🚨 Circuit breaker forced closed");
        }
    }

    /// Remove circuit breaker for a component
    pub async fn remove_circuit_breaker(&self, component_name: &str) -> bool {
        let mut breakers = self.circuit_breakers.write().await;
        if breakers.remove(component_name).is_some() {
            info!(
                component = component_name,
                remaining_count = breakers.len(),
                "🗑Removed circuit breaker"
            );
            true
        } else {
            false
        }
    }

    /// Get count of circuit breakers by state
    pub async fn get_state_summary(&self) -> HashMap<crate::resilience::CircuitState, usize> {
        let system_metrics = self.get_system_metrics().await;
        system_metrics.count_by_state()
    }

    /// Check overall system health based on circuit breaker states
    pub async fn system_health_score(&self) -> f64 {
        let system_metrics = self.get_system_metrics().await;
        system_metrics.health_score()
    }
}

impl Clone for CircuitBreakerManager {
    fn clone(&self) -> Self {
        Self {
            circuit_breakers: Arc::clone(&self.circuit_breakers),
            config: self.config.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_config() -> CircuitBreakerConfig {
        use crate::config::tasker::{
            CircuitBreakerComponentConfig, CircuitBreakerDefaultConfig,
            ComponentCircuitBreakerConfigs, GlobalCircuitBreakerSettings,
        };

        CircuitBreakerConfig {
            enabled: true,
            global_settings: GlobalCircuitBreakerSettings {
                max_circuit_breakers: 50,
                metrics_collection_interval_seconds: 30,
                min_state_transition_interval_seconds: 1.0,
            },
            default_config: CircuitBreakerDefaultConfig {
                failure_threshold: 5,
                timeout_seconds: 30,
                success_threshold: 2,
            },
            component_configs: ComponentCircuitBreakerConfigs {
                task_readiness: CircuitBreakerComponentConfig {
                    failure_threshold: 5,
                    timeout_seconds: 30,
                    success_threshold: 2,
                },
                pgmq: CircuitBreakerComponentConfig {
                    failure_threshold: 5,
                    timeout_seconds: 30,
                    success_threshold: 2,
                },
            },
        }
    }

    #[tokio::test]
    async fn test_circuit_breaker_manager_creation() {
        let config = create_test_config();
        let manager = CircuitBreakerManager::from_config(&config);

        let components = manager.list_components().await;
        assert!(components.is_empty());

        let health_score = manager.system_health_score().await;
        assert_eq!(health_score, 1.0); // No circuit breakers = healthy
    }

    #[tokio::test]
    async fn test_get_or_create_circuit_breaker() {
        let config = create_test_config();
        let manager = CircuitBreakerManager::from_config(&config);

        // Get circuit breaker (should create new one)
        let breaker1 = manager.get_circuit_breaker("database").await;
        assert_eq!(breaker1.name(), "database");

        // Get same circuit breaker (should return existing)
        let breaker2 = manager.get_circuit_breaker("database").await;
        assert_eq!(breaker1.name(), breaker2.name());

        // Verify they are the same instance
        assert!(Arc::ptr_eq(&breaker1, &breaker2));

        let components = manager.list_components().await;
        assert_eq!(components.len(), 1);
        assert!(components.contains(&"database".to_string()));
    }

    #[tokio::test]
    async fn test_system_metrics_aggregation() {
        let config = create_test_config();
        let manager = CircuitBreakerManager::from_config(&config);

        // Create multiple circuit breakers
        let _db_breaker = manager.get_circuit_breaker("database").await;
        let _queue_breaker = manager.get_circuit_breaker("queue").await;
        let _external_api_breaker = manager.get_circuit_breaker("external_api").await;

        let system_metrics = manager.get_system_metrics().await;
        assert_eq!(system_metrics.circuit_breakers.len(), 3);

        let state_summary = manager.get_state_summary().await;
        assert_eq!(state_summary.len(), 1); // All should be Closed initially
        assert_eq!(
            state_summary.get(&crate::resilience::CircuitState::Closed),
            Some(&3)
        );

        let health_score = manager.system_health_score().await;
        assert_eq!(health_score, 1.0); // All healthy
    }
}
