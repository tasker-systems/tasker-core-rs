//! Type conversion utilities for TypeScript FFI
//!
//! Provides conversion between Rust types and JSON for TypeScript consumption.
//! Uses explicit DTO types for compile-time guarantees on the FFI contract.

use anyhow::Result;

use crate::dto::{FfiDispatchMetricsDto, FfiStepEventDto};
use tasker_worker::worker::{FfiDispatchMetrics, FfiStepEvent};

/// Convert an FfiStepEvent to a JSON string for TypeScript consumption.
///
/// Uses explicit DTO conversion for compile-time field mapping guarantees.
/// If the underlying Rust types change, this will fail to compile rather
/// than silently producing incorrect JSON.
pub fn convert_ffi_step_event_to_json(event: &FfiStepEvent) -> Result<String> {
    let dto = FfiStepEventDto::from(event);
    Ok(serde_json::to_string(&dto)?)
}

/// Convert FfiDispatchMetrics to a JSON string for TypeScript consumption.
pub fn convert_ffi_dispatch_metrics_to_json(metrics: &FfiDispatchMetrics) -> Result<String> {
    let dto = FfiDispatchMetricsDto::from(metrics);
    Ok(serde_json::to_string(&dto)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_conversion() {
        let metrics = FfiDispatchMetrics {
            pending_count: 10,
            starvation_detected: true,
            starving_event_count: 3,
            oldest_pending_age_ms: Some(5000),
            newest_pending_age_ms: Some(100),
            oldest_event_id: None,
        };

        let json = convert_ffi_dispatch_metrics_to_json(&metrics).unwrap();

        assert!(json.contains("\"pending_count\":10"));
        assert!(json.contains("\"starvation_detected\":true"));
        assert!(json.contains("\"starving_event_count\":3"));
    }
}
