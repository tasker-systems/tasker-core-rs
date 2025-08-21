//! # Operational State Management
//!
//! Manages system operational states to enable context-aware health monitoring and operations.
//!
//! ## Overview
//!
//! The `SystemOperationalState` provides lifecycle state tracking for the orchestration system,
//! enabling components to adapt their behavior based on whether the system is starting up,
//! running normally, shutting down gracefully, or in an emergency state.
//!
//! ## Key Features
//!
//! - **Lifecycle State Tracking**: Clear enumeration of system operational phases
//! - **Thread-Safe Transitions**: Atomic state changes with proper synchronization
//! - **Health Monitor Integration**: Context-aware health assessment during transitions
//! - **Shutdown Coordination**: Distinguishes intentional vs emergency shutdowns
//! - **Operational Logging**: State-aware logging reduces alert noise
//!
//! ## State Transitions
//!
//! ```text
//! Startup → Normal → GracefulShutdown → Stopped
//!    ↓         ↓            ↓
//! Emergency ← Emergency ← Emergency
//! ```
//!
//! ## TAS-37 Supplemental Integration
//!
//! This module implements the operational state management component of the TAS-37 supplemental
//! enhancement for shutdown-aware health monitoring, eliminating false alerts during planned operations.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::sync::Arc;
use tokio::sync::RwLock;

/// System operational state for lifecycle and health monitoring coordination
///
/// This enum represents the current operational phase of the orchestration system,
/// enabling components to adapt their behavior appropriately.
///
/// # State Descriptions
///
/// - **Normal**: Standard operation with full health monitoring
/// - **GracefulShutdown**: Planned shutdown in progress, suppress health alerts
/// - **Emergency**: Unexpected shutdown or failure state, emergency logging
/// - **Stopped**: System fully stopped, health monitoring suspended
/// - **Startup**: System initializing, reduced health thresholds
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SystemOperationalState {
    /// Normal operation - full health monitoring active
    ///
    /// All health thresholds are enforced, alerts are generated for failures,
    /// and the system operates under normal performance expectations.
    Normal,

    /// Graceful shutdown in progress - suppress health alerts
    ///
    /// The system is intentionally shutting down in a coordinated manner.
    /// Health degradation is expected and should not generate failure alerts.
    GracefulShutdown,

    /// Emergency shutdown - log but don't alert
    ///
    /// The system is shutting down due to an unexpected condition or emergency.
    /// Health monitoring continues but with emergency context logging.
    Emergency,

    /// Fully stopped - health monitoring suspended
    ///
    /// The system has completed shutdown and is no longer operational.
    /// Health monitoring should be completely suspended.
    Stopped,

    /// Starting up - reduced health thresholds
    ///
    /// The system is initializing and coming online.
    /// Health thresholds should be relaxed during this transitional period.
    #[default]
    Startup,
}

impl SystemOperationalState {
    /// Check if health monitoring should be active for this state
    pub fn should_monitor_health(&self) -> bool {
        match self {
            SystemOperationalState::Normal => true,
            SystemOperationalState::Startup => true,
            SystemOperationalState::GracefulShutdown => false, // Monitor but don't alert
            SystemOperationalState::Emergency => true,
            SystemOperationalState::Stopped => false,
        }
    }

    /// Check if health alerts should be suppressed for this state
    pub fn should_suppress_alerts(&self) -> bool {
        match self {
            SystemOperationalState::Normal => false,
            SystemOperationalState::Startup => false,
            SystemOperationalState::GracefulShutdown => true,
            SystemOperationalState::Emergency => false,
            SystemOperationalState::Stopped => true,
        }
    }

    /// Get the health threshold multiplier for this state
    ///
    /// Returns a multiplier to apply to normal health thresholds:
    /// - 1.0 = normal thresholds
    /// - 0.5 = relaxed thresholds (50% of normal)
    /// - 0.0 = no health requirements
    pub fn health_threshold_multiplier(&self) -> f64 {
        match self {
            SystemOperationalState::Normal => 1.0,
            SystemOperationalState::Startup => 0.5, // Relaxed during startup
            SystemOperationalState::GracefulShutdown => 0.0, // No requirements during shutdown
            SystemOperationalState::Emergency => 1.0, // Full monitoring during emergency
            SystemOperationalState::Stopped => 0.0, // No monitoring when stopped
        }
    }

    /// Check if this is a shutdown state (graceful or emergency)
    pub fn is_shutdown(&self) -> bool {
        matches!(
            self,
            SystemOperationalState::GracefulShutdown
                | SystemOperationalState::Emergency
                | SystemOperationalState::Stopped
        )
    }

    /// Check if this is a transitional state (startup or shutdown)
    pub fn is_transitional(&self) -> bool {
        matches!(
            self,
            SystemOperationalState::Startup | SystemOperationalState::GracefulShutdown
        )
    }

    /// Get the appropriate log level for health events in this state
    pub fn health_log_level(&self) -> &'static str {
        match self {
            SystemOperationalState::Normal => "ERROR",
            SystemOperationalState::Startup => "WARN",
            SystemOperationalState::GracefulShutdown => "INFO",
            SystemOperationalState::Emergency => "ERROR",
            SystemOperationalState::Stopped => "DEBUG",
        }
    }

    /// Get a human-readable description of this state
    pub fn description(&self) -> &'static str {
        match self {
            SystemOperationalState::Normal => "System operating normally",
            SystemOperationalState::Startup => "System starting up",
            SystemOperationalState::GracefulShutdown => "System shutting down gracefully",
            SystemOperationalState::Emergency => "System in emergency shutdown",
            SystemOperationalState::Stopped => "System stopped",
        }
    }

    /// Check if transition to the target state is valid
    pub fn can_transition_to(&self, target: &SystemOperationalState) -> bool {
        use SystemOperationalState::*;

        match (self, target) {
            // From Startup
            (Startup, Normal) => true,
            (Startup, Emergency) => true,
            (Startup, Stopped) => true,

            // From Normal
            (Normal, GracefulShutdown) => true,
            (Normal, Emergency) => true,
            (Normal, Stopped) => true,

            // From GracefulShutdown
            (GracefulShutdown, Stopped) => true,
            (GracefulShutdown, Emergency) => true,

            // From Emergency
            (Emergency, Stopped) => true,

            // From Stopped
            (Stopped, Startup) => true,

            // Same state transitions are always valid
            (state, target_state) if state == target_state => true,

            // All other transitions are invalid
            _ => false,
        }
    }
}

impl fmt::Display for SystemOperationalState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            SystemOperationalState::Normal => "Normal",
            SystemOperationalState::GracefulShutdown => "GracefulShutdown",
            SystemOperationalState::Emergency => "Emergency",
            SystemOperationalState::Stopped => "Stopped",
            SystemOperationalState::Startup => "Startup",
        };
        write!(f, "{name}")
    }
}

/// Thread-safe operational state manager for orchestration coordination
///
/// Provides atomic state transitions with validation and proper synchronization
/// for use across multiple components and threads.
///
/// # Example
///
/// ```rust,no_run
/// use tasker_shared::orchestration::coordinator::operational_state::{
///     OperationalStateManager, SystemOperationalState
/// };
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let state_manager = OperationalStateManager::new();
///
/// // Transition from startup to normal operation
/// state_manager.transition_to(SystemOperationalState::Normal).await?;
///
/// // Later, initiate graceful shutdown
/// state_manager.transition_to(SystemOperationalState::GracefulShutdown).await?;
///
/// // Check current state
/// let current = state_manager.current_state().await;
/// println!("Current state: {}", current);
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct OperationalStateManager {
    state: Arc<RwLock<SystemOperationalState>>,
}

impl OperationalStateManager {
    /// Create a new operational state manager starting in Startup state
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(SystemOperationalState::Startup)),
        }
    }

    /// Create a new operational state manager with a specific initial state
    pub fn with_initial_state(initial_state: SystemOperationalState) -> Self {
        Self {
            state: Arc::new(RwLock::new(initial_state)),
        }
    }

    /// Get the current operational state
    pub async fn current_state(&self) -> SystemOperationalState {
        self.state.read().await.clone()
    }

    /// Attempt to transition to a new operational state
    ///
    /// Returns `Ok(())` if the transition is successful, or an error if the transition
    /// is invalid or if there was a concurrency issue.
    pub async fn transition_to(
        &self,
        target_state: SystemOperationalState,
    ) -> Result<(), StateTransitionError> {
        let mut current_state = self.state.write().await;

        if !current_state.can_transition_to(&target_state) {
            return Err(StateTransitionError::InvalidTransition {
                from: current_state.clone(),
                to: target_state,
            });
        }

        tracing::info!(
            from_state = %current_state,
            to_state = %target_state,
            "🔄 STATE TRANSITION: {}",
            target_state.description()
        );

        *current_state = target_state;
        Ok(())
    }

    /// Force a state transition without validation (for emergency situations)
    ///
    /// This should only be used in exceptional circumstances where normal
    /// state transition validation needs to be bypassed.
    pub async fn force_transition_to(&self, target_state: SystemOperationalState) {
        let mut current_state = self.state.write().await;

        tracing::warn!(
            from_state = %current_state,
            to_state = %target_state,
            "⚠️ FORCED STATE TRANSITION: {} (validation bypassed)",
            target_state.description()
        );

        *current_state = target_state;
    }

    /// Check if the system is currently in shutdown mode
    pub async fn is_shutdown(&self) -> bool {
        self.state.read().await.is_shutdown()
    }

    /// Check if the system is currently in a transitional state
    pub async fn is_transitional(&self) -> bool {
        self.state.read().await.is_transitional()
    }

    /// Check if health monitoring should be active
    pub async fn should_monitor_health(&self) -> bool {
        self.state.read().await.should_monitor_health()
    }

    /// Check if health alerts should be suppressed
    pub async fn should_suppress_alerts(&self) -> bool {
        self.state.read().await.should_suppress_alerts()
    }

    /// Get the health threshold multiplier for the current state
    pub async fn health_threshold_multiplier(&self) -> f64 {
        self.state.read().await.health_threshold_multiplier()
    }

    /// Get configuration-aware health threshold multiplier (TAS-37 Supplemental)
    ///
    /// This method uses actual configuration values for operational state thresholds
    /// instead of hardcoded defaults, enabling customizable shutdown-aware monitoring.
    pub async fn health_threshold_multiplier_with_config(
        &self,
        config: &tasker_shared::config::OperationalStateConfig,
    ) -> f64 {
        let state = self.current_state().await;
        match state {
            SystemOperationalState::Normal => 1.0,
            SystemOperationalState::Startup => config.startup_health_threshold_multiplier,
            SystemOperationalState::GracefulShutdown => config.shutdown_health_threshold_multiplier,
            SystemOperationalState::Emergency => 0.0, // Always 0.0 for emergency
            SystemOperationalState::Stopped => 0.0,   // Always 0.0 for stopped
        }
    }
}

impl Default for OperationalStateManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for OperationalStateManager {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
        }
    }
}

/// Errors that can occur during state transitions
#[derive(Debug, thiserror::Error)]
pub enum StateTransitionError {
    #[error("Invalid state transition from {from} to {to}")]
    InvalidTransition {
        from: SystemOperationalState,
        to: SystemOperationalState,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operational_state_properties() {
        let normal = SystemOperationalState::Normal;
        assert!(normal.should_monitor_health());
        assert!(!normal.should_suppress_alerts());
        assert_eq!(normal.health_threshold_multiplier(), 1.0);
        assert!(!normal.is_shutdown());
        assert!(!normal.is_transitional());

        let graceful_shutdown = SystemOperationalState::GracefulShutdown;
        assert!(!graceful_shutdown.should_monitor_health());
        assert!(graceful_shutdown.should_suppress_alerts());
        assert_eq!(graceful_shutdown.health_threshold_multiplier(), 0.0);
        assert!(graceful_shutdown.is_shutdown());
        assert!(graceful_shutdown.is_transitional());

        let startup = SystemOperationalState::Startup;
        assert!(startup.should_monitor_health());
        assert!(!startup.should_suppress_alerts());
        assert_eq!(startup.health_threshold_multiplier(), 0.5);
        assert!(!startup.is_shutdown());
        assert!(startup.is_transitional());
    }

    #[test]
    fn test_valid_state_transitions() {
        use SystemOperationalState::*;

        // Valid transitions from Startup
        assert!(Startup.can_transition_to(&Normal));
        assert!(Startup.can_transition_to(&Emergency));
        assert!(Startup.can_transition_to(&Stopped));

        // Valid transitions from Normal
        assert!(Normal.can_transition_to(&GracefulShutdown));
        assert!(Normal.can_transition_to(&Emergency));
        assert!(Normal.can_transition_to(&Stopped));

        // Valid transitions from GracefulShutdown
        assert!(GracefulShutdown.can_transition_to(&Stopped));
        assert!(GracefulShutdown.can_transition_to(&Emergency));

        // Valid transitions from Emergency
        assert!(Emergency.can_transition_to(&Stopped));

        // Valid transitions from Stopped
        assert!(Stopped.can_transition_to(&Startup));
    }

    #[test]
    fn test_invalid_state_transitions() {
        use SystemOperationalState::*;

        // Invalid transitions - going backwards
        assert!(!Normal.can_transition_to(&Startup));
        assert!(!GracefulShutdown.can_transition_to(&Normal));
        assert!(!Stopped.can_transition_to(&Normal));
        assert!(!Stopped.can_transition_to(&GracefulShutdown));
        assert!(!Emergency.can_transition_to(&Normal));
    }

    #[test]
    fn test_state_display() {
        assert_eq!(SystemOperationalState::Normal.to_string(), "Normal");
        assert_eq!(
            SystemOperationalState::GracefulShutdown.to_string(),
            "GracefulShutdown"
        );
        assert_eq!(SystemOperationalState::Emergency.to_string(), "Emergency");
        assert_eq!(SystemOperationalState::Stopped.to_string(), "Stopped");
        assert_eq!(SystemOperationalState::Startup.to_string(), "Startup");
    }

    #[tokio::test]
    async fn test_operational_state_manager() {
        let manager = OperationalStateManager::new();

        // Should start in Startup state
        assert_eq!(
            manager.current_state().await,
            SystemOperationalState::Startup
        );

        // Valid transition to Normal
        manager
            .transition_to(SystemOperationalState::Normal)
            .await
            .unwrap();
        assert_eq!(
            manager.current_state().await,
            SystemOperationalState::Normal
        );

        // Valid transition to GracefulShutdown
        manager
            .transition_to(SystemOperationalState::GracefulShutdown)
            .await
            .unwrap();
        assert_eq!(
            manager.current_state().await,
            SystemOperationalState::GracefulShutdown
        );

        // Invalid transition should fail
        let result = manager.transition_to(SystemOperationalState::Normal).await;
        assert!(result.is_err());

        // State should remain unchanged after failed transition
        assert_eq!(
            manager.current_state().await,
            SystemOperationalState::GracefulShutdown
        );
    }

    #[tokio::test]
    async fn test_force_transition() {
        let manager = OperationalStateManager::new();

        // Start in Startup
        assert_eq!(
            manager.current_state().await,
            SystemOperationalState::Startup
        );

        // Force invalid transition
        manager
            .force_transition_to(SystemOperationalState::Stopped)
            .await;
        assert_eq!(
            manager.current_state().await,
            SystemOperationalState::Stopped
        );
    }

    #[test]
    fn test_health_log_levels() {
        assert_eq!(SystemOperationalState::Normal.health_log_level(), "ERROR");
        assert_eq!(SystemOperationalState::Startup.health_log_level(), "WARN");
        assert_eq!(
            SystemOperationalState::GracefulShutdown.health_log_level(),
            "INFO"
        );
        assert_eq!(
            SystemOperationalState::Emergency.health_log_level(),
            "ERROR"
        );
        assert_eq!(SystemOperationalState::Stopped.health_log_level(), "DEBUG");
    }
}
