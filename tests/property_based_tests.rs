mod common;

use common::strategies::*;
use proptest::prelude::*;

proptest! {
    /// Property: Generated DAG patterns should always be acyclic
    #[test]
    fn generated_workflow_patterns_are_acyclic(pattern in workflow_pattern_strategy()) {
        prop_assert!(pattern.is_valid_dag(), "Generated pattern should be a valid DAG: {:?}", pattern);
    }

    /// Property: Namespace names should always be valid identifiers
    #[test]
    fn namespace_names_are_valid_identifiers(name in namespace_name_strategy()) {
        // Should start with letter or underscore
        let first_char = name.chars().next().unwrap();
        prop_assert!(first_char.is_ascii_alphabetic() || first_char == '_');

        // Should contain only valid characters
        prop_assert!(name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));

        // Should not be empty
        prop_assert!(!name.is_empty());

        // Should not be too long
        prop_assert!(name.len() <= 64);
    }

    /// Property: JSON contexts should round-trip through serialization
    #[test]
    fn json_contexts_round_trip_correctly(context in json_context_strategy()) {
        let serialized = serde_json::to_string(&context).unwrap();
        let deserialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        prop_assert_eq!(context, deserialized);
    }

    /// Property: DAG edges should never create self-loops
    #[test]
    fn dag_edges_have_no_self_loops(edges in dag_edges_strategy()) {
        for (from, to) in edges {
            prop_assert_ne!(from, to, "Edge ({}, {}) is a self-loop", from, to);
        }
    }
}

/// Property-based test for complex DAG operations
#[tokio::test]
#[ignore] // TODO: Fix async issues with property testing
async fn test_dag_properties_with_database() {
    // This test needs to be reimplemented with proper async/proptest integration
    // For now, it's ignored to avoid compilation issues
    todo!("Implement property-based DAG testing with proper async support");
}

/// Property-based test for state transitions
#[tokio::test]
#[ignore] // TODO: Fix async issues with property testing
async fn test_state_transition_properties() {
    // This test needs to be reimplemented with proper async/proptest integration
    // For now, it's ignored to avoid compilation issues
    todo!("Implement property-based state transition testing with proper async support");
}

/// Property-based test for retry logic
#[tokio::test]
#[ignore] // TODO: Fix async issues with property testing
async fn test_retry_logic_properties() {
    // This test needs to be reimplemented with proper async/proptest integration
    // For now, it's ignored to avoid compilation issues
    todo!("Implement property-based retry logic testing with proper async support");
}

#[cfg(test)]
mod workflow_pattern_invariants {
    use super::common::strategies::*;

    #[test]
    fn test_linear_workflow_invariants() {
        let pattern = WorkflowPattern::Linear(5);

        // Linear workflows should have exactly n-1 edges
        assert_eq!(pattern.edges().len(), pattern.step_count() - 1);

        // Should be a valid DAG
        assert!(pattern.is_valid_dag());

        // Should have exactly one root and one leaf
        let edges = pattern.edges();
        let _nodes: std::collections::HashSet<usize> =
            edges.iter().flat_map(|(from, to)| [*from, *to]).collect();

        let sources: std::collections::HashSet<usize> = edges
            .iter()
            .filter_map(|(from, _to)| {
                if !edges.iter().any(|(_, target)| target == from) {
                    Some(*from)
                } else {
                    None
                }
            })
            .collect();

        let sinks: std::collections::HashSet<usize> = edges
            .iter()
            .filter_map(|(_from, to)| {
                if !edges.iter().any(|(source, _)| source == to) {
                    Some(*to)
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(
            sources.len(),
            1,
            "Linear workflow should have exactly one source"
        );
        assert_eq!(
            sinks.len(),
            1,
            "Linear workflow should have exactly one sink"
        );
    }

    #[test]
    fn test_diamond_workflow_invariants() {
        let pattern = WorkflowPattern::Diamond;

        assert_eq!(pattern.step_count(), 4);
        assert_eq!(pattern.edges().len(), 4);
        assert!(pattern.is_valid_dag());

        let edges = pattern.edges();
        assert_eq!(edges, vec![(0, 1), (0, 2), (1, 3), (2, 3)]);
    }

    #[test]
    fn test_fan_out_workflow_invariants() {
        let pattern = WorkflowPattern::FanOut(3);

        assert_eq!(pattern.step_count(), 4); // 1 source + 3 targets
        assert_eq!(pattern.edges().len(), 3); // Source connects to each target
        assert!(pattern.is_valid_dag());

        let edges = pattern.edges();
        // All edges should originate from node 0
        for (_from, _) in edges {
            assert_eq!(_from, 0);
        }
    }
}
