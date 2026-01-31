//! # Command Outcome Classification
//!
//! Provides a unified outcome type for classifying orchestration command results.
//! Used by the event system's send-and-await pipeline to uniformly handle stats
//! tracking and error reporting across all command types.

use crate::orchestration::commands::{
    StepProcessResult, TaskFinalizationResult, TaskInitializeResult,
};

/// Classifies any command result into a uniform outcome for stats and logging.
///
/// The orchestration event system processes three command types (StepResult,
/// TaskInitialize, TaskFinalization) through the same send-and-await pipeline.
/// Each has different result variants, but from the event system's perspective
/// they all reduce to:
/// - **Success**: Increment `operations_coordinated`
/// - **Failed**: Increment `events_failed`, return error
/// - **Skipped**: Increment `operations_coordinated` (non-error skip)
///
/// Uses an enum instead of a trait because the result type set is intentionally
/// finite and exhaustive — there are exactly three command result types.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CommandOutcome {
    /// Command completed successfully
    Success,
    /// Command processing failed
    Failed(String),
    /// Command was skipped (non-error condition)
    Skipped(String),
}

impl CommandOutcome {
    /// Classify a step result processing outcome
    pub fn from_step_result(result: &StepProcessResult) -> Self {
        match result {
            StepProcessResult::Success { .. } => Self::Success,
            StepProcessResult::Failed { error } => Self::Failed(error.clone()),
            StepProcessResult::Skipped { reason } => Self::Skipped(reason.clone()),
        }
    }

    /// Classify a task initialization outcome
    pub fn from_task_initialize_result(result: &TaskInitializeResult) -> Self {
        match result {
            TaskInitializeResult::Success { .. } => Self::Success,
            TaskInitializeResult::Failed { error } => Self::Failed(error.clone()),
            TaskInitializeResult::Skipped { reason } => Self::Skipped(reason.clone()),
        }
    }

    /// Classify a task finalization outcome
    ///
    /// `NotClaimed` maps to `Skipped` — it's a non-error condition where another
    /// processor already claimed the finalization.
    pub fn from_task_finalization_result(result: &TaskFinalizationResult) -> Self {
        match result {
            TaskFinalizationResult::Success { .. } => Self::Success,
            TaskFinalizationResult::Failed { error } => Self::Failed(error.clone()),
            TaskFinalizationResult::NotClaimed { reason, .. } => Self::Skipped(reason.clone()),
        }
    }

    #[cfg(test)]
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success)
    }

    #[cfg(test)]
    pub fn is_failure(&self) -> bool {
        matches!(self, Self::Failed(_))
    }

    /// Get the detail message for non-success outcomes
    #[cfg(test)]
    pub fn detail(&self) -> Option<&str> {
        match self {
            Self::Success => None,
            Self::Failed(s) | Self::Skipped(s) => Some(s),
        }
    }

    /// Get a human-readable label for the outcome
    #[cfg(test)]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failed(_) => "failed",
            Self::Skipped(_) => "skipped",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    // --- from_step_result ---

    #[test]
    fn test_from_step_result_success() {
        let result = StepProcessResult::Success {
            message: "done".to_string(),
        };
        let outcome = CommandOutcome::from_step_result(&result);
        assert_eq!(outcome, CommandOutcome::Success);
        assert!(outcome.is_success());
        assert!(!outcome.is_failure());
        assert_eq!(outcome.detail(), None);
        assert_eq!(outcome.label(), "success");
    }

    #[test]
    fn test_from_step_result_failed() {
        let result = StepProcessResult::Failed {
            error: "timeout".to_string(),
        };
        let outcome = CommandOutcome::from_step_result(&result);
        assert_eq!(outcome, CommandOutcome::Failed("timeout".to_string()));
        assert!(!outcome.is_success());
        assert!(outcome.is_failure());
        assert_eq!(outcome.detail(), Some("timeout"));
        assert_eq!(outcome.label(), "failed");
    }

    #[test]
    fn test_from_step_result_skipped() {
        let result = StepProcessResult::Skipped {
            reason: "duplicate".to_string(),
        };
        let outcome = CommandOutcome::from_step_result(&result);
        assert_eq!(outcome, CommandOutcome::Skipped("duplicate".to_string()));
        assert!(!outcome.is_success());
        assert!(!outcome.is_failure());
        assert_eq!(outcome.detail(), Some("duplicate"));
        assert_eq!(outcome.label(), "skipped");
    }

    // --- from_task_initialize_result ---

    #[test]
    fn test_from_task_initialize_success() {
        let result = TaskInitializeResult::Success {
            task_uuid: Uuid::nil(),
            message: "initialized".to_string(),
        };
        let outcome = CommandOutcome::from_task_initialize_result(&result);
        assert_eq!(outcome, CommandOutcome::Success);
    }

    #[test]
    fn test_from_task_initialize_failed() {
        let result = TaskInitializeResult::Failed {
            error: "invalid config".to_string(),
        };
        let outcome = CommandOutcome::from_task_initialize_result(&result);
        assert_eq!(
            outcome,
            CommandOutcome::Failed("invalid config".to_string())
        );
    }

    #[test]
    fn test_from_task_initialize_skipped() {
        let result = TaskInitializeResult::Skipped {
            reason: "already exists".to_string(),
        };
        let outcome = CommandOutcome::from_task_initialize_result(&result);
        assert_eq!(
            outcome,
            CommandOutcome::Skipped("already exists".to_string())
        );
    }

    // --- from_task_finalization_result ---

    #[test]
    fn test_from_finalization_success() {
        let result = TaskFinalizationResult::Success {
            task_uuid: Uuid::nil(),
            final_status: "complete".to_string(),
            completion_time: None,
        };
        let outcome = CommandOutcome::from_task_finalization_result(&result);
        assert_eq!(outcome, CommandOutcome::Success);
    }

    #[test]
    fn test_from_finalization_failed() {
        let result = TaskFinalizationResult::Failed {
            error: "db error".to_string(),
        };
        let outcome = CommandOutcome::from_task_finalization_result(&result);
        assert_eq!(outcome, CommandOutcome::Failed("db error".to_string()));
    }

    #[test]
    fn test_from_finalization_not_claimed_maps_to_skipped() {
        let result = TaskFinalizationResult::NotClaimed {
            reason: "claimed by other".to_string(),
            already_claimed_by: Some(Uuid::nil()),
        };
        let outcome = CommandOutcome::from_task_finalization_result(&result);
        assert_eq!(
            outcome,
            CommandOutcome::Skipped("claimed by other".to_string())
        );
        assert!(!outcome.is_failure());
    }

    #[test]
    fn test_from_finalization_not_claimed_without_claimer() {
        let result = TaskFinalizationResult::NotClaimed {
            reason: "not ready".to_string(),
            already_claimed_by: None,
        };
        let outcome = CommandOutcome::from_task_finalization_result(&result);
        assert_eq!(outcome, CommandOutcome::Skipped("not ready".to_string()));
    }

    // --- Clone and equality ---

    #[test]
    fn test_clone_preserves_equality() {
        let outcome = CommandOutcome::Failed("err".to_string());
        let cloned = outcome.clone();
        assert_eq!(outcome, cloned);
    }

    #[test]
    fn test_variants_are_distinct() {
        assert_ne!(
            CommandOutcome::Success,
            CommandOutcome::Failed("x".to_string())
        );
        assert_ne!(
            CommandOutcome::Success,
            CommandOutcome::Skipped("x".to_string())
        );
        assert_ne!(
            CommandOutcome::Failed("x".to_string()),
            CommandOutcome::Skipped("x".to_string())
        );
    }
}
