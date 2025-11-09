//! Test for TOML-based circuit breaker configuration

#[cfg(test)]
mod tests {
    use crate::config::tasker::{
        CircuitBreakerComponentConfig, CircuitBreakerConfig, CircuitBreakerDefaultConfig,
        ComponentCircuitBreakerConfigs, GlobalCircuitBreakerSettings,
    };
    use crate::resilience::CircuitBreakerManager;

    /// Test that the new TOML-based configuration works correctly
    #[tokio::test]
    async fn test_toml_based_circuit_breaker_configuration() {
        // Create a TOML-compatible configuration structure using V2 canonical types
        let component_configs = ComponentCircuitBreakerConfigs {
            task_readiness: CircuitBreakerComponentConfig {
                failure_threshold: 3,
                timeout_seconds: 45,
                success_threshold: 2,
            },
            pgmq: CircuitBreakerComponentConfig {
                failure_threshold: 2,
                timeout_seconds: 10,
                success_threshold: 1,
            },
        };

        let toml_config = CircuitBreakerConfig {
            enabled: true,
            global_settings: GlobalCircuitBreakerSettings {
                max_circuit_breakers: 25,
                metrics_collection_interval_seconds: 15,
                min_state_transition_interval_seconds: 0.5,
            },
            default_config: CircuitBreakerDefaultConfig {
                failure_threshold: 4,
                timeout_seconds: 20,
                success_threshold: 2,
            },
            component_configs,
        };

        // Create manager using the TOML-based configuration
        let manager = CircuitBreakerManager::from_config(&toml_config);

        // Test that we can create circuit breakers with the configuration
        let task_readiness_breaker = manager.get_circuit_breaker("task_readiness").await;
        let pgmq_breaker = manager.get_circuit_breaker("pgmq").await;
        let unknown_breaker = manager.get_circuit_breaker("unknown_component").await;

        // Verify circuit breakers were created
        assert_eq!(task_readiness_breaker.name(), "task_readiness");
        assert_eq!(pgmq_breaker.name(), "pgmq");
        assert_eq!(unknown_breaker.name(), "unknown_component");

        // Verify we have the expected number of components
        let components = manager.list_components().await;
        assert_eq!(components.len(), 3);
        assert!(components.contains(&"task_readiness".to_string()));
        assert!(components.contains(&"pgmq".to_string()));
        assert!(components.contains(&"unknown_component".to_string()));

        // Test system health
        let health_score = manager.system_health_score().await;
        assert_eq!(health_score, 1.0); // All circuit breakers should start healthy
    }

    /// Test that environment-specific configurations can be applied
    #[tokio::test]
    async fn test_environment_specific_toml_configuration() {
        // Simulate test environment configuration with faster timeouts
        let component_configs = ComponentCircuitBreakerConfigs {
            task_readiness: CircuitBreakerComponentConfig {
                failure_threshold: 1,
                timeout_seconds: 1,
                success_threshold: 1,
            },
            pgmq: CircuitBreakerComponentConfig {
                failure_threshold: 1,
                timeout_seconds: 1,
                success_threshold: 1,
            },
        };

        let toml_config = CircuitBreakerConfig {
            enabled: true,
            global_settings: GlobalCircuitBreakerSettings {
                max_circuit_breakers: 10,
                metrics_collection_interval_seconds: 1, // Fast metrics in test
                min_state_transition_interval_seconds: 0.01, // Very fast transitions
            },
            default_config: CircuitBreakerDefaultConfig {
                failure_threshold: 1, // Fail fast in tests
                timeout_seconds: 1,   // Short timeout in tests
                success_threshold: 1, // Quick recovery in tests
            },
            component_configs,
        };

        let manager = CircuitBreakerManager::from_config(&toml_config);
        let test_breaker = manager.get_circuit_breaker("test_component").await;

        // Test that the circuit breaker was created with test configuration
        assert_eq!(test_breaker.name(), "test_component");

        // All circuit breakers should start in closed state and be healthy
        let metrics = manager
            .get_component_metrics("test_component")
            .await
            .unwrap();
        assert_eq!(metrics.total_calls, 0);
        assert_eq!(metrics.failure_count, 0);
    }
}
