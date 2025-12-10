//! # Health Module
//!
//! TAS-75 Phase 5: Centralized health monitoring and backpressure management.
//!
//! This module provides a cache-first architecture for health status monitoring,
//! replacing the real-time interrogation pattern in `web/state.rs`.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                    HEALTH MODULE ARCHITECTURE                       │
//! └─────────────────────────────────────────────────────────────────────┘
//!
//!   ┌─────────────────────┐
//!   │  StatusEvaluator    │ (Background Task)
//!   │  - DB health check  │
//!   │  - Channel check    │
//!   │  - Queue depth      │
//!   │  - Backpressure     │
//!   └──────────┬──────────┘
//!              │ writes at interval
//!              ▼
//!   ┌─────────────────────┐
//!   │  HealthStatusCaches │ (Thread-safe)
//!   │  - db_status        │
//!   │  - channel_status   │
//!   │  - queue_status     │
//!   │  - backpressure     │
//!   └──────────┬──────────┘
//!              │ reads (non-blocking)
//!              ▼
//!   ┌─────────────────────┐
//!   │  BackpressureChecker│ (Public API)
//!   │  - try_check_*()    │
//!   │  - get_*_status()   │
//!   └─────────────────────┘
//! ```
//!
//! ## Benefits
//!
//! 1. **Performance**: No database queries in API hot path
//! 2. **Reliability**: Health checks isolated from request handling
//! 3. **Observability**: Clear metrics for evaluation frequency
//! 4. **Testability**: Cache-based design easier to test
//! 5. **Configurability**: Evaluation interval tunable per environment
//!
//! ## Usage
//!
//! ```ignore
//! use tasker_orchestration::health::{HealthStatusCaches, BackpressureChecker};
//!
//! // During bootstrap
//! let caches = HealthStatusCaches::new();
//! let checker = BackpressureChecker::new(caches.clone());
//!
//! // In API handlers (synchronous check)
//! if let Some(error) = checker.try_check_backpressure() {
//!     return Err(error);
//! }
//!
//! // Get detailed status for health endpoint
//! let metrics = checker.get_backpressure_metrics().await;
//! ```

pub mod backpressure;
pub mod caches;
pub mod channel_status;
pub mod db_status;
pub mod queue_status;
pub mod status_evaluator;
pub mod types;

// Re-export primary types for convenience
pub use backpressure::BackpressureChecker;
pub use caches::HealthStatusCaches;
pub use status_evaluator::StatusEvaluator;
pub use types::{
    BackpressureMetrics, BackpressureSource, BackpressureStatus, ChannelHealthConfig,
    ChannelHealthStatus, DatabaseHealthConfig, DatabaseHealthStatus, HealthConfig,
    QueueDepthStatus, QueueDepthTier, QueueHealthConfig,
};
