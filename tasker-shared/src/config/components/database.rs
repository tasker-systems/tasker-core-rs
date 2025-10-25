// TAS-50: Database configuration component
//
// Phase 1: Re-export existing types for backward compatibility
// Future phases may refactor the actual implementation here

// Re-export from existing tasker.rs for now
pub use crate::config::tasker::{DatabaseConfig, DatabasePoolConfig, DatabaseVariables};
