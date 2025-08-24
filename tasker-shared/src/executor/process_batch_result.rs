use serde::{Deserialize, Serialize};

/// Result of processing a batch of items
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessBatchResult {
    /// Number of items processed successfully
    pub processed_count: usize,
    /// Number of items that failed processing
    pub failed_count: usize,
    /// Number of items that were skipped
    pub skipped_count: usize,
    /// Total processing time in milliseconds
    pub processing_time_ms: u64,
    /// Average processing time per item in milliseconds
    pub avg_processing_time_ms: f64,
    /// Any warnings or non-fatal errors encountered
    pub warnings: Vec<String>,
    /// Whether more items are available for processing
    pub has_more_items: bool,
}

impl ProcessBatchResult {
    /// Create a new empty result
    pub fn empty() -> Self {
        Self {
            processed_count: 0,
            failed_count: 0,
            skipped_count: 0,
            processing_time_ms: 0,
            avg_processing_time_ms: 0.0,
            warnings: Vec::new(),
            has_more_items: false,
        }
    }

    /// Create a result with processed items
    pub fn processed(count: usize, processing_time_ms: u64) -> Self {
        let avg_time = if count > 0 {
            processing_time_ms as f64 / count as f64
        } else {
            0.0
        };

        Self {
            processed_count: count,
            failed_count: 0,
            skipped_count: 0,
            processing_time_ms,
            avg_processing_time_ms: avg_time,
            warnings: Vec::new(),
            has_more_items: false,
        }
    }

    /// Get total items processed (success + failed + skipped)
    pub fn total_items(&self) -> usize {
        self.processed_count + self.failed_count + self.skipped_count
    }

    /// Get success rate as a percentage (0.0 - 1.0)
    pub fn success_rate(&self) -> f64 {
        let total = self.total_items();
        if total == 0 {
            1.0
        } else {
            self.processed_count as f64 / total as f64
        }
    }

    /// Add a warning message
    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }

    /// Check if the result indicates good performance
    pub fn is_performing_well(&self) -> bool {
        // Consider performance good if:
        // - Success rate > 90%
        // - Average processing time < 1000ms per item
        // - No more than 5 warnings
        self.success_rate() > 0.9
            && self.avg_processing_time_ms < 1000.0
            && self.warnings.len() <= 5
    }
}
