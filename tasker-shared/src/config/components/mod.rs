// TAS-50: Reusable configuration components
//
// This module contains individual configuration components that are
// composed into the context-specific configurations.
//
// TAS-61 Phase 6C/6D cleanup: Removed unused component re-export wrappers.
// Only actively used components remain (backoff, database, dlq).

pub mod backoff;
pub mod database;
pub mod dlq;

// Re-export component types
pub use backoff::BackoffConfig;
pub use database::{DatabaseConfig, DatabasePoolConfig};
pub use dlq::{
    DlqConfig, DlqReasons, StalenessActions, StalenessDetectionConfig, StalenessThresholds,
};
