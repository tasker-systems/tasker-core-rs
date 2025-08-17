//! # Integration Tests for TAS-37 Supplemental Shutdown-Aware Health Monitoring
//!
//! This module contains comprehensive integration tests for the shutdown-aware health monitoring
//! enhancement implemented as part of TAS-37 supplemental.
//!
//! ## Test Coverage
//!
//! - Operational state transitions and validation
//! - Health monitoring behavior during different operational states
//! - Configuration-aware threshold multiplier application
//! - Alert suppression during graceful shutdown
//! - Emergency shutdown behavior
//! - Integration between coordinator, operational state, and health monitor

use std::time::Duration;
use tokio::time::sleep;

use crate::config::OperationalStateConfig;
use crate::orchestration::coordinator::{
    monitor::HealthMonitor,
    operational_state::{OperationalStateManager, SystemOperationalState},
    pool::{HealthReport, PoolStatus},
};
use crate::orchestration::executor::traits::ExecutorType;
use std::collections::HashMap;

/// Test that operational state transitions follow expected lifecycle
#[tokio::test]
async fn test_operational_state_lifecycle() {
    let state_manager = OperationalStateManager::new();
    
    // Should start in Startup state
    assert_eq!(state_manager.current_state().await, SystemOperationalState::Startup);
    
    // Transition to Normal operation
    state_manager.transition_to(SystemOperationalState::Normal).await.unwrap();
    assert_eq!(state_manager.current_state().await, SystemOperationalState::Normal);
    
    // Transition to GracefulShutdown
    state_manager.transition_to(SystemOperationalState::GracefulShutdown).await.unwrap();
    assert_eq!(state_manager.current_state().await, SystemOperationalState::GracefulShutdown);
    
    // Transition to Stopped
    state_manager.transition_to(SystemOperationalState::Stopped).await.unwrap();
    assert_eq!(state_manager.current_state().await, SystemOperationalState::Stopped);
    
    // Can restart from Stopped to Startup
    state_manager.transition_to(SystemOperationalState::Startup).await.unwrap();
    assert_eq!(state_manager.current_state().await, SystemOperationalState::Startup);
}

/// Test that emergency transitions are always allowed
#[tokio::test]
async fn test_emergency_state_transitions() {
    let state_manager = OperationalStateManager::new();
    
    // Emergency transition from any state should work
    state_manager.transition_to(SystemOperationalState::Normal).await.unwrap();
    state_manager.transition_to(SystemOperationalState::Emergency).await.unwrap();
    assert_eq!(state_manager.current_state().await, SystemOperationalState::Emergency);
    
    // Can only go to Stopped from Emergency
    let result = state_manager.transition_to(SystemOperationalState::Normal).await;
    assert!(result.is_err()); // Should fail
    
    state_manager.transition_to(SystemOperationalState::Stopped).await.unwrap();
    assert_eq!(state_manager.current_state().await, SystemOperationalState::Stopped);
}

/// Test configuration-aware health threshold multipliers
#[tokio::test]
async fn test_configuration_aware_health_thresholds() {
    let state_manager = OperationalStateManager::new();
    
    // Create custom configuration with specific multipliers
    let config = OperationalStateConfig {
        enable_shutdown_aware_monitoring: true,
        suppress_alerts_during_shutdown: true,
        startup_health_threshold_multiplier: 0.3, // Very relaxed during startup
        shutdown_health_threshold_multiplier: 0.1, // Almost no requirements during shutdown
        graceful_shutdown_timeout_seconds: 45,
        emergency_shutdown_timeout_seconds: 10,
        enable_transition_logging: true,
        transition_log_level: "INFO".to_string(),
    };
    
    // Test startup state with custom config
    assert_eq!(state_manager.current_state().await, SystemOperationalState::Startup);
    let multiplier = state_manager.health_threshold_multiplier_with_config(&config).await;
    assert_eq!(multiplier, 0.3);
    
    // Test normal state
    state_manager.transition_to(SystemOperationalState::Normal).await.unwrap();
    let multiplier = state_manager.health_threshold_multiplier_with_config(&config).await;
    assert_eq!(multiplier, 1.0);
    
    // Test graceful shutdown state with custom config
    state_manager.transition_to(SystemOperationalState::GracefulShutdown).await.unwrap();
    let multiplier = state_manager.health_threshold_multiplier_with_config(&config).await;
    assert_eq!(multiplier, 0.1);
    
    // Test emergency state (should always be 0.0)
    state_manager.transition_to(SystemOperationalState::Emergency).await.unwrap();
    let multiplier = state_manager.health_threshold_multiplier_with_config(&config).await;
    assert_eq!(multiplier, 0.0);
}

/// Test health monitoring behavior during different operational states
#[tokio::test]
async fn test_health_monitoring_operational_state_awareness() {
    let state_manager = OperationalStateManager::new();
    let monitor = HealthMonitor::new(uuid::Uuid::new_v4(), 10, 0.8);
    
    // Create a test health report
    let health_report = create_test_health_report(5, 10); // 50% healthy
    
    // Test during startup - should monitor but not alert aggressively
    assert_eq!(state_manager.current_state().await, SystemOperationalState::Startup);
    assert!(state_manager.should_monitor_health().await);
    assert!(!state_manager.should_suppress_alerts().await);
    
    // Record health report during startup
    monitor.record_health_report_with_operational_state(health_report.clone(), Some(&state_manager)).await.unwrap();
    
    // Transition to normal operation
    state_manager.transition_to(SystemOperationalState::Normal).await.unwrap();
    assert!(state_manager.should_monitor_health().await);
    assert!(!state_manager.should_suppress_alerts().await);
    
    // Record health report during normal operation
    monitor.record_health_report_with_operational_state(health_report.clone(), Some(&state_manager)).await.unwrap();
    
    // Transition to graceful shutdown
    state_manager.transition_to(SystemOperationalState::GracefulShutdown).await.unwrap();
    assert!(!state_manager.should_monitor_health().await);
    assert!(state_manager.should_suppress_alerts().await);
    
    // Record health report during graceful shutdown
    monitor.record_health_report_with_operational_state(health_report.clone(), Some(&state_manager)).await.unwrap();
    
    // Transition to stopped
    state_manager.transition_to(SystemOperationalState::Stopped).await.unwrap();
    assert!(!state_manager.should_monitor_health().await);
    assert!(state_manager.should_suppress_alerts().await);
}

/// Test complete configuration integration with health monitoring
#[tokio::test]
async fn test_complete_configuration_integration() {
    let state_manager = OperationalStateManager::new();
    let monitor = HealthMonitor::new(uuid::Uuid::new_v4(), 10, 0.8);
    
    // Create custom operational state configuration
    let config = OperationalStateConfig {
        enable_shutdown_aware_monitoring: true,
        suppress_alerts_during_shutdown: true,
        startup_health_threshold_multiplier: 0.4,
        shutdown_health_threshold_multiplier: 0.2,
        graceful_shutdown_timeout_seconds: 60,
        emergency_shutdown_timeout_seconds: 15,
        enable_transition_logging: true,
        transition_log_level: "WARN".to_string(),
    };
    
    // Validate configuration
    config.validate().unwrap();
    
    // Test health report with configuration
    let health_report = create_test_health_report(6, 10); // 60% healthy
    
    // Test startup state with configuration
    assert_eq!(state_manager.current_state().await, SystemOperationalState::Startup);
    monitor.record_health_report_with_config(
        health_report.clone(), 
        Some(&state_manager), 
        Some(&config)
    ).await.unwrap();
    
    // Transition to normal and test
    state_manager.transition_to(SystemOperationalState::Normal).await.unwrap();
    monitor.record_health_report_with_config(
        health_report.clone(), 
        Some(&state_manager), 
        Some(&config)
    ).await.unwrap();
    
    // Transition to graceful shutdown and test
    state_manager.transition_to(SystemOperationalState::GracefulShutdown).await.unwrap();
    monitor.record_health_report_with_config(
        health_report.clone(), 
        Some(&state_manager), 
        Some(&config)
    ).await.unwrap();
    
    // Verify health history was recorded
    let history = monitor.get_health_history().await;
    assert_eq!(history.len(), 3);
}

/// Test comprehensive shutdown simulation with health monitoring
#[tokio::test]
async fn test_comprehensive_shutdown_simulation() {
    let state_manager = OperationalStateManager::new();
    let monitor = HealthMonitor::new(uuid::Uuid::new_v4(), 5, 0.8);
    let config = OperationalStateConfig::default();
    
    // Simulate a complete system lifecycle with health monitoring
    let simulation_scenarios = vec![
        // Startup phase - system coming online
        (SystemOperationalState::Startup, create_test_health_report(3, 10)), // 30% - low during startup
        (SystemOperationalState::Startup, create_test_health_report(6, 10)), // 60% - improving
        (SystemOperationalState::Startup, create_test_health_report(8, 10)), // 80% - ready for normal
        
        // Normal operation - healthy system
        (SystemOperationalState::Normal, create_test_health_report(9, 10)), // 90% - healthy
        (SystemOperationalState::Normal, create_test_health_report(10, 10)), // 100% - perfect
        (SystemOperationalState::Normal, create_test_health_report(8, 10)),  // 80% - still good
        
        // Graceful shutdown - degrading performance expected
        (SystemOperationalState::GracefulShutdown, create_test_health_report(6, 10)), // 60% - degrading
        (SystemOperationalState::GracefulShutdown, create_test_health_report(4, 10)), // 40% - more degraded
        (SystemOperationalState::GracefulShutdown, create_test_health_report(2, 10)), // 20% - mostly down
        
        // System stopped
        (SystemOperationalState::Stopped, create_test_health_report(0, 10)), // 0% - fully stopped
    ];
    
    for (target_state, health_report) in simulation_scenarios {
        // Transition to target state (if valid)
        if state_manager.current_state().await.can_transition_to(&target_state) {
            state_manager.transition_to(target_state.clone()).await.unwrap();
        }
        
        // Record health report with operational state context
        monitor.record_health_report_with_config(
            health_report,
            Some(&state_manager),
            Some(&config)
        ).await.unwrap();
        
        // Verify state-specific behavior
        match target_state {
            SystemOperationalState::Startup => {
                assert!(state_manager.should_monitor_health().await);
                assert!(!state_manager.should_suppress_alerts().await);
                assert_eq!(state_manager.health_threshold_multiplier_with_config(&config).await, 0.5);
            },
            SystemOperationalState::Normal => {
                assert!(state_manager.should_monitor_health().await);
                assert!(!state_manager.should_suppress_alerts().await);
                assert_eq!(state_manager.health_threshold_multiplier_with_config(&config).await, 1.0);
            },
            SystemOperationalState::GracefulShutdown => {
                assert!(!state_manager.should_monitor_health().await);
                assert!(state_manager.should_suppress_alerts().await);
                assert_eq!(state_manager.health_threshold_multiplier_with_config(&config).await, 0.0);
            },
            SystemOperationalState::Stopped => {
                assert!(!state_manager.should_monitor_health().await);
                assert!(state_manager.should_suppress_alerts().await);
                assert_eq!(state_manager.health_threshold_multiplier_with_config(&config).await, 0.0);
            },
            SystemOperationalState::Emergency => {
                assert!(state_manager.should_monitor_health().await);
                assert!(!state_manager.should_suppress_alerts().await);
                assert_eq!(state_manager.health_threshold_multiplier_with_config(&config).await, 0.0);
            },
        }
        
        // Small delay to ensure different timestamps
        sleep(Duration::from_millis(10)).await;
    }
    
    // Verify complete health history was recorded
    let history = monitor.get_health_history().await;
    assert_eq!(history.len(), 10);
    
    // Verify health summary computation
    let summary = monitor.get_health_summary(10).await;
    assert_eq!(summary.report_count, 10);
    assert!(summary.time_span_seconds > 0.0);
}

/// Test edge cases in operational state transitions during health monitoring
#[tokio::test]
async fn test_edge_case_state_transitions() {
    let state_manager = OperationalStateManager::new();
    let monitor = HealthMonitor::new(uuid::Uuid::new_v4(), 5, 0.8);
    let config = OperationalStateConfig::default();
    
    // Test edge case: Emergency transition from any state
    state_manager.transition_to(SystemOperationalState::Normal).await.unwrap();
    
    // Record health during normal operation
    let normal_report = create_test_health_report(9, 10);
    monitor.record_health_report_with_config(
        normal_report, 
        Some(&state_manager), 
        Some(&config)
    ).await.unwrap();
    
    // Emergency transition should always be allowed
    state_manager.transition_to(SystemOperationalState::Emergency).await.unwrap();
    assert_eq!(state_manager.current_state().await, SystemOperationalState::Emergency);
    
    // Record health during emergency (should still monitor but with emergency context)
    let emergency_report = create_test_health_report(3, 10); // 30% - very degraded
    monitor.record_health_report_with_config(
        emergency_report, 
        Some(&state_manager), 
        Some(&config)
    ).await.unwrap();
    
    // Verify emergency state behavior
    assert!(state_manager.should_monitor_health().await);
    assert!(!state_manager.should_suppress_alerts().await); // Emergency should not suppress alerts
    assert_eq!(state_manager.health_threshold_multiplier_with_config(&config).await, 0.0);
    
    // From emergency, can only go to stopped
    assert!(state_manager.current_state().await.can_transition_to(&SystemOperationalState::Stopped));
    assert!(!state_manager.current_state().await.can_transition_to(&SystemOperationalState::Normal));
    assert!(!state_manager.current_state().await.can_transition_to(&SystemOperationalState::GracefulShutdown));
    
    // Complete transition to stopped
    state_manager.transition_to(SystemOperationalState::Stopped).await.unwrap();
    
    // Record final health report (should be suppressed)
    let stopped_report = create_test_health_report(0, 10);
    monitor.record_health_report_with_config(
        stopped_report, 
        Some(&state_manager), 
        Some(&config)
    ).await.unwrap();
    
    // Verify stopped state behavior  
    assert!(!state_manager.should_monitor_health().await);
    assert!(state_manager.should_suppress_alerts().await);
    
    // Verify restart capability
    assert!(state_manager.current_state().await.can_transition_to(&SystemOperationalState::Startup));
    state_manager.transition_to(SystemOperationalState::Startup).await.unwrap();
    assert_eq!(state_manager.current_state().await, SystemOperationalState::Startup);
    
    // Verify all health reports were recorded
    let history = monitor.get_health_history().await;
    assert_eq!(history.len(), 4);
}

/// Test concurrent health monitoring with state transitions
#[tokio::test]
async fn test_concurrent_health_monitoring_with_state_transitions() {
    let state_manager = OperationalStateManager::new();
    let monitor = HealthMonitor::new(uuid::Uuid::new_v4(), 2, 0.8);
    let config = OperationalStateConfig::default();
    
    // Create multiple concurrent tasks that transition states and record health
    let mut tasks = Vec::new();
    
    // Task 1: Health monitoring loop
    let monitor_clone = monitor.clone();
    let state_manager_clone = state_manager.clone();
    let config_clone = config.clone();
    let health_task = tokio::spawn(async move {
        for i in 1..=10 {
            let health_percentage = 50 + (i * 5); // Gradually improving health
            let healthy_executors = health_percentage / 10;
            let report = create_test_health_report(healthy_executors, 10);
            
            monitor_clone.record_health_report_with_config(
                report,
                Some(&state_manager_clone),
                Some(&config_clone)
            ).await.unwrap();
            
            sleep(Duration::from_millis(20)).await;
        }
    });
    tasks.push(health_task);
    
    // Task 2: State transition loop
    let state_manager_clone = state_manager.clone();
    let transition_task = tokio::spawn(async move {
        sleep(Duration::from_millis(50)).await; // Let health monitoring start
        
        // Transition through lifecycle
        state_manager_clone.transition_to(SystemOperationalState::Normal).await.unwrap();
        sleep(Duration::from_millis(80)).await;
        
        state_manager_clone.transition_to(SystemOperationalState::GracefulShutdown).await.unwrap();
        sleep(Duration::from_millis(80)).await;
        
        state_manager_clone.transition_to(SystemOperationalState::Stopped).await.unwrap();
    });
    tasks.push(transition_task);
    
    // Wait for all tasks to complete
    for task in tasks {
        task.await.unwrap();
    }
    
    // Verify final state
    assert_eq!(state_manager.current_state().await, SystemOperationalState::Stopped);
    
    // Verify health history was recorded throughout transitions
    let history = monitor.get_health_history().await;
    assert_eq!(history.len(), 10);
    
    // Verify health summary
    let summary = monitor.get_health_summary(10).await;
    assert_eq!(summary.report_count, 10);
    assert!(summary.average_health_percentage > 50.0); // Should show gradual improvement
}

/// Test configuration edge cases and validation
#[tokio::test]
async fn test_operational_state_config_edge_cases() {
    // Test configuration with extreme values
    let extreme_config = OperationalStateConfig {
        enable_shutdown_aware_monitoring: true,
        suppress_alerts_during_shutdown: false, // Don't suppress alerts
        startup_health_threshold_multiplier: 0.1, // Very relaxed startup
        shutdown_health_threshold_multiplier: 0.9, // Very strict shutdown
        graceful_shutdown_timeout_seconds: 1, // Very short timeout
        emergency_shutdown_timeout_seconds: 1, // Very short timeout
        enable_transition_logging: false,
        transition_log_level: "ERROR".to_string(),
    };
    
    // Should still validate successfully
    extreme_config.validate().unwrap();
    
    // Test with the extreme configuration
    let state_manager = OperationalStateManager::new();
    let monitor = HealthMonitor::new(uuid::Uuid::new_v4(), 5, 0.8);
    
    // Test startup with very relaxed thresholds
    let startup_report = create_test_health_report(1, 10); // Only 10% healthy
    monitor.record_health_report_with_config(
        startup_report,
        Some(&state_manager),
        Some(&extreme_config)
    ).await.unwrap();
    
    let startup_multiplier = state_manager.health_threshold_multiplier_with_config(&extreme_config).await;
    assert_eq!(startup_multiplier, 0.1);
    
    // Transition to graceful shutdown
    state_manager.transition_to(SystemOperationalState::Normal).await.unwrap();
    state_manager.transition_to(SystemOperationalState::GracefulShutdown).await.unwrap();
    
    // Test shutdown with strict thresholds (but alerts not suppressed in this config)
    let shutdown_report = create_test_health_report(8, 10); // 80% healthy
    monitor.record_health_report_with_config(
        shutdown_report,
        Some(&state_manager),
        Some(&extreme_config)
    ).await.unwrap();
    
    let shutdown_multiplier = state_manager.health_threshold_multiplier_with_config(&extreme_config).await;
    assert_eq!(shutdown_multiplier, 0.9);
    
    // Verify alerts are not suppressed (different from default behavior)
    assert!(!state_manager.should_suppress_alerts().await);
    
    // Test timeout conversions
    assert_eq!(extreme_config.graceful_shutdown_timeout(), Duration::from_secs(1));
    assert_eq!(extreme_config.emergency_shutdown_timeout(), Duration::from_secs(1));
}

/// Test configuration timeout values
#[tokio::test]
async fn test_configuration_timeout_values() {
    let config = OperationalStateConfig {
        enable_shutdown_aware_monitoring: true,
        suppress_alerts_during_shutdown: true,
        startup_health_threshold_multiplier: 0.5,
        shutdown_health_threshold_multiplier: 0.0,
        graceful_shutdown_timeout_seconds: 30,
        emergency_shutdown_timeout_seconds: 5,
        enable_transition_logging: true,
        transition_log_level: "INFO".to_string(),
    };
    
    // Test timeout conversion methods
    assert_eq!(config.graceful_shutdown_timeout(), Duration::from_secs(30));
    assert_eq!(config.emergency_shutdown_timeout(), Duration::from_secs(5));
    
    // Test validation
    config.validate().unwrap();
}

/// Test invalid configuration validation
#[tokio::test]
async fn test_invalid_configuration_validation() {
    // Test invalid startup multiplier (>1.0)
    let mut config = OperationalStateConfig::default();
    config.startup_health_threshold_multiplier = 1.5;
    assert!(config.validate().is_err());
    
    // Test invalid shutdown multiplier (<0.0)
    let mut config = OperationalStateConfig::default();
    config.shutdown_health_threshold_multiplier = -0.1;
    assert!(config.validate().is_err());
    
    // Test zero timeout
    let mut config = OperationalStateConfig::default();
    config.graceful_shutdown_timeout_seconds = 0;
    assert!(config.validate().is_err());
    
    // Test invalid log level
    let mut config = OperationalStateConfig::default();
    config.transition_log_level = "INVALID".to_string();
    assert!(config.validate().is_err());
}

/// Test default configuration values
#[tokio::test]
async fn test_default_configuration_values() {
    let config = OperationalStateConfig::default();
    
    // Verify defaults
    assert!(config.enable_shutdown_aware_monitoring);
    assert!(config.suppress_alerts_during_shutdown);
    assert_eq!(config.startup_health_threshold_multiplier, 0.5);
    assert_eq!(config.shutdown_health_threshold_multiplier, 0.0);
    assert_eq!(config.graceful_shutdown_timeout_seconds, 30);
    assert_eq!(config.emergency_shutdown_timeout_seconds, 5);
    assert!(config.enable_transition_logging);
    assert_eq!(config.transition_log_level, "INFO");
    
    // Should validate successfully
    config.validate().unwrap();
}

/// Test health monitoring during state transitions
#[tokio::test]
async fn test_health_monitoring_during_transitions() {
    let state_manager = OperationalStateManager::new();
    let monitor = HealthMonitor::new(uuid::Uuid::new_v4(), 5, 0.8);
    let config = OperationalStateConfig::default();
    
    // Create health reports with different health levels
    let healthy_report = create_test_health_report(9, 10); // 90% healthy
    let degraded_report = create_test_health_report(6, 10); // 60% healthy
    let unhealthy_report = create_test_health_report(3, 10); // 30% healthy
    
    // Test startup state with degraded health (should be more tolerant)
    assert_eq!(state_manager.current_state().await, SystemOperationalState::Startup);
    monitor.record_health_report_with_config(
        degraded_report.clone(), 
        Some(&state_manager), 
        Some(&config)
    ).await.unwrap();
    
    // Transition to normal operation
    state_manager.transition_to(SystemOperationalState::Normal).await.unwrap();
    
    // Test normal operation with healthy report
    monitor.record_health_report_with_config(
        healthy_report.clone(), 
        Some(&state_manager), 
        Some(&config)
    ).await.unwrap();
    
    // Test normal operation with unhealthy report (should alert)
    monitor.record_health_report_with_config(
        unhealthy_report.clone(), 
        Some(&state_manager), 
        Some(&config)
    ).await.unwrap();
    
    // Transition to graceful shutdown
    state_manager.transition_to(SystemOperationalState::GracefulShutdown).await.unwrap();
    
    // Test graceful shutdown with unhealthy report (should not alert)
    monitor.record_health_report_with_config(
        unhealthy_report.clone(), 
        Some(&state_manager), 
        Some(&config)
    ).await.unwrap();
    
    // Verify all reports were recorded
    let history = monitor.get_health_history().await;
    assert_eq!(history.len(), 4);
}

/// Helper function to create test health reports
fn create_test_health_report(healthy_executors: usize, total_executors: usize) -> HealthReport {
    let mut pool_statuses = HashMap::new();
    
    pool_statuses.insert(
        ExecutorType::TaskRequestProcessor,
        PoolStatus {
            executor_type: ExecutorType::TaskRequestProcessor,
            active_executors: total_executors,
            healthy_executors,
            unhealthy_executors: total_executors - healthy_executors,
            min_executors: 1,
            max_executors: 15,
            total_items_processed: 1000,
            total_items_failed: 50,
            average_utilization: 0.7,
            uptime_seconds: 3600,
        },
    );
    
    HealthReport {
        total_pools: 1,
        total_executors,
        healthy_executors,
        unhealthy_executors: total_executors - healthy_executors,
        pool_statuses,
    }
}

/// Test health summary generation with operational state context
#[tokio::test]
async fn test_health_summary_with_operational_context() {
    let state_manager = OperationalStateManager::new();
    let monitor = HealthMonitor::new(uuid::Uuid::new_v4(), 5, 0.8);
    let config = OperationalStateConfig::default();
    
    // Record several health reports across different operational states
    let reports = vec![
        (create_test_health_report(10, 10), SystemOperationalState::Startup),
        (create_test_health_report(9, 10), SystemOperationalState::Normal),
        (create_test_health_report(8, 10), SystemOperationalState::Normal),
        (create_test_health_report(5, 10), SystemOperationalState::GracefulShutdown),
        (create_test_health_report(2, 10), SystemOperationalState::GracefulShutdown),
    ];
    
    for (report, state) in reports {
        state_manager.transition_to(state).await.unwrap();
        monitor.record_health_report_with_config(
            report, 
            Some(&state_manager), 
            Some(&config)
        ).await.unwrap();
        
        // Small delay to ensure different timestamps
        sleep(Duration::from_millis(10)).await;
    }
    
    // Get health summary
    let summary = monitor.get_health_summary(5).await;
    assert_eq!(summary.report_count, 5);
    assert!(summary.time_span_seconds > 0.0);
    
    // Average should be (100 + 90 + 80 + 50 + 20) / 5 = 68%
    assert!((summary.average_health_percentage - 68.0).abs() < 1.0);
}

#[cfg(test)]
mod operational_state_integration_tests {
    use super::*;
    
    /// Test that operational state properties are consistent
    #[test]
    fn test_operational_state_properties_consistency() {
        let states = vec![
            SystemOperationalState::Normal,
            SystemOperationalState::Startup,
            SystemOperationalState::GracefulShutdown,
            SystemOperationalState::Emergency,
            SystemOperationalState::Stopped,
        ];
        
        for state in states {
            // Test that shutdown states behave consistently
            if state.is_shutdown() {
                assert!(matches!(state, 
                    SystemOperationalState::GracefulShutdown | 
                    SystemOperationalState::Emergency | 
                    SystemOperationalState::Stopped
                ));
            }
            
            // Test that transitional states behave consistently
            if state.is_transitional() {
                assert!(matches!(state, 
                    SystemOperationalState::Startup | 
                    SystemOperationalState::GracefulShutdown
                ));
            }
            
            // Test that health log levels are valid
            let log_level = state.health_log_level();
            assert!(matches!(log_level, "DEBUG" | "INFO" | "WARN" | "ERROR"));
            
            // Test that descriptions are not empty
            assert!(!state.description().is_empty());
        }
    }
}