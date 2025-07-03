use chrono::{NaiveDateTime};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::{FromRow, PgPool};
use sqlx::types::BigDecimal;

/// TaskDiagram represents visualization data for tasks
/// Maps to `tasker_task_diagrams` table - workflow visualization (10KB Rails model)
/// Generates Mermaid diagrams, HTML documents, and JSON representations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct TaskDiagram {
    pub task_diagram_id: i64,
    pub task_id: i64,
    pub diagram_type: String,
    pub diagram_data: String,
    pub layout_metadata: JsonValue,
    pub generated_at: NaiveDateTime,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New TaskDiagram for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewTaskDiagram {
    pub task_id: i64,
    pub diagram_type: Option<String>,
    pub diagram_data: String,
    pub layout_metadata: Option<JsonValue>,
}

/// Task diagram with task details
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TaskDiagramWithTask {
    pub task_diagram_id: i64,
    pub task_id: i64,
    pub diagram_type: String,
    pub diagram_data: String,
    pub layout_metadata: JsonValue,
    pub generated_at: NaiveDateTime,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub task_name: Option<String>,
    pub task_complete: bool,
    pub workflow_step_count: Option<i64>,
}

/// Diagram node representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagramNode {
    pub id: String,
    pub label: String,
    pub shape: Option<String>,
    pub style: Option<String>,
    pub url: Option<String>,
}

/// Diagram edge representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagramEdge {
    pub source_id: String,
    pub target_id: String,
    pub label: Option<String>,
}

/// Complete flowchart structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flowchart {
    pub direction: String,
    pub title: Option<String>,
    pub nodes: Vec<DiagramNode>,
    pub edges: Vec<DiagramEdge>,
}

/// Diagram generation statistics
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DiagramStats {
    pub diagram_type: String,
    pub diagram_count: Option<i64>,
    pub avg_nodes: Option<BigDecimal>,
    pub avg_edges: Option<BigDecimal>,
}

impl TaskDiagram {
    /// Create a new task diagram
    pub async fn create(pool: &PgPool, new_diagram: NewTaskDiagram) -> Result<TaskDiagram, sqlx::Error> {
        let diagram = sqlx::query_as!(
            TaskDiagram,
            r#"
            INSERT INTO tasker_task_diagrams 
            (task_id, diagram_type, diagram_data, layout_metadata)
            VALUES ($1, $2, $3, $4)
            RETURNING task_diagram_id, task_id, diagram_type, diagram_data, layout_metadata, 
                      generated_at, created_at, updated_at
            "#,
            new_diagram.task_id,
            new_diagram.diagram_type.unwrap_or_else(|| "mermaid".to_string()),
            new_diagram.diagram_data,
            new_diagram.layout_metadata.unwrap_or(JsonValue::Object(serde_json::Map::new()))
        )
        .fetch_one(pool)
        .await?;

        Ok(diagram)
    }

    /// Find a task diagram by ID
    pub async fn find_by_id(pool: &PgPool, id: i64) -> Result<Option<TaskDiagram>, sqlx::Error> {
        let diagram = sqlx::query_as!(
            TaskDiagram,
            r#"
            SELECT task_diagram_id, task_id, diagram_type, diagram_data, layout_metadata,
                   generated_at, created_at, updated_at
            FROM tasker_task_diagrams
            WHERE task_diagram_id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(diagram)
    }

    /// Find diagrams for a specific task (Rails scope: for_task)
    pub async fn for_task(pool: &PgPool, task_id: i64) -> Result<Vec<TaskDiagram>, sqlx::Error> {
        let diagrams = sqlx::query_as!(
            TaskDiagram,
            r#"
            SELECT task_diagram_id, task_id, diagram_type, diagram_data, layout_metadata,
                   generated_at, created_at, updated_at
            FROM tasker_task_diagrams
            WHERE task_id = $1
            ORDER BY generated_at DESC
            "#,
            task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(diagrams)
    }

    /// Find latest diagram for a task (Rails scope: latest_for_task)
    pub async fn latest_for_task(pool: &PgPool, task_id: i64) -> Result<Option<TaskDiagram>, sqlx::Error> {
        let diagram = sqlx::query_as!(
            TaskDiagram,
            r#"
            SELECT task_diagram_id, task_id, diagram_type, diagram_data, layout_metadata,
                   generated_at, created_at, updated_at
            FROM tasker_task_diagrams
            WHERE task_id = $1
            ORDER BY generated_at DESC
            LIMIT 1
            "#,
            task_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(diagram)
    }

    /// Find diagrams by type (Rails scope: by_type)
    pub async fn by_type(pool: &PgPool, diagram_type: &str) -> Result<Vec<TaskDiagram>, sqlx::Error> {
        let diagrams = sqlx::query_as!(
            TaskDiagram,
            r#"
            SELECT task_diagram_id, task_id, diagram_type, diagram_data, layout_metadata,
                   generated_at, created_at, updated_at
            FROM tasker_task_diagrams
            WHERE diagram_type = $1
            ORDER BY generated_at DESC
            "#,
            diagram_type
        )
        .fetch_all(pool)
        .await?;

        Ok(diagrams)
    }

    /// Get recent diagrams (Rails scope: recent)
    pub async fn recent(pool: &PgPool, limit: Option<i64>) -> Result<Vec<TaskDiagram>, sqlx::Error> {
        let limit = limit.unwrap_or(50);
        let diagrams = sqlx::query_as!(
            TaskDiagram,
            r#"
            SELECT task_diagram_id, task_id, diagram_type, diagram_data, layout_metadata,
                   generated_at, created_at, updated_at
            FROM tasker_task_diagrams
            ORDER BY generated_at DESC
            LIMIT $1
            "#,
            limit
        )
        .fetch_all(pool)
        .await?;

        Ok(diagrams)
    }

    /// Get diagrams with task information (Rails includes: task)
    pub async fn with_tasks(pool: &PgPool) -> Result<Vec<TaskDiagramWithTask>, sqlx::Error> {
        let diagrams = sqlx::query_as!(
            TaskDiagramWithTask,
            r#"
            SELECT 
                td.task_diagram_id,
                td.task_id,
                td.diagram_type,
                td.diagram_data,
                td.layout_metadata,
                td.generated_at,
                td.created_at,
                td.updated_at,
                nt.name as task_name,
                t.complete as task_complete,
                (SELECT COUNT(*) FROM tasker_workflow_steps ws WHERE ws.task_id = t.task_id) as workflow_step_count
            FROM tasker_task_diagrams td
            JOIN tasker_tasks t ON t.task_id = td.task_id
            LEFT JOIN tasker_named_tasks nt ON nt.named_task_id = t.named_task_id
            ORDER BY td.generated_at DESC
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(diagrams)
    }

    /// Search diagrams by layout metadata (Rails scope: search_metadata)
    pub async fn search_metadata(pool: &PgPool, key: &str, value: &str) -> Result<Vec<TaskDiagram>, sqlx::Error> {
        let diagrams = sqlx::query_as!(
            TaskDiagram,
            r#"
            SELECT task_diagram_id, task_id, diagram_type, diagram_data, layout_metadata,
                   generated_at, created_at, updated_at
            FROM tasker_task_diagrams
            WHERE layout_metadata ->> $1 ILIKE $2
            ORDER BY generated_at DESC
            "#,
            key,
            format!("%{}%", value)
        )
        .fetch_all(pool)
        .await?;

        Ok(diagrams)
    }

    /// Generate a Mermaid diagram for a task (Rails method: to_mermaid)
    pub async fn generate_mermaid_for_task(pool: &PgPool, task_id: i64) -> Result<String, sqlx::Error> {
        // Build flowchart structure from task and workflow steps
        let flowchart = Self::build_flowchart_for_task(pool, task_id).await?;
        Ok(Self::flowchart_to_mermaid(&flowchart))
    }

    /// Generate HTML document with Mermaid diagram (Rails method: to_html)
    pub async fn generate_html_for_task(pool: &PgPool, task_id: i64, base_url: Option<&str>) -> Result<String, sqlx::Error> {
        let mermaid_diagram = Self::generate_mermaid_for_task(pool, task_id).await?;
        Ok(Self::mermaid_to_html(&mermaid_diagram, task_id, base_url))
    }

    /// Generate JSON representation (Rails method: to_json)
    pub async fn generate_json_for_task(pool: &PgPool, task_id: i64, pretty: bool) -> Result<String, sqlx::Error> {
        let flowchart = Self::build_flowchart_for_task(pool, task_id).await?;
        
        if pretty {
            Ok(serde_json::to_string_pretty(&flowchart).unwrap_or_default())
        } else {
            Ok(serde_json::to_string(&flowchart).unwrap_or_default())
        }
    }

    /// Build flowchart structure for a task
    async fn build_flowchart_for_task(pool: &PgPool, task_id: i64) -> Result<Flowchart, sqlx::Error> {
        use crate::models::{Task, WorkflowStep};

        // Get task details
        let task = Task::find_by_id(pool, task_id).await?
            .ok_or_else(|| sqlx::Error::RowNotFound)?;

        // Get workflow steps for the task
        let workflow_steps = WorkflowStep::for_task(pool, task_id).await?;

        let mut nodes = vec![];
        let mut edges = vec![];

        // Add task node
        let task_name = task.identity_hash.clone(); // Using identity_hash as task name for now
        nodes.push(DiagramNode {
            id: format!("task_{}", task_id),
            label: format!("Task: {}\nID: {}\nComplete: {}", task_name, task_id, task.complete),
            shape: Some("box".to_string()),
            style: Some("fill:lightblue;".to_string()),
            url: None,
        });

        // Add workflow step nodes
        for step in &workflow_steps {
            let step_status = if step.processed {
                "complete"
            } else if step.in_process {
                "in_progress"
            } else {
                "pending"
            };

            let color = match step_status {
                "complete" => "green",
                "in_progress" => "lightgreen",
                "pending" => "lightblue",
                _ => "lightgray",
            };

            nodes.push(DiagramNode {
                id: format!("step_{}", step.workflow_step_id),
                label: format!(
                    "Step: {}\nStatus: {}\nAttempts: {}",
                    step.workflow_step_id,
                    step_status,
                    step.attempts.unwrap_or(0)
                ),
                shape: Some("box".to_string()),
                style: Some(format!("fill:{};", color)),
                url: None,
            });
        }

        // Add edges from task to root steps and between steps
        for step in &workflow_steps {
            // For now, connect task to all steps (simplified)
            edges.push(DiagramEdge {
                source_id: format!("task_{}", task_id),
                target_id: format!("step_{}", step.workflow_step_id),
                label: None,
            });
        }

        Ok(Flowchart {
            direction: "TD".to_string(),
            title: Some(format!("Task {}: {}", task_id, task_name)),
            nodes,
            edges,
        })
    }

    /// Convert flowchart to Mermaid syntax
    fn flowchart_to_mermaid(flowchart: &Flowchart) -> String {
        let mut mermaid = String::new();
        
        // Add flowchart declaration
        mermaid.push_str(&format!("flowchart {}\n", flowchart.direction));

        // Add title if present
        if let Some(title) = &flowchart.title {
            mermaid.push_str(&format!("    title[\"{}\"]\n", title));
        }

        // Add nodes
        for node in &flowchart.nodes {
            let shape_open = match node.shape.as_deref() {
                Some("box") => "[",
                Some("circle") => "((",
                Some("diamond") => "{",
                _ => "[",
            };
            let shape_close = match node.shape.as_deref() {
                Some("box") => "]",
                Some("circle") => "))",
                Some("diamond") => "}",
                _ => "]",
            };

            mermaid.push_str(&format!(
                "    {}{}\"{}\"{}\n",
                node.id,
                shape_open,
                node.label.replace('\n', "<br/>"),
                shape_close
            ));

            // Add styling if present
            if let Some(style) = &node.style {
                mermaid.push_str(&format!("    style {} {}\n", node.id, style));
            }

            // Add click events if URL present
            if let Some(url) = &node.url {
                mermaid.push_str(&format!("    click {} \"{}\" _blank\n", node.id, url));
            }
        }

        // Add edges
        for edge in &flowchart.edges {
            if let Some(label) = &edge.label {
                mermaid.push_str(&format!(
                    "    {} -->|\"{}\"| {}\n",
                    edge.source_id, label, edge.target_id
                ));
            } else {
                mermaid.push_str(&format!(
                    "    {} --> {}\n",
                    edge.source_id, edge.target_id
                ));
            }
        }

        mermaid
    }

    /// Convert Mermaid diagram to HTML
    fn mermaid_to_html(mermaid_diagram: &str, task_id: i64, base_url: Option<&str>) -> String {
        let _base_url = base_url.unwrap_or("http://localhost:3000");
        
        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Task {} Diagram</title>
    <script src="https://cdn.jsdelivr.net/npm/mermaid/dist/mermaid.min.js"></script>
    <style>
        body {{
            font-family: Arial, sans-serif;
            margin: 20px;
            background-color: #f5f5f5;
        }}
        .diagram-container {{
            background-color: white;
            padding: 20px;
            border-radius: 8px;
            box-shadow: 0 2px 4px rgba(0,0,0,0.1);
        }}
        h1 {{
            color: #333;
            text-align: center;
        }}
        .mermaid {{
            text-align: center;
        }}
    </style>
</head>
<body>
    <div class="diagram-container">
        <h1>Task {} Workflow Diagram</h1>
        <div class="mermaid">
{}
        </div>
    </div>
    <script>
        mermaid.initialize({{ startOnLoad: true }});
    </script>
</body>
</html>"#,
            task_id, task_id, mermaid_diagram
        )
    }

    /// Update a task diagram
    pub async fn update(
        &mut self,
        pool: &PgPool,
        diagram_data: Option<String>,
        layout_metadata: Option<JsonValue>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE tasker_task_diagrams
            SET diagram_data = COALESCE($2, diagram_data),
                layout_metadata = COALESCE($3, layout_metadata),
                generated_at = NOW(),
                updated_at = NOW()
            WHERE task_diagram_id = $1
            "#,
            self.task_diagram_id,
            diagram_data,
            layout_metadata
        )
        .execute(pool)
        .await?;

        // Update local instance
        if let Some(data) = diagram_data {
            self.diagram_data = data;
        }
        if let Some(metadata) = layout_metadata {
            self.layout_metadata = metadata;
        }
        self.generated_at = chrono::Utc::now().naive_utc();

        Ok(())
    }

    /// Delete a task diagram
    pub async fn delete(pool: &PgPool, id: i64) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_task_diagrams
            WHERE task_diagram_id = $1
            "#,
            id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete all diagrams for a task
    pub async fn delete_for_task(pool: &PgPool, task_id: i64) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_task_diagrams
            WHERE task_id = $1
            "#,
            task_id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Get diagram statistics (Rails scope: stats)
    pub async fn get_diagram_stats(pool: &PgPool) -> Result<Vec<DiagramStats>, sqlx::Error> {
        let stats = sqlx::query_as!(
            DiagramStats,
            r#"
            SELECT 
                diagram_type,
                COUNT(*) as diagram_count,
                AVG((layout_metadata ->> 'node_count')::int) as avg_nodes,
                AVG((layout_metadata ->> 'edge_count')::int) as avg_edges
            FROM tasker_task_diagrams
            GROUP BY diagram_type
            ORDER BY diagram_count DESC
            "#
        )
        .fetch_all(pool)
        .await?;

        Ok(stats)
    }

    /// Clean up diagrams for deleted tasks
    pub async fn cleanup_orphaned_diagrams(pool: &PgPool) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_task_diagrams
            WHERE task_id NOT IN (
                SELECT task_id FROM tasker_tasks
            )
            "#
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DatabaseConnection;
    use crate::models::{Task, NamedTask, TaskNamespace};
    use crate::models::task::NewTask;
    use crate::models::named_task::NewNamedTask;
    use crate::models::task_namespace::NewTaskNamespace;

    #[tokio::test]
    async fn test_task_diagram_crud() {
        let db = DatabaseConnection::new().await.expect("Failed to connect to database");
        let pool = db.pool();

        // Create test dependencies
        let namespace = TaskNamespace::create(pool, NewTaskNamespace {
            name: format!("test_namespace_{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)),
            description: None,
        }).await.expect("Failed to create namespace");

        let named_task = NamedTask::create(pool, NewNamedTask {
            name: format!("test_task_{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)),
            version: Some("1.0.0".to_string()),
            description: None,
            task_namespace_id: namespace.task_namespace_id as i64,
            configuration: None,
        }).await.expect("Failed to create named task");

        let task = Task::create(pool, NewTask {
            named_task_id: named_task.named_task_id as i32,
            requested_at: None,
            initiator: None,
            source_system: None,
            reason: None,
            bypass_steps: None,
            tags: None,
            context: Some(serde_json::json!({})),
            identity_hash: "test_hash".to_string(),
        }).await.expect("Failed to create task");

        // Test diagram creation
        let mermaid_data = "flowchart TD\n    A[Start] --> B[Process]\n    B --> C[End]";
        let new_diagram = NewTaskDiagram {
            task_id: task.task_id,
            diagram_type: Some("mermaid".to_string()),
            diagram_data: mermaid_data.to_string(),
            layout_metadata: Some(serde_json::json!({"node_count": 3, "edge_count": 2, "direction": "TD"})),
        };

        let created = TaskDiagram::create(pool, new_diagram)
            .await
            .expect("Failed to create diagram");
        assert_eq!(created.task_id, task.task_id);
        assert_eq!(created.diagram_type, "mermaid");
        assert!(created.diagram_data.contains("flowchart TD"));

        // Test find by ID
        let found = TaskDiagram::find_by_id(pool, created.task_diagram_id)
            .await
            .expect("Failed to find diagram")
            .expect("Diagram not found");
        assert_eq!(found.task_diagram_id, created.task_diagram_id);

        // Test for_task
        let for_task = TaskDiagram::for_task(pool, task.task_id)
            .await
            .expect("Failed to get diagrams for task");
        assert_eq!(for_task.len(), 1);

        // Test latest_for_task
        let latest = TaskDiagram::latest_for_task(pool, task.task_id)
            .await
            .expect("Failed to get latest diagram")
            .expect("No latest diagram");
        assert_eq!(latest.task_diagram_id, created.task_diagram_id);

        // Test by_type
        let by_type = TaskDiagram::by_type(pool, "mermaid")
            .await
            .expect("Failed to get diagrams by type");
        assert!(!by_type.is_empty());

        // Test recent
        let recent = TaskDiagram::recent(pool, Some(10))
            .await
            .expect("Failed to get recent diagrams");
        assert!(!recent.is_empty());

        // Test with_tasks
        let with_tasks = TaskDiagram::with_tasks(pool)
            .await
            .expect("Failed to get diagrams with tasks");
        assert!(!with_tasks.is_empty());

        // Test search_metadata
        let metadata_search = TaskDiagram::search_metadata(pool, "direction", "TD")
            .await
            .expect("Failed to search metadata");
        assert!(!metadata_search.is_empty());

        // Test generate_mermaid_for_task
        let generated_mermaid = TaskDiagram::generate_mermaid_for_task(pool, task.task_id)
            .await
            .expect("Failed to generate mermaid");
        assert!(generated_mermaid.contains("flowchart"));

        // Test generate_html_for_task
        let generated_html = TaskDiagram::generate_html_for_task(pool, task.task_id, Some("http://localhost:3000"))
            .await
            .expect("Failed to generate HTML");
        assert!(generated_html.contains("<!DOCTYPE html>"));
        assert!(generated_html.contains("mermaid"));

        // Test generate_json_for_task
        let generated_json = TaskDiagram::generate_json_for_task(pool, task.task_id, true)
            .await
            .expect("Failed to generate JSON");
        assert!(generated_json.contains("direction"));

        // Test update
        let mut diagram = created.clone();
        diagram.update(
            pool,
            Some("flowchart LR\n    X[Updated] --> Y[Diagram]".to_string()),
            Some(serde_json::json!({"node_count": 2, "edge_count": 1, "direction": "LR", "updated": true})),
        )
        .await
        .expect("Failed to update diagram");
        assert!(diagram.diagram_data.contains("Updated"));

        // Test get_diagram_stats
        let stats = TaskDiagram::get_diagram_stats(pool)
            .await
            .expect("Failed to get diagram stats");
        assert!(!stats.is_empty());

        // Cleanup
        TaskDiagram::delete(pool, created.task_diagram_id)
            .await
            .expect("Failed to delete diagram");
        Task::delete(pool, task.task_id)
            .await
            .expect("Failed to delete task");
        NamedTask::delete(pool, named_task.named_task_id)
            .await
            .expect("Failed to delete named task");
        TaskNamespace::delete(pool, namespace.task_namespace_id)
            .await
            .expect("Failed to delete namespace");

        db.close().await;
    }
}