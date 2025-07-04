//! # Task Diagram Service
//!
//! **CRITICAL**: This is NOT a database table - it's a service class for diagram generation.
//!
//! ## Overview
//!
//! The `TaskDiagram` is a service class that generates workflow visualization diagrams
//! from task data. Unlike other models, this does NOT represent stored data - it's a
//! computational service that creates diagrams on-demand from existing workflow data.
//!
//! ## Service Pattern
//!
//! This follows the Rails service pattern where:
//! - Takes a task_id as input
//! - Analyzes workflow steps and edges
//! - Generates visualization outputs (Mermaid, JSON, etc.)
//! - No persistence - purely computational
//!
//! ## Dependencies
//!
//! The diagram generation relies on:
//! - `tasker_workflow_steps` - Step instances in the task
//! - `tasker_workflow_step_edges` - DAG relationships between steps
//! - `tasker_workflow_step_transitions` - Current step states
//!
//! ## Note on DAG View
//!
//! This implementation is simplified until the `tasker_step_dag_relationships`
//! materialized view is added to the migration. Once available, this can be
//! enhanced with more sophisticated DAG analysis.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use std::collections::HashMap;

/// Service class for generating task workflow diagrams.
///
/// **IMPORTANT**: This is NOT a database table - it's a service that generates
/// diagrams from existing workflow data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDiagram {
    pub task_id: i64,
    pub nodes: Vec<DiagramNode>,
    pub edges: Vec<DiagramEdge>,
    pub metadata: JsonValue,
}

/// Represents a node in the workflow diagram.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagramNode {
    pub workflow_step_id: i64,
    pub named_step_id: i32,
    pub name: String,
    pub current_state: String,
    pub position: Option<DiagramPosition>,
    pub metadata: JsonValue,
}

/// Represents an edge in the workflow diagram.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagramEdge {
    pub from_step_id: i64,
    pub to_step_id: i64,
    pub edge_name: String,
    pub edge_type: String, // "dependency", "sequence", etc.
}

/// Position information for diagram layout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagramPosition {
    pub x: f64,
    pub y: f64,
    pub layer: i32, // For hierarchical layout
}

/// Flowchart configuration for diagram generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flowchart {
    pub direction: String, // "TD", "LR", etc.
    pub theme: String,
    pub show_states: bool,
    pub show_dependencies: bool,
}

/// Diagram statistics and metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagramStats {
    pub total_nodes: usize,
    pub total_edges: usize,
    pub max_depth: i32,
    pub branching_factor: f64,
    pub completion_percentage: f64,
}

/// Compatibility stubs (not used in service pattern)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewTaskDiagram {
    // Placeholder - not used since this is a service, not a model
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDiagramWithTask {
    // Placeholder - not used since this is a service, not a model
}

impl TaskDiagram {
    /// Generate a new task diagram by analyzing workflow data.
    ///
    /// This is the main entry point for diagram generation. It analyzes
    /// the task's workflow steps and their relationships to create a
    /// comprehensive diagram representation.
    pub async fn generate_for_task(
        pool: &PgPool,
        task_id: i64,
    ) -> Result<TaskDiagram, sqlx::Error> {
        // Get all workflow steps for the task
        let steps = sqlx::query!(
            r#"
            SELECT 
                ws.workflow_step_id,
                ws.named_step_id,
                ns.name as step_name,
                COALESCE(wst.to_state, 'pending') as current_state
            FROM tasker_workflow_steps ws
            INNER JOIN tasker_named_steps ns ON ns.named_step_id = ws.named_step_id
            LEFT JOIN tasker_workflow_step_transitions wst ON wst.workflow_step_id = ws.workflow_step_id 
                AND wst.most_recent = true
            WHERE ws.task_id = $1
            ORDER BY ws.workflow_step_id
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?;

        // Get all edges (dependencies) for the task steps
        let edges = sqlx::query!(
            r#"
            SELECT 
                wse.from_step_id,
                wse.to_step_id,
                wse.name as edge_name
            FROM tasker_workflow_step_edges wse
            INNER JOIN tasker_workflow_steps ws_from ON ws_from.workflow_step_id = wse.from_step_id
            INNER JOIN tasker_workflow_steps ws_to ON ws_to.workflow_step_id = wse.to_step_id
            WHERE ws_from.task_id = $1 AND ws_to.task_id = $1
            ORDER BY wse.from_step_id, wse.to_step_id
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?;

        // Convert to diagram nodes
        let nodes: Vec<DiagramNode> = steps
            .into_iter()
            .map(|step| {
                DiagramNode {
                    workflow_step_id: step.workflow_step_id,
                    named_step_id: step.named_step_id,
                    name: step.step_name,
                    current_state: step.current_state.unwrap_or_else(|| "pending".to_string()),
                    position: None, // Could be enhanced with layout algorithm
                    metadata: serde_json::json!({}),
                }
            })
            .collect();

        // Convert to diagram edges
        let diagram_edges: Vec<DiagramEdge> = edges
            .into_iter()
            .map(|edge| DiagramEdge {
                from_step_id: edge.from_step_id,
                to_step_id: edge.to_step_id,
                edge_name: edge.edge_name,
                edge_type: "dependency".to_string(),
            })
            .collect();

        Ok(TaskDiagram {
            task_id,
            nodes,
            edges: diagram_edges,
            metadata: serde_json::json!({
                "generated_at": chrono::Utc::now().to_rfc3339(),
                "version": "1.0"
            }),
        })
    }

    /// Generate Mermaid flowchart syntax for the diagram.
    ///
    /// Creates a Mermaid.js compatible flowchart that can be rendered
    /// in web interfaces or documentation.
    pub fn to_mermaid(&self, config: Option<Flowchart>) -> String {
        let flowchart = config.unwrap_or_else(|| Flowchart {
            direction: "TD".to_string(),
            theme: "default".to_string(),
            show_states: true,
            show_dependencies: true,
        });

        let mut mermaid = format!("flowchart {}\n", flowchart.direction);

        // Add nodes with state styling
        for node in &self.nodes {
            let style_class = match node.current_state.as_str() {
                "complete" => "success",
                "error" => "error",
                "in_progress" => "active",
                _ => "default",
            };

            mermaid.push_str(&format!(
                "    {}[\"{}\"]\n    {}:::{}\n",
                node.workflow_step_id, node.name, node.workflow_step_id, style_class
            ));
        }

        // Add edges if enabled
        if flowchart.show_dependencies {
            for edge in &self.edges {
                mermaid.push_str(&format!(
                    "    {} --> {}\n",
                    edge.from_step_id, edge.to_step_id
                ));
            }
        }

        // Add CSS classes
        mermaid.push_str("\n    classDef success fill:#d4edda,stroke:#155724,stroke-width:2px\n");
        mermaid.push_str("    classDef error fill:#f8d7da,stroke:#721c24,stroke-width:2px\n");
        mermaid.push_str("    classDef active fill:#d1ecf1,stroke:#0c5460,stroke-width:2px\n");
        mermaid.push_str("    classDef default fill:#e2e3e5,stroke:#383d41,stroke-width:1px\n");

        mermaid
    }

    /// Generate JSON representation of the diagram.
    ///
    /// Creates a JSON structure suitable for custom diagram renderers
    /// or API responses.
    pub fn to_json(&self) -> Result<JsonValue, serde_json::Error> {
        serde_json::to_value(self)
    }

    /// Generate HTML representation with embedded diagram.
    ///
    /// Creates a simple HTML page with embedded Mermaid diagram.
    pub fn to_html(&self, title: Option<&str>) -> String {
        let mermaid = self.to_mermaid(None);
        let default_title = format!("Task {} Workflow", self.task_id);
        let page_title = title.unwrap_or(&default_title);

        format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <title>{}</title>
    <script src="https://cdn.jsdelivr.net/npm/mermaid@10.6.1/dist/mermaid.min.js"></script>
    <script>mermaid.initialize({{startOnLoad: true}});</script>
</head>
<body>
    <h1>{}</h1>
    <div class="mermaid">
{}
    </div>
</body>
</html>"#,
            page_title, page_title, mermaid
        )
    }

    /// Calculate diagram statistics.
    pub fn get_stats(&self) -> DiagramStats {
        let total_nodes = self.nodes.len();
        let total_edges = self.edges.len();

        // Calculate completion percentage
        let completed_nodes = self
            .nodes
            .iter()
            .filter(|n| n.current_state == "complete")
            .count();
        let completion_percentage = if total_nodes > 0 {
            (completed_nodes as f64 / total_nodes as f64) * 100.0
        } else {
            0.0
        };

        // Calculate branching factor (average edges per node)
        let branching_factor = if total_nodes > 0 {
            total_edges as f64 / total_nodes as f64
        } else {
            0.0
        };

        // Simple depth calculation (could be enhanced with proper DAG analysis)
        let max_depth = self.calculate_max_depth();

        DiagramStats {
            total_nodes,
            total_edges,
            max_depth,
            branching_factor,
            completion_percentage,
        }
    }

    /// Calculate maximum depth of the DAG (simplified version).
    fn calculate_max_depth(&self) -> i32 {
        // This is a simplified implementation
        // Would be enhanced once tasker_step_dag_relationships view is available
        let mut depth_map: HashMap<i64, i32> = HashMap::new();
        let mut max_depth = 0;

        // Find root nodes (no incoming edges)
        let mut root_nodes = Vec::new();
        for node in &self.nodes {
            let has_incoming = self
                .edges
                .iter()
                .any(|e| e.to_step_id == node.workflow_step_id);
            if !has_incoming {
                root_nodes.push(node.workflow_step_id);
                depth_map.insert(node.workflow_step_id, 0);
            }
        }

        // Simple iterative depth calculation
        let mut changed = true;
        while changed {
            changed = false;
            for edge in &self.edges {
                if let Some(&from_depth) = depth_map.get(&edge.from_step_id) {
                    let new_depth = from_depth + 1;
                    let current_depth = depth_map.get(&edge.to_step_id).unwrap_or(&-1);
                    if new_depth > *current_depth {
                        depth_map.insert(edge.to_step_id, new_depth);
                        max_depth = max_depth.max(new_depth);
                        changed = true;
                    }
                }
            }
        }

        max_depth
    }
}

impl Default for Flowchart {
    fn default() -> Self {
        Self {
            direction: "TD".to_string(),
            theme: "default".to_string(),
            show_states: true,
            show_dependencies: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DatabaseConnection;

    #[tokio::test]
    async fn test_generate_task_diagram() {
        let db = DatabaseConnection::new()
            .await
            .expect("Failed to connect to database");
        let pool = db.pool();

        // Create test dependencies
        let namespace = crate::models::task_namespace::TaskNamespace::create(
            pool,
            crate::models::task_namespace::NewTaskNamespace {
                name: format!(
                    "test_namespace_{}",
                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
                ),
                description: None,
            },
        )
        .await
        .expect("Failed to create namespace");

        let named_task = crate::models::named_task::NamedTask::create(
            pool,
            crate::models::named_task::NewNamedTask {
                name: format!(
                    "test_task_{}",
                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
                ),
                version: Some("1.0.0".to_string()),
                description: None,
                task_namespace_id: namespace.task_namespace_id as i64,
                configuration: None,
            },
        )
        .await
        .expect("Failed to create named task");

        let task = crate::models::task::Task::create(
            pool,
            crate::models::task::NewTask {
                named_task_id: named_task.named_task_id,
                identity_hash: format!(
                    "test_hash_{}",
                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
                ),
                requested_at: None,
                initiator: None,
                source_system: None,
                reason: None,
                bypass_steps: None,
                tags: None,
                context: None,
            },
        )
        .await
        .expect("Failed to create task");

        // Generate diagram (might be empty for a new task)
        let diagram = TaskDiagram::generate_for_task(pool, task.task_id)
            .await
            .expect("Failed to generate diagram");

        assert_eq!(diagram.task_id, task.task_id);
        // Could be empty if no workflow steps exist yet
        // Diagram should be generated without error (nodes/edges could be empty)
    }

    #[tokio::test]
    async fn test_diagram_to_mermaid() {
        let diagram = TaskDiagram {
            task_id: 1,
            nodes: vec![
                DiagramNode {
                    workflow_step_id: 1,
                    named_step_id: 1,
                    name: "Step 1".to_string(),
                    current_state: "complete".to_string(),
                    position: None,
                    metadata: serde_json::json!({}),
                },
                DiagramNode {
                    workflow_step_id: 2,
                    named_step_id: 2,
                    name: "Step 2".to_string(),
                    current_state: "pending".to_string(),
                    position: None,
                    metadata: serde_json::json!({}),
                },
            ],
            edges: vec![DiagramEdge {
                from_step_id: 1,
                to_step_id: 2,
                edge_name: "depends_on".to_string(),
                edge_type: "dependency".to_string(),
            }],
            metadata: serde_json::json!({}),
        };

        let mermaid = diagram.to_mermaid(None);
        assert!(mermaid.contains("flowchart TD"));
        assert!(mermaid.contains("Step 1"));
        assert!(mermaid.contains("Step 2"));
        assert!(mermaid.contains("1 --> 2"));
    }
}
