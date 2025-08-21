//! # Executor Health State Machine
//!
//! This module provides a robust state machine implementation for executor health management
//! using manual state management. It ensures proper state transitions and validation for all
//! health state changes, preventing invalid state transitions that could lead to inconsistent
//! health reporting.

use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};
use uuid::Uuid;

use tasker_shared::{TaskerError, TaskerResult};

/// Health states for the executor
#[derive(Debug, Clone, PartialEq)]
pub enum HealthState {
    /// Executor is starting up
    Starting,
    /// Executor is functioning normally
    Healthy,
    /// Executor is experiencing performance issues but still functional
    Degraded,
    /// Executor is not responding or has critical issues
    Unhealthy,
    /// Executor is in the process of stopping
    Stopping,
    /// Executor has stopped
    Stopped,
}

/// Events that can trigger health state transitions
#[derive(Debug, Clone, PartialEq)]
pub enum HealthEvent {
    /// Executor startup initiated
    StartupInitiated { phase: String },
    /// Startup completed successfully
    StartupCompleted,
    /// Startup failed
    StartupFailed { reason: String },
    /// Performance degradation detected
    PerformanceDegraded { reason: String, error_rate: f64 },
    /// Performance recovered to healthy levels
    PerformanceRecovered,
    /// Heartbeat timeout occurred
    HeartbeatTimeout { last_seen: u64 },
    /// Critical failure detected
    CriticalFailure { reason: String },
    /// Graceful shutdown initiated
    GracefulShutdownInitiated { phase: String },
    /// Forced shutdown initiated
    ForcedShutdownInitiated { phase: String },
    /// Shutdown completed
    ShutdownCompleted,
    /// Recovery attempt initiated
    RecoveryAttempted,
}

/// Guard conditions for state transitions
pub struct HealthGuards;

impl HealthGuards {
    /// Check if startup can proceed (basic validation)
    pub fn can_start(_context: &HealthContext) -> bool {
        true // Always allow startup attempts
    }

    /// Check if performance degradation is significant enough to transition
    pub fn is_significant_degradation(context: &HealthContext) -> bool {
        // Only transition to degraded if we have sufficient metrics
        context.total_observations > 5
    }

    /// Check if recovery conditions are met
    pub fn can_recover(context: &HealthContext) -> bool {
        // Must have recent successful operations
        context.recent_success_rate > 0.8 && context.total_observations > 3
    }

    /// Check if failure is critical enough for unhealthy state
    pub fn is_critical_failure(context: &HealthContext) -> bool {
        // Critical if error rate is very high or no heartbeat
        context.error_rate > 0.5 || context.heartbeat_timeout_exceeded
    }

    /// Check if graceful shutdown is possible
    pub fn can_shutdown_gracefully(context: &HealthContext) -> bool {
        // Can shutdown gracefully if not in critical state
        !context.heartbeat_timeout_exceeded
    }
}

/// Actions performed during state transitions
pub struct HealthActions;

impl HealthActions {
    /// Action when starting up
    pub fn on_startup_initiated(context: &mut HealthContext, event: &HealthEvent) {
        if let HealthEvent::StartupInitiated { phase } = event {
            context.current_phase = phase.clone();
            context.state_start_time = current_timestamp();
            info!(
                executor_id = %context.executor_id,
                phase = %phase,
                "Health state machine: Starting up"
            );
        }
    }

    /// Action when becoming healthy
    pub fn on_became_healthy(context: &mut HealthContext, _event: &HealthEvent) {
        context.last_healthy_time = current_timestamp();
        context.degraded_since = None;
        context.unhealthy_since = None;
        info!(
            executor_id = %context.executor_id,
            "Health state machine: Transitioned to healthy"
        );
    }

    /// Action when becoming degraded
    pub fn on_became_degraded(context: &mut HealthContext, event: &HealthEvent) {
        if let HealthEvent::PerformanceDegraded { reason, error_rate } = event {
            context.degraded_reason = Some(reason.clone());
            context.error_rate = *error_rate;
            if context.degraded_since.is_none() {
                context.degraded_since = Some(current_timestamp());
            }
            warn!(
                executor_id = %context.executor_id,
                reason = %reason,
                error_rate = %error_rate,
                "Health state machine: Transitioned to degraded"
            );
        }
    }

    /// Action when becoming unhealthy
    pub fn on_became_unhealthy(context: &mut HealthContext, event: &HealthEvent) {
        match event {
            HealthEvent::HeartbeatTimeout { last_seen } => {
                context.unhealthy_reason = Some("Heartbeat timeout".to_string());
                context.last_seen = *last_seen;
                context.heartbeat_timeout_exceeded = true;
            }
            HealthEvent::CriticalFailure { reason } => {
                context.unhealthy_reason = Some(reason.clone());
            }
            _ => {
                context.unhealthy_reason = Some("Unknown critical failure".to_string());
            }
        }

        if context.unhealthy_since.is_none() {
            context.unhealthy_since = Some(current_timestamp());
        }

        warn!(
            executor_id = %context.executor_id,
            reason = %context.unhealthy_reason.as_ref().unwrap_or(&"Unknown".to_string()),
            "Health state machine: Transitioned to unhealthy"
        );
    }

    /// Action when initiating shutdown
    pub fn on_shutdown_initiated(context: &mut HealthContext, event: &HealthEvent) {
        match event {
            HealthEvent::GracefulShutdownInitiated { phase } => {
                context.current_phase = phase.clone();
                context.graceful_shutdown = true;
                info!(
                    executor_id = %context.executor_id,
                    phase = %phase,
                    "Health state machine: Graceful shutdown initiated"
                );
            }
            HealthEvent::ForcedShutdownInitiated { phase } => {
                context.current_phase = phase.clone();
                context.graceful_shutdown = false;
                warn!(
                    executor_id = %context.executor_id,
                    phase = %phase,
                    "Health state machine: Forced shutdown initiated"
                );
            }
            _ => {}
        }
        context.state_start_time = current_timestamp();
    }

    /// Action when shutdown completes
    pub fn on_shutdown_completed(context: &mut HealthContext, _event: &HealthEvent) {
        info!(
            executor_id = %context.executor_id,
            "Health state machine: Shutdown completed"
        );
    }
}

/// Context data for the health state machine
#[derive(Debug, Clone)]
pub struct HealthContext {
    /// Executor ID for logging and identification
    pub executor_id: Uuid,
    /// When the current state was entered
    pub state_start_time: u64,
    /// Current operational phase (e.g., "initializing", "processing", "shutdown")
    pub current_phase: String,
    /// Total number of health observations recorded
    pub total_observations: usize,
    /// Current error rate (0.0 - 1.0)
    pub error_rate: f64,
    /// Recent success rate (0.0 - 1.0)
    pub recent_success_rate: f64,
    /// Whether heartbeat timeout has been exceeded
    pub heartbeat_timeout_exceeded: bool,
    /// Last time executor was seen alive (Unix timestamp)
    pub last_seen: u64,
    /// Last time executor was healthy
    pub last_healthy_time: u64,
    /// When degraded state began (if applicable)
    pub degraded_since: Option<u64>,
    /// When unhealthy state began (if applicable)
    pub unhealthy_since: Option<u64>,
    /// Reason for degraded state
    pub degraded_reason: Option<String>,
    /// Reason for unhealthy state
    pub unhealthy_reason: Option<String>,
    /// Whether shutdown is graceful
    pub graceful_shutdown: bool,
}

impl HealthContext {
    /// Create a new health context
    pub fn new(executor_id: Uuid) -> Self {
        let now = current_timestamp();
        Self {
            executor_id,
            state_start_time: now,
            current_phase: "initializing".to_string(),
            total_observations: 0,
            error_rate: 0.0,
            recent_success_rate: 1.0,
            heartbeat_timeout_exceeded: false,
            last_seen: now,
            last_healthy_time: now,
            degraded_since: None,
            unhealthy_since: None,
            degraded_reason: None,
            unhealthy_reason: None,
            graceful_shutdown: true,
        }
    }

    /// Update performance metrics
    pub fn update_performance(&mut self, success_rate: f64, error_rate: f64) {
        self.recent_success_rate = success_rate;
        self.error_rate = error_rate;
        self.total_observations += 1;
        self.last_seen = current_timestamp();
    }

    /// Update heartbeat status
    pub fn update_heartbeat(&mut self, timeout_exceeded: bool) {
        self.heartbeat_timeout_exceeded = timeout_exceeded;
        if !timeout_exceeded {
            self.last_seen = current_timestamp();
        }
    }

    /// Get duration in current state (seconds)
    pub fn duration_in_current_state(&self) -> u64 {
        current_timestamp().saturating_sub(self.state_start_time)
    }

    /// Get duration since degraded (seconds)
    pub fn duration_degraded(&self) -> Option<u64> {
        self.degraded_since
            .map(|since| current_timestamp().saturating_sub(since))
    }

    /// Get duration since unhealthy (seconds)
    pub fn duration_unhealthy(&self) -> Option<u64> {
        self.unhealthy_since
            .map(|since| current_timestamp().saturating_sub(since))
    }
}

/// High-level health state machine wrapper
#[derive(Debug)]
pub struct ExecutorHealthStateMachine {
    current_state: HealthState,
    context: HealthContext,
}

impl ExecutorHealthStateMachine {
    /// Create a new health state machine
    pub fn new(executor_id: Uuid) -> Self {
        let context = HealthContext::new(executor_id);

        debug!(
            executor_id = %executor_id,
            "Created new health state machine"
        );

        Self {
            current_state: HealthState::Starting,
            context,
        }
    }

    /// Get current state
    pub fn current_state(&self) -> &HealthState {
        &self.current_state
    }

    /// Get current context
    pub fn context(&self) -> &HealthContext {
        &self.context
    }

    /// Get mutable context
    pub fn context_mut(&mut self) -> &mut HealthContext {
        &mut self.context
    }

    /// Process a health event with manual state transitions
    pub fn process_event(&mut self, event: HealthEvent) -> TaskerResult<()> {
        debug!(
            executor_id = %self.context().executor_id,
            current_state = ?self.current_state(),
            event = ?event,
            "Processing health event"
        );

        // Determine the new state based on current state and event
        let new_state = match (&self.current_state, &event) {
            // Starting state transitions
            (HealthState::Starting, HealthEvent::StartupCompleted) => Some(HealthState::Healthy),
            (HealthState::Starting, HealthEvent::StartupFailed { .. }) => {
                Some(HealthState::Unhealthy)
            }
            (HealthState::Starting, HealthEvent::CriticalFailure { .. }) => {
                Some(HealthState::Unhealthy)
            }
            (HealthState::Starting, HealthEvent::GracefulShutdownInitiated { .. }) => {
                Some(HealthState::Stopping)
            }
            (HealthState::Starting, HealthEvent::ForcedShutdownInitiated { .. }) => {
                Some(HealthState::Stopping)
            }

            // Healthy state transitions
            (HealthState::Healthy, HealthEvent::PerformanceDegraded { .. }) => {
                if HealthGuards::is_significant_degradation(&self.context) {
                    Some(HealthState::Degraded)
                } else {
                    None // Not significant enough
                }
            }
            (HealthState::Healthy, HealthEvent::HeartbeatTimeout { .. }) => {
                if HealthGuards::is_critical_failure(&self.context) {
                    Some(HealthState::Unhealthy)
                } else {
                    None
                }
            }
            (HealthState::Healthy, HealthEvent::CriticalFailure { .. }) => {
                Some(HealthState::Unhealthy)
            }
            (HealthState::Healthy, HealthEvent::GracefulShutdownInitiated { .. }) => {
                if HealthGuards::can_shutdown_gracefully(&self.context) {
                    Some(HealthState::Stopping)
                } else {
                    None
                }
            }
            (HealthState::Healthy, HealthEvent::ForcedShutdownInitiated { .. }) => {
                Some(HealthState::Stopping)
            }

            // Degraded state transitions
            (HealthState::Degraded, HealthEvent::PerformanceRecovered) => {
                if HealthGuards::can_recover(&self.context) {
                    Some(HealthState::Healthy)
                } else {
                    None
                }
            }
            (HealthState::Degraded, HealthEvent::CriticalFailure { .. }) => {
                Some(HealthState::Unhealthy)
            }
            (HealthState::Degraded, HealthEvent::HeartbeatTimeout { .. }) => {
                if HealthGuards::is_critical_failure(&self.context) {
                    Some(HealthState::Unhealthy)
                } else {
                    None
                }
            }
            (HealthState::Degraded, HealthEvent::GracefulShutdownInitiated { .. }) => {
                if HealthGuards::can_shutdown_gracefully(&self.context) {
                    Some(HealthState::Stopping)
                } else {
                    None
                }
            }
            (HealthState::Degraded, HealthEvent::ForcedShutdownInitiated { .. }) => {
                Some(HealthState::Stopping)
            }

            // Unhealthy state transitions
            (HealthState::Unhealthy, HealthEvent::RecoveryAttempted) => {
                if HealthGuards::can_recover(&self.context) {
                    Some(HealthState::Starting)
                } else {
                    None
                }
            }
            (HealthState::Unhealthy, HealthEvent::GracefulShutdownInitiated { .. }) => {
                if HealthGuards::can_shutdown_gracefully(&self.context) {
                    Some(HealthState::Stopping)
                } else {
                    None
                }
            }
            (HealthState::Unhealthy, HealthEvent::ForcedShutdownInitiated { .. }) => {
                Some(HealthState::Stopping)
            }

            // Stopping state transitions
            (HealthState::Stopping, HealthEvent::ShutdownCompleted) => Some(HealthState::Stopped),

            // All other combinations are invalid
            _ => None,
        };

        // If a valid transition exists, perform it
        if let Some(target_state) = new_state {
            // Execute action based on the target state
            match &target_state {
                HealthState::Healthy => {
                    HealthActions::on_became_healthy(&mut self.context, &event);
                }
                HealthState::Degraded => {
                    HealthActions::on_became_degraded(&mut self.context, &event);
                }
                HealthState::Unhealthy => {
                    HealthActions::on_became_unhealthy(&mut self.context, &event);
                }
                HealthState::Stopping => {
                    HealthActions::on_shutdown_initiated(&mut self.context, &event);
                }
                HealthState::Stopped => {
                    HealthActions::on_shutdown_completed(&mut self.context, &event);
                }
                HealthState::Starting => {
                    HealthActions::on_startup_initiated(&mut self.context, &event);
                }
            }

            // Update the state
            self.current_state = target_state;

            debug!(
                executor_id = %self.context().executor_id,
                new_state = ?self.current_state(),
                "Health state transition completed"
            );

            Ok(())
        } else {
            Err(TaskerError::InvalidState(format!(
                "Invalid health event transition from {:?} with event {:?}",
                self.current_state(),
                event
            )))
        }
    }

    /// Update performance metrics and check for state transitions
    pub fn update_performance(&mut self, success_rate: f64, error_rate: f64) -> TaskerResult<()> {
        // Update context first
        self.context_mut()
            .update_performance(success_rate, error_rate);

        // Check for state transitions based on new metrics
        if error_rate > 0.1 && success_rate < 0.9 {
            self.process_event(HealthEvent::PerformanceDegraded {
                reason: format!("High error rate: {:.1}%", error_rate * 100.0),
                error_rate,
            })?;
        } else if matches!(self.current_state(), HealthState::Degraded) && success_rate > 0.9 {
            self.process_event(HealthEvent::PerformanceRecovered)?;
        }

        Ok(())
    }

    /// Update heartbeat and check for timeout
    pub fn update_heartbeat(&mut self, heartbeat_timeout_seconds: u64) -> TaskerResult<()> {
        let now = current_timestamp();
        let last_seen = self.context().last_seen;
        let calculated_timeout_exceeded = now.saturating_sub(last_seen) > heartbeat_timeout_seconds;

        // Check if timeout was already exceeded in context (for testing scenarios)
        let existing_timeout_exceeded = self.context().heartbeat_timeout_exceeded;

        // Timeout is exceeded if either it was already set or if we just calculated it
        let timeout_exceeded = existing_timeout_exceeded || calculated_timeout_exceeded;

        // Update context first
        self.context_mut().update_heartbeat(timeout_exceeded);

        // Check for heartbeat timeout and trigger state transition if needed
        if timeout_exceeded
            && !matches!(
                self.current_state(),
                HealthState::Unhealthy | HealthState::Stopping
            )
        {
            self.process_event(HealthEvent::HeartbeatTimeout { last_seen })?;
        }

        Ok(())
    }

    /// Check if executor is operational
    pub fn is_operational(&self) -> bool {
        matches!(
            self.current_state(),
            HealthState::Healthy | HealthState::Degraded
        )
    }

    /// Check if executor needs attention
    pub fn needs_attention(&self) -> bool {
        matches!(
            self.current_state(),
            HealthState::Degraded | HealthState::Unhealthy
        )
    }

    /// Get human-readable state description
    pub fn state_description(&self) -> String {
        let context = self.context();
        match self.current_state() {
            HealthState::Starting => {
                format!("Starting - {}", context.current_phase)
            }
            HealthState::Healthy => {
                format!(
                    "Healthy - Success rate: {:.1}%, Error rate: {:.1}%",
                    context.recent_success_rate * 100.0,
                    context.error_rate * 100.0
                )
            }
            HealthState::Degraded => {
                let reason = context
                    .degraded_reason
                    .as_deref()
                    .unwrap_or("Performance issues");
                format!(
                    "Degraded - {} (error rate: {:.1}%)",
                    reason,
                    context.error_rate * 100.0
                )
            }
            HealthState::Unhealthy => {
                let reason = context
                    .unhealthy_reason
                    .as_deref()
                    .unwrap_or("Critical failure");
                format!("Unhealthy - {reason}")
            }
            HealthState::Stopping => {
                let shutdown_type = if context.graceful_shutdown {
                    "graceful"
                } else {
                    "forced"
                };
                format!("Stopping ({}) - {}", shutdown_type, context.current_phase)
            }
            HealthState::Stopped => "Stopped".to_string(),
        }
    }
}

/// Get current Unix timestamp
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_state_machine_creation() {
        let executor_id = Uuid::new_v4();
        let sm = ExecutorHealthStateMachine::new(executor_id);

        assert_eq!(sm.current_state(), &HealthState::Starting);
        assert_eq!(sm.context().executor_id, executor_id);
    }

    #[test]
    fn test_startup_transitions() {
        let executor_id = Uuid::new_v4();
        let mut sm = ExecutorHealthStateMachine::new(executor_id);

        // Successful startup
        assert!(sm.process_event(HealthEvent::StartupCompleted).is_ok());
        assert_eq!(sm.current_state(), &HealthState::Healthy);
    }

    #[test]
    fn test_performance_degradation() {
        let executor_id = Uuid::new_v4();
        let mut sm = ExecutorHealthStateMachine::new(executor_id);

        // Start healthy
        sm.process_event(HealthEvent::StartupCompleted).unwrap();
        assert_eq!(sm.current_state(), &HealthState::Healthy);

        // Add enough observations for degradation to be significant
        sm.context_mut().total_observations = 10;

        // Simulate performance degradation
        sm.update_performance(0.7, 0.3).unwrap(); // High error rate
        assert_eq!(sm.current_state(), &HealthState::Degraded);

        // Add enough observations for recovery
        sm.context_mut().total_observations = 5;

        // Simulate recovery
        sm.update_performance(0.95, 0.05).unwrap(); // Good performance
        assert_eq!(sm.current_state(), &HealthState::Healthy);
    }

    #[test]
    fn test_heartbeat_timeout() {
        let executor_id = Uuid::new_v4();
        let mut sm = ExecutorHealthStateMachine::new(executor_id);

        // Start healthy
        sm.process_event(HealthEvent::StartupCompleted).unwrap();
        assert_eq!(sm.current_state(), &HealthState::Healthy);

        // Set up conditions for critical failure
        sm.context_mut().heartbeat_timeout_exceeded = true;

        // Simulate heartbeat timeout
        sm.update_heartbeat(1).unwrap(); // Very short timeout
        assert_eq!(sm.current_state(), &HealthState::Unhealthy);
    }

    #[test]
    fn test_shutdown_transitions() {
        let executor_id = Uuid::new_v4();
        let mut sm = ExecutorHealthStateMachine::new(executor_id);

        // Start healthy
        sm.process_event(HealthEvent::StartupCompleted).unwrap();

        // Graceful shutdown
        sm.process_event(HealthEvent::GracefulShutdownInitiated {
            phase: "cleanup".to_string(),
        })
        .unwrap();
        assert_eq!(sm.current_state(), &HealthState::Stopping);

        // Complete shutdown
        sm.process_event(HealthEvent::ShutdownCompleted).unwrap();
        assert_eq!(sm.current_state(), &HealthState::Stopped);
    }

    #[test]
    fn test_invalid_transitions() {
        let executor_id = Uuid::new_v4();
        let mut sm = ExecutorHealthStateMachine::new(executor_id);

        // Try to complete shutdown from Starting state (should fail)
        let result = sm.process_event(HealthEvent::ShutdownCompleted);
        assert!(result.is_err());
        assert_eq!(sm.current_state(), &HealthState::Starting); // State unchanged
    }

    #[test]
    fn test_operational_status() {
        let executor_id = Uuid::new_v4();
        let mut sm = ExecutorHealthStateMachine::new(executor_id);

        // Starting is not operational
        assert!(!sm.is_operational());

        // Healthy is operational
        sm.process_event(HealthEvent::StartupCompleted).unwrap();
        assert!(sm.is_operational());

        // Add enough observations for degradation
        sm.context_mut().total_observations = 10;

        // Degraded is operational but needs attention
        sm.update_performance(0.7, 0.3).unwrap();
        assert!(sm.is_operational());
        assert!(sm.needs_attention());

        // Unhealthy is not operational
        sm.process_event(HealthEvent::CriticalFailure {
            reason: "Test failure".to_string(),
        })
        .unwrap();
        assert!(!sm.is_operational());
        assert!(sm.needs_attention());
    }
}
