//! # Health Status Types
//!
//! TAS-75 Phase 5: Shared types for the health monitoring subsystem.
//!
//! These types are used throughout the health module for:
//! - Database health status
//! - Channel saturation status
//! - Queue depth monitoring
//! - Backpressure decisions

use std::collections::HashMap;

// =============================================================================
// Queue Depth Types (migrated from web/state.rs)
// =============================================================================

/// Queue depth tier classification
///
/// Used to categorize queue depth severity for backpressure decisions.
///
/// ## Unknown Variant
///
/// The `Unknown` variant is used when we cannot determine the actual queue depth:
/// - Lock contention prevents cache access
/// - Configuration disables queue depth checks
/// - Database query fails
///
/// This avoids false positives where "we don't know" gets interpreted as "looks clear".
/// Consuming systems should handle `Unknown` explicitly rather than treating it as healthy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QueueDepthTier {
    /// Unknown - could not determine queue depth (explicit "we don't know")
    #[default]
    Unknown = 0,
    /// Normal operation (below warning threshold)
    Normal = 1,
    /// Warning level (above warning, below critical)
    Warning = 2,
    /// Critical level (above critical, API returns 503)
    Critical = 3,
    /// Overflow level (above overflow threshold, emergency)
    Overflow = 4,
}

impl PartialOrd for QueueDepthTier {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueueDepthTier {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Unknown is treated as "no information" - it compares as less than Normal
        // This ensures that Unknown doesn't "win" when finding worst tier,
        // but also doesn't accidentally look like Normal
        (*self as u8).cmp(&(*other as u8))
    }
}

impl QueueDepthTier {
    /// Check if this tier represents an evaluated state (not Unknown)
    #[must_use]
    pub const fn is_evaluated(&self) -> bool {
        !matches!(self, Self::Unknown)
    }

    /// Check if this tier indicates a warning condition
    #[must_use]
    pub const fn is_warning(&self) -> bool {
        matches!(self, Self::Warning | Self::Critical | Self::Overflow)
    }

    /// Check if this tier indicates a critical condition (Critical or worse)
    #[must_use]
    pub const fn is_critical(&self) -> bool {
        matches!(self, Self::Critical | Self::Overflow)
    }

    /// Check if this tier indicates overflow condition
    #[must_use]
    pub const fn is_overflow(&self) -> bool {
        matches!(self, Self::Overflow)
    }
}

/// Queue depth status across all monitored queues
#[derive(Debug, Clone, Default)]
pub struct QueueDepthStatus {
    /// Worst tier across all queues
    pub tier: QueueDepthTier,
    /// Maximum depth across all queues
    pub max_depth: i64,
    /// Queue with worst depth
    pub worst_queue: String,
    /// Individual queue depths
    pub queue_depths: HashMap<String, i64>,
}

// =============================================================================
// Channel Health Types
// =============================================================================

/// Channel saturation status
///
/// ## Evaluated Field
///
/// The `evaluated` field indicates whether we actually checked the channel status:
/// - `true`: Status reflects real channel metrics
/// - `false`: Status is unknown (config disabled checks, lock contention, etc.)
///
/// This avoids false positives where "we don't know" gets interpreted as "looks clear".
#[derive(Debug, Clone)]
pub struct ChannelHealthStatus {
    /// Whether this status was actually evaluated (vs unknown/default)
    pub evaluated: bool,
    /// Command channel saturation percentage (0-100)
    pub command_saturation_percent: f64,
    /// Available capacity in command channel
    pub command_available_capacity: usize,
    /// Total messages sent through command channel
    pub command_messages_sent: u64,
    /// Total overflow events on command channel
    pub command_overflow_events: u64,
    /// Whether command channel is saturated (>80%)
    pub is_saturated: bool,
    /// Whether command channel is critical (>95%)
    pub is_critical: bool,
}

impl Default for ChannelHealthStatus {
    fn default() -> Self {
        Self {
            evaluated: false, // Default is unknown, not healthy
            command_saturation_percent: 0.0,
            command_available_capacity: 0,
            command_messages_sent: 0,
            command_overflow_events: 0,
            is_saturated: false,
            is_critical: false,
        }
    }
}

// =============================================================================
// Database Health Types
// =============================================================================

/// Database health status
///
/// ## Evaluated Field
///
/// The `evaluated` field indicates whether we actually checked the database status:
/// - `true`: Status reflects real database/circuit breaker state
/// - `false`: Status is unknown (config disabled checks, lock contention, etc.)
///
/// This avoids false positives where "we don't know" gets interpreted as "looks clear".
#[derive(Debug, Clone, Default)]
pub struct DatabaseHealthStatus {
    /// Whether this status was actually evaluated (vs unknown/default)
    pub evaluated: bool,
    /// Whether the database is connected and responsive
    pub is_connected: bool,
    /// Whether the circuit breaker is open
    pub circuit_breaker_open: bool,
    /// Current failure count on circuit breaker
    pub circuit_breaker_failures: u32,
    /// Last successful health check duration (milliseconds)
    pub last_check_duration_ms: u64,
    /// Error message if unhealthy
    pub error_message: Option<String>,
}

// =============================================================================
// Backpressure Types
// =============================================================================

/// Source of backpressure condition
#[derive(Debug, Clone)]
pub enum BackpressureSource {
    /// Circuit breaker is open
    CircuitBreaker,
    /// Channel saturation exceeds threshold
    ChannelSaturation {
        channel: String,
        saturation_percent: f64,
    },
    /// Queue depth exceeds threshold
    QueueDepth {
        queue: String,
        depth: i64,
        tier: QueueDepthTier,
    },
}

/// Pre-computed backpressure decision
///
/// This is computed by the StatusEvaluator and cached for fast access.
#[derive(Debug, Clone, Default)]
pub struct BackpressureStatus {
    /// Is backpressure currently active?
    pub active: bool,
    /// Reason for backpressure (if active)
    pub reason: Option<String>,
    /// Suggested retry-after seconds
    pub retry_after_secs: Option<u64>,
    /// Source of backpressure (if active)
    pub source: Option<BackpressureSource>,
}

/// Detailed backpressure metrics for health endpoint
///
/// Aggregates all health status information for monitoring and debugging.
#[derive(Debug, Clone)]
pub struct BackpressureMetrics {
    /// Whether the web database circuit breaker is open
    pub circuit_breaker_open: bool,
    /// Current failure count on circuit breaker
    pub circuit_breaker_failures: u32,
    /// Command channel saturation percentage (0-100)
    pub command_channel_saturation_percent: f64,
    /// Available slots in command channel
    pub command_channel_available_capacity: usize,
    /// Total messages sent through command channel
    pub command_channel_messages_sent: u64,
    /// Total overflow events on command channel
    pub command_channel_overflow_events: u64,
    /// Whether any backpressure condition is active
    pub backpressure_active: bool,
    /// Queue depth tier (Normal/Warning/Critical/Overflow)
    pub queue_depth_tier: String,
    /// Maximum queue depth across all queues
    pub queue_depth_max: i64,
    /// Queue with worst depth
    pub queue_depth_worst_queue: String,
    /// Individual queue depths
    pub queue_depths: HashMap<String, i64>,
}

impl Default for BackpressureMetrics {
    fn default() -> Self {
        Self {
            circuit_breaker_open: false,
            circuit_breaker_failures: 0,
            command_channel_saturation_percent: 0.0,
            command_channel_available_capacity: 0,
            command_channel_messages_sent: 0,
            command_channel_overflow_events: 0,
            backpressure_active: false,
            queue_depth_tier: "Normal".to_string(),
            queue_depth_max: 0,
            queue_depth_worst_queue: String::new(),
            queue_depths: HashMap::new(),
        }
    }
}

// =============================================================================
// Configuration Types
// =============================================================================

/// Health status evaluation configuration
///
/// Loaded from `orchestration.toml` [health] section.
#[derive(Debug, Clone)]
pub struct HealthConfig {
    /// Background evaluation interval (milliseconds)
    pub evaluation_interval_ms: u64,
    /// Enable database health checks
    pub check_database: bool,
    /// Enable channel health checks
    pub check_channels: bool,
    /// Enable queue depth checks
    pub check_queues: bool,
    /// Stale data threshold (milliseconds) - log warning if cache is older
    pub stale_threshold_ms: u64,
    /// Database health check configuration
    pub database: DatabaseHealthConfig,
    /// Channel health check configuration
    pub channels: ChannelHealthConfig,
    /// Queue health check configuration
    pub queues: QueueHealthConfig,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            evaluation_interval_ms: 5000, // 5 seconds
            check_database: true,
            check_channels: true,
            check_queues: true,
            stale_threshold_ms: 30000, // 30 seconds
            database: DatabaseHealthConfig::default(),
            channels: ChannelHealthConfig::default(),
            queues: QueueHealthConfig::default(),
        }
    }
}

/// Database health check configuration
#[derive(Debug, Clone)]
pub struct DatabaseHealthConfig {
    /// Query timeout (milliseconds)
    pub query_timeout_ms: u64,
}

impl Default for DatabaseHealthConfig {
    fn default() -> Self {
        Self {
            query_timeout_ms: 1000,
        }
    }
}

/// Channel health check configuration
#[derive(Debug, Clone)]
pub struct ChannelHealthConfig {
    /// Warning threshold (percent)
    pub warning_threshold: f64,
    /// Critical threshold (percent)
    pub critical_threshold: f64,
    /// Emergency threshold (percent)
    pub emergency_threshold: f64,
}

impl Default for ChannelHealthConfig {
    fn default() -> Self {
        Self {
            warning_threshold: 70.0,
            critical_threshold: 80.0,
            emergency_threshold: 95.0,
        }
    }
}

/// Queue health check configuration
#[derive(Debug, Clone)]
pub struct QueueHealthConfig {
    /// Warning threshold (message count)
    pub warning_threshold: i64,
    /// Critical threshold (message count)
    pub critical_threshold: i64,
    /// Overflow threshold (message count)
    pub overflow_threshold: i64,
}

impl Default for QueueHealthConfig {
    fn default() -> Self {
        Self {
            warning_threshold: 1000,
            critical_threshold: 5000,
            overflow_threshold: 10000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_depth_tier_ordering() {
        // Unknown is lowest (least severe) - it's "we don't know", not "looks bad"
        assert!(QueueDepthTier::Unknown < QueueDepthTier::Normal);
        assert!(QueueDepthTier::Normal < QueueDepthTier::Warning);
        assert!(QueueDepthTier::Warning < QueueDepthTier::Critical);
        assert!(QueueDepthTier::Critical < QueueDepthTier::Overflow);
    }

    #[test]
    fn test_queue_depth_tier_default() {
        // Default is now Unknown, not Normal - this is intentional
        // We default to "we don't know" rather than "looks clear"
        assert_eq!(QueueDepthTier::default(), QueueDepthTier::Unknown);
    }

    #[test]
    fn test_backpressure_status_default() {
        let status = BackpressureStatus::default();
        assert!(!status.active);
        assert!(status.reason.is_none());
        assert!(status.retry_after_secs.is_none());
        assert!(status.source.is_none());
    }

    #[test]
    fn test_health_config_default() {
        let config = HealthConfig::default();
        assert_eq!(config.evaluation_interval_ms, 5000);
        assert!(config.check_database);
        assert!(config.check_channels);
        assert!(config.check_queues);
        assert_eq!(config.channels.critical_threshold, 80.0);
    }

    #[test]
    fn test_database_health_status_default_is_unknown() {
        let status = DatabaseHealthStatus::default();
        // Default should be explicitly unevaluated (unknown)
        assert!(!status.evaluated);
        assert!(!status.is_connected);
        assert!(!status.circuit_breaker_open);
    }

    #[test]
    fn test_channel_health_status_default_is_unknown() {
        let status = ChannelHealthStatus::default();
        // Default should be explicitly unevaluated (unknown)
        assert!(!status.evaluated);
        assert!(!status.is_saturated);
        assert!(!status.is_critical);
    }

    #[test]
    fn test_queue_depth_status_default_is_unknown() {
        let status = QueueDepthStatus::default();
        // Default tier should be Unknown
        assert_eq!(status.tier, QueueDepthTier::Unknown);
    }
}
