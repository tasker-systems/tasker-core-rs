use proptest::prelude::*;
use proptest::strategy::Just;
use tasker_core::models::{
    task_namespace::NewTaskNamespace,
    named_step::NewNamedStep,
};

/// Strategy for generating valid task namespace names
pub fn namespace_name_strategy() -> impl Strategy<Value = String> {
    "[a-zA-Z_][a-zA-Z0-9_]{0,63}"
}

/// Strategy for generating optional descriptions
pub fn description_strategy() -> impl Strategy<Value = Option<String>> {
    prop::option::of("[a-zA-Z0-9 .,!?]{0,255}")
}

/// Strategy for generating NewTaskNamespace instances
pub fn new_task_namespace_strategy() -> impl Strategy<Value = NewTaskNamespace> {
    (namespace_name_strategy(), description_strategy())
        .prop_map(|(name, description)| NewTaskNamespace { name, description })
}

/// Strategy for generating handler class names
pub fn handler_class_strategy() -> impl Strategy<Value = String> {
    "[A-Z][a-zA-Z0-9_]*Handler"
}

/// Strategy for generating NewNamedStep instances
pub fn new_named_step_strategy() -> impl Strategy<Value = NewNamedStep> {
    (
        1i32..10,  // dependent_system_id
        namespace_name_strategy(),
        description_strategy(),
    ).prop_map(|(dependent_system_id, name, description)| NewNamedStep {
        dependent_system_id,
        name,
        description,
    })
}

/// Strategy for generating valid JSON contexts
pub fn json_context_strategy() -> impl Strategy<Value = serde_json::Value> {
    prop_oneof![
        Just(serde_json::json!({})),
        Just(serde_json::json!({"key": "value"})),
        Just(serde_json::json!({"number": 42, "boolean": true})),
        Just(serde_json::json!({"nested": {"data": [1, 2, 3]}})),
        Just(serde_json::json!({"workflow_id": "test-workflow", "step_count": 5})),
    ]
}

/// Strategy for generating DAG edges (ensuring no self-loops)
pub fn dag_edge_strategy() -> impl Strategy<Value = (i64, i64)> {
    (1i64..=100, 1i64..=100)
        .prop_filter("No self-loops", |(from, to)| from != to)
}

/// Strategy for generating collections of DAG edges
pub fn dag_edges_strategy() -> impl Strategy<Value = Vec<(i64, i64)>> {
    prop::collection::vec(dag_edge_strategy(), 0..20)
}

/// Strategy for generating acyclic DAG structures
pub fn acyclic_dag_strategy() -> impl Strategy<Value = Vec<(usize, usize)>> {
    // Use predefined patterns instead of complex generation
    prop_oneof![
        Just(vec![(0, 1)]), // Simple 2-node chain
        Just(vec![(0, 1), (1, 2)]), // Simple 3-node chain
        Just(vec![(0, 1), (0, 2), (1, 2)]), // Triangle DAG
        Just(vec![(0, 1), (0, 2)]), // Fan-out from 0
        Just(vec![(0, 2), (1, 2)]), // Fan-in to 2
    ]
}

/// Strategy for generating retry counts
pub fn retry_count_strategy() -> impl Strategy<Value = i32> {
    0i32..=10
}

/// Strategy for generating max retries
pub fn max_retries_strategy() -> impl Strategy<Value = i32> {
    1i32..=10
}

/// Strategy for generating workflow step states
pub fn step_state_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("created".to_string()),
        Just("ready".to_string()),
        Just("running".to_string()),
        Just("complete".to_string()),
        Just("error".to_string()),
        Just("skipped".to_string()),
    ]
}

/// Strategy for generating task states
pub fn task_state_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("created".to_string()),
        Just("running".to_string()),
        Just("complete".to_string()),
        Just("error".to_string()),
        Just("cancelled".to_string()),
    ]
}

/// Strategy for generating realistic workflow patterns
#[derive(Debug, Clone)]
pub enum WorkflowPattern {
    Linear(usize),           // Linear chain of N steps
    Diamond,                 // Diamond: 1->2,3->4 pattern
    FanOut(usize),          // 1 -> N pattern
    FanIn(usize),           // N -> 1 pattern
    Complex(Vec<(usize, usize)>), // Custom DAG structure
}

pub fn workflow_pattern_strategy() -> impl Strategy<Value = WorkflowPattern> {
    prop_oneof![
        (2usize..=10).prop_map(WorkflowPattern::Linear),
        Just(WorkflowPattern::Diamond),
        (2usize..=8).prop_map(WorkflowPattern::FanOut),
        (2usize..=8).prop_map(WorkflowPattern::FanIn),
        acyclic_dag_strategy().prop_map(WorkflowPattern::Complex),
    ]
}

impl WorkflowPattern {
    /// Get the number of steps in this workflow pattern
    pub fn step_count(&self) -> usize {
        match self {
            WorkflowPattern::Linear(n) => *n,
            WorkflowPattern::Diamond => 4,
            WorkflowPattern::FanOut(n) => n + 1,
            WorkflowPattern::FanIn(n) => n + 1,
            WorkflowPattern::Complex(edges) => {
                edges.iter()
                    .flat_map(|(from, to)| [*from, *to])
                    .max()
                    .map(|max| max + 1)
                    .unwrap_or(1)
            }
        }
    }

    /// Get the edges for this workflow pattern
    pub fn edges(&self) -> Vec<(usize, usize)> {
        match self {
            WorkflowPattern::Linear(n) => {
                (0..(*n - 1)).map(|i| (i, i + 1)).collect()
            }
            WorkflowPattern::Diamond => {
                vec![(0, 1), (0, 2), (1, 3), (2, 3)]
            }
            WorkflowPattern::FanOut(n) => {
                (1..=*n).map(|i| (0, i)).collect()
            }
            WorkflowPattern::FanIn(n) => {
                (0..*n).map(|i| (i, *n)).collect()
            }
            WorkflowPattern::Complex(edges) => edges.clone(),
        }
    }

    /// Check if this pattern is a valid DAG (no cycles)
    pub fn is_valid_dag(&self) -> bool {
        let edges = self.edges();
        let node_count = self.step_count();
        
        // Use DFS to detect cycles
        let mut visited = vec![false; node_count];
        let mut rec_stack = vec![false; node_count];
        
        // Build adjacency list
        let mut adj_list: Vec<Vec<usize>> = vec![Vec::new(); node_count];
        for (from, to) in edges {
            if from < node_count && to < node_count {
                adj_list[from].push(to);
            }
        }
        
        fn has_cycle(
            node: usize,
            visited: &mut [bool],
            rec_stack: &mut [bool],
            adj_list: &[Vec<usize>],
        ) -> bool {
            visited[node] = true;
            rec_stack[node] = true;
            
            for &neighbor in &adj_list[node] {
                if !visited[neighbor] && has_cycle(neighbor, visited, rec_stack, adj_list) {
                    return true;
                }
                if rec_stack[neighbor] {
                    return true;
                }
            }
            
            rec_stack[node] = false;
            false
        }
        
        for i in 0..node_count {
            if !visited[i] && has_cycle(i, &mut visited, &mut rec_stack, &adj_list) {
                return false;
            }
        }
        
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    proptest! {
        #[test]
        fn test_namespace_name_strategy_generates_valid_names(name in namespace_name_strategy()) {
            // Names should start with letter or underscore
            assert!(name.chars().next().unwrap().is_ascii_alphabetic() || name.starts_with('_'));
            // Names should be reasonable length
            assert!(name.len() <= 64);
            // Names should contain only valid characters
            assert!(name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        }

        #[test]
        fn test_acyclic_dag_strategy_generates_valid_dags(pattern in workflow_pattern_strategy()) {
            // All generated patterns should be valid DAGs
            prop_assert!(pattern.is_valid_dag());
            // Should have reasonable number of steps
            prop_assert!(pattern.step_count() >= 1);
            prop_assert!(pattern.step_count() <= 20);
        }

        #[test]
        fn test_json_context_strategy_generates_valid_json(context in json_context_strategy()) {
            // Should be able to serialize and deserialize
            let serialized = serde_json::to_string(&context).unwrap();
            let deserialized: serde_json::Value = serde_json::from_str(&serialized).unwrap();
            prop_assert_eq!(context, deserialized);
        }
    }

    #[test]
    fn test_workflow_patterns() {
        // Test linear pattern
        let linear = WorkflowPattern::Linear(3);
        assert_eq!(linear.step_count(), 3);
        assert_eq!(linear.edges(), vec![(0, 1), (1, 2)]);
        assert!(linear.is_valid_dag());

        // Test diamond pattern
        let diamond = WorkflowPattern::Diamond;
        assert_eq!(diamond.step_count(), 4);
        assert_eq!(diamond.edges(), vec![(0, 1), (0, 2), (1, 3), (2, 3)]);
        assert!(diamond.is_valid_dag());

        // Test fan-out pattern
        let fan_out = WorkflowPattern::FanOut(3);
        assert_eq!(fan_out.step_count(), 4);
        assert_eq!(fan_out.edges(), vec![(0, 1), (0, 2), (0, 3)]);
        assert!(fan_out.is_valid_dag());
    }
}