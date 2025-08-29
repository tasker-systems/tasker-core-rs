//! # Event-Driven System Abstractions - Shared Patterns
//!
//! This module provides unified trait-based architecture for managing event-driven patterns
//! across different contexts (orchestration, worker systems, etc.). The design abstracts
//! common patterns while respecting the distinction between different event types:
//!
//! - **Queue-level events**: Step results, task requests (using queue mechanisms like pgmq)
//! - **Database-level events**: Task readiness, state changes (using PostgreSQL LISTEN/NOTIFY)
//! - **Worker-level events**: Task completion, error notifications (using various mechanisms)
//!
//! ## Design Principles
//!
//! - **Unified Interface**: Common trait for all event-driven systems
//! - **Context Awareness**: Respects different event types and sources
//! - **Deployment Mode Support**: PollingOnly, Hybrid, EventDrivenOnly patterns
//! - **Configuration Driven**: TOML-based configuration for all systems
//! - **Cross-System Reuse**: Same patterns work across orchestration and worker contexts

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Duration;

use crate::deployment::{DeploymentMode, DeploymentModeError, DeploymentModeHealthStatus};

/// Unified trait for all event-driven systems
///
/// This trait provides a common interface for managing event-driven patterns across
/// different contexts while respecting the architectural differences between event types.
#[async_trait]
pub trait EventDrivenSystem: Send + Sync {
    /// Unique identifier for this event system instance
    type SystemId: Send + Sync + Clone + fmt::Display + fmt::Debug;
    
    /// Event type that this system processes
    type Event: Send + Sync + Clone + fmt::Debug;
    
    /// Configuration type for this event system
    type Config: Send + Sync + Clone;
    
    /// Statistics type for monitoring this system
    type Statistics: EventSystemStatistics + Send + Sync + Clone;

    /// Get the unique identifier for this system instance
    fn system_id(&self) -> Self::SystemId;

    /// Get the current deployment mode (PollingOnly/Hybrid/EventDrivenOnly)
    fn deployment_mode(&self) -> DeploymentMode;

    /// Check if this system is currently running
    fn is_running(&self) -> bool;

    /// Start the event-driven system
    ///
    /// This method should:
    /// 1. Initialize event listeners based on deployment mode
    /// 2. Start event processing loops
    /// 3. Start fallback polling if in Hybrid mode
    /// 4. Begin health monitoring
    async fn start(&mut self) -> Result<(), DeploymentModeError>;

    /// Stop the event-driven system gracefully
    ///
    /// This method should:
    /// 1. Stop accepting new events
    /// 2. Complete in-flight processing
    /// 3. Clean up resources
    /// 4. Close connections
    async fn stop(&mut self) -> Result<(), DeploymentModeError>;

    /// Get current system statistics for monitoring
    fn statistics(&self) -> Self::Statistics;

    /// Process a single event
    ///
    /// This is the core event processing method that each system implements
    /// according to its specific delegation patterns.
    async fn process_event(&self, event: Self::Event) -> Result<(), DeploymentModeError>;

    /// Health check for the event-driven system
    ///
    /// Returns Ok(()) if the system is healthy, Err if there are issues.
    /// Used by deployment mode orchestrators for rollback detection.
    async fn health_check(&self) -> Result<DeploymentModeHealthStatus, DeploymentModeError>;

    /// Get system configuration
    fn config(&self) -> &Self::Config;
}

/// Event system statistics trait for monitoring
pub trait EventSystemStatistics: fmt::Debug + Clone {
    /// Total events processed successfully
    fn events_processed(&self) -> u64;
    
    /// Total events that failed processing
    fn events_failed(&self) -> u64;
    
    /// Current event processing rate (events/second)
    fn processing_rate(&self) -> f64;
    
    /// Average event processing latency in milliseconds
    fn average_latency_ms(&self) -> f64;
    
    /// Current deployment mode effectiveness score (0.0-1.0)
    /// Higher scores indicate better performance/reliability for the current deployment mode
    fn deployment_mode_score(&self) -> f64;
    
    /// Get success rate (0.0-1.0)
    fn success_rate(&self) -> f64 {
        let total = self.events_processed() + self.events_failed();
        if total == 0 { 1.0 } else { self.events_processed() as f64 / total as f64 }
    }
}

/// Base configuration shared by all event-driven systems
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSystemBaseConfig {
    /// System identifier
    pub system_id: String,
    /// Deployment mode for this system
    pub deployment_mode: DeploymentMode,
    /// Enable health monitoring
    pub health_monitoring_enabled: bool,
    /// Health check interval
    pub health_check_interval: Duration,
    /// Maximum concurrent event processors
    pub max_concurrent_processors: usize,
    /// Event processing timeout
    pub processing_timeout: Duration,
}

impl Default for EventSystemBaseConfig {
    fn default() -> Self {
        Self {
            system_id: "event-system".to_string(),
            deployment_mode: DeploymentMode::Hybrid,
            health_monitoring_enabled: true,
            health_check_interval: Duration::from_secs(30),
            max_concurrent_processors: 10,
            processing_timeout: Duration::from_millis(100),
        }
    }
}

/// Notification enum for event system lifecycle events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventSystemNotification {
    /// System started successfully
    Started { 
        system_id: String, 
        deployment_mode: DeploymentMode 
    },
    /// System stopped gracefully
    Stopped { 
        system_id: String 
    },
    /// Event processed successfully
    EventProcessed { 
        system_id: String, 
        event_type: String, 
        latency_ms: u64 
    },
    /// Event processing failed
    EventFailed { 
        system_id: String, 
        event_type: String, 
        error: String 
    },
    /// Health check passed
    HealthCheckPassed { 
        system_id: String,
        status: DeploymentModeHealthStatus,
    },
    /// Health check failed (potential rollback trigger)
    HealthCheckFailed { 
        system_id: String, 
        status: DeploymentModeHealthStatus,
        error: String 
    },
    /// Deployment mode changed
    DeploymentModeChanged { 
        system_id: String, 
        from: DeploymentMode, 
        to: DeploymentMode 
    },
}

/// Common statistics implementation that can be used by concrete systems
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemStatistics {
    pub events_processed: u64,
    pub events_failed: u64,
    pub processing_rate: f64,
    pub average_latency_ms: f64,
    pub deployment_mode_score: f64,
}

impl EventSystemStatistics for SystemStatistics {
    fn events_processed(&self) -> u64 { self.events_processed }
    fn events_failed(&self) -> u64 { self.events_failed }
    fn processing_rate(&self) -> f64 { self.processing_rate }
    fn average_latency_ms(&self) -> f64 { self.average_latency_ms }
    fn deployment_mode_score(&self) -> f64 { self.deployment_mode_score }
}

/// Factory trait for creating event-driven systems
#[async_trait]
pub trait EventSystemFactory<T: EventDrivenSystem> {
    /// Create a new event system instance
    async fn create_system(
        config: T::Config,
    ) -> Result<T, DeploymentModeError>;
}

/// Event context for providing additional information during event processing
#[derive(Debug, Clone)]
pub struct EventContext {
    /// System that generated this event
    pub source_system: String,
    /// Timestamp when event was created
    pub created_at: std::time::SystemTime,
    /// Optional correlation ID for tracing
    pub correlation_id: Option<String>,
    /// Event metadata
    pub metadata: std::collections::HashMap<String, String>,
}

impl Default for EventContext {
    fn default() -> Self {
        Self {
            source_system: "unknown".to_string(),
            created_at: std::time::SystemTime::now(),
            correlation_id: None,
            metadata: std::collections::HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone)]
    struct MockEventSystem {
        system_id: String,
        deployment_mode: DeploymentMode,
        is_running: bool,
        config: EventSystemBaseConfig,
        statistics: SystemStatistics,
    }

    #[derive(Debug, Clone)]
    struct MockEvent {
        data: String,
    }

    #[async_trait]
    impl EventDrivenSystem for MockEventSystem {
        type SystemId = String;
        type Event = MockEvent;
        type Config = EventSystemBaseConfig;
        type Statistics = SystemStatistics;

        fn system_id(&self) -> Self::SystemId {
            self.system_id.clone()
        }

        fn deployment_mode(&self) -> DeploymentMode {
            self.deployment_mode.clone()
        }

        fn is_running(&self) -> bool {
            self.is_running
        }

        async fn start(&mut self) -> Result<(), DeploymentModeError> {
            self.is_running = true;
            Ok(())
        }

        async fn stop(&mut self) -> Result<(), DeploymentModeError> {
            self.is_running = false;
            Ok(())
        }

        fn statistics(&self) -> Self::Statistics {
            self.statistics.clone()
        }

        async fn process_event(&self, _event: Self::Event) -> Result<(), DeploymentModeError> {
            Ok(())
        }

        async fn health_check(&self) -> Result<DeploymentModeHealthStatus, DeploymentModeError> {
            Ok(DeploymentModeHealthStatus::Healthy)
        }

        fn config(&self) -> &Self::Config {
            &self.config
        }
    }

    #[tokio::test]
    async fn test_event_driven_system_lifecycle() {
        let config = EventSystemBaseConfig::default();
        let mut system = MockEventSystem {
            system_id: "test-system".to_string(),
            deployment_mode: DeploymentMode::Hybrid,
            is_running: false,
            config: config.clone(),
            statistics: SystemStatistics::default(),
        };

        assert!(!system.is_running());
        
        system.start().await.unwrap();
        assert!(system.is_running());
        
        let health = system.health_check().await.unwrap();
        assert_eq!(health, DeploymentModeHealthStatus::Healthy);
        
        system.stop().await.unwrap();
        assert!(!system.is_running());
    }

    #[test]
    fn test_system_statistics() {
        let stats = SystemStatistics {
            events_processed: 100,
            events_failed: 10,
            processing_rate: 50.0,
            average_latency_ms: 25.0,
            deployment_mode_score: 0.8,
        };

        assert_eq!(stats.success_rate(), 0.9090909090909091); // 100/(100+10)
        assert_eq!(stats.events_processed(), 100);
        assert_eq!(stats.events_failed(), 10);
    }
}