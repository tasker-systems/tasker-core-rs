// TAS-50: Circuit breakers configuration component
//
// Phase 1: Re-export existing types for backward compatibility

pub use crate::config::circuit_breaker::{
    CircuitBreakerComponentConfig, CircuitBreakerConfig as CircuitBreakersConfig,
    CircuitBreakerGlobalSettings,
};
