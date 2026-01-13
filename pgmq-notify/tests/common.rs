use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::collections::HashMap;
use uuid::Uuid;

/// Test database utilities for pgmq-notify integration tests
#[derive(Debug)]
pub struct TestDb {
    pub pool: PgPool,
    pub test_id: String,
    pub created_queues: Vec<String>,
}

impl TestDb {
    /// Create a new test database connection with unique test ID
    ///
    /// TAS-78: In split-database mode, PGMQ operations need a separate database.
    /// This helper uses PGMQ_DATABASE_URL when set, falling back to DATABASE_URL.
    pub async fn new() -> Result<Self, sqlx::Error> {
        // TAS-78: Prefer PGMQ_DATABASE_URL for PGMQ tests (split-db mode)
        let database_url = std::env::var("PGMQ_DATABASE_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| std::env::var("DATABASE_URL").ok())
            .unwrap_or_else(|| {
                "postgresql://tasker:tasker@localhost:5432/tasker_rust_test".to_string()
            });

        let pool = PgPool::connect(&database_url).await?;
        let test_id = Uuid::new_v4().to_string()[..8].to_string();

        Ok(TestDb {
            pool,
            test_id,
            created_queues: Vec::new(),
        })
    }

    /// Create a test queue with unique name
    pub async fn create_test_queue(
        &mut self,
        queue_base_name: &str,
    ) -> Result<String, sqlx::Error> {
        let queue_name = format!("{}_{}", queue_base_name, self.test_id);

        sqlx::query("SELECT pgmq.create($1)")
            .bind(&queue_name)
            .execute(&self.pool)
            .await?;

        self.created_queues.push(queue_name.clone());
        Ok(queue_name)
    }

    /// Send a single message using the wrapper function
    pub async fn send_message(
        &self,
        queue_name: &str,
        message: Value,
        delay_seconds: i32,
    ) -> Result<i64, sqlx::Error> {
        let row = sqlx::query("SELECT pgmq_send_with_notify($1, $2, $3) as msg_id")
            .bind(queue_name)
            .bind(message)
            .bind(delay_seconds)
            .fetch_one(&self.pool)
            .await?;

        Ok(row.get("msg_id"))
    }

    /// Send batch messages using the wrapper function
    pub async fn send_batch_messages(
        &self,
        queue_name: &str,
        messages: Vec<Value>,
        delay_seconds: i32,
    ) -> Result<Vec<i64>, sqlx::Error> {
        let rows = sqlx::query("SELECT pgmq_send_batch_with_notify($1, $2, $3)")
            .bind(queue_name)
            .bind(messages)
            .bind(delay_seconds)
            .fetch_all(&self.pool)
            .await?;

        let msg_ids: Vec<i64> = rows.iter().map(|row| row.get::<i64, usize>(0)).collect();

        Ok(msg_ids)
    }

    /// Get message count in a queue
    pub async fn get_message_count(&self, queue_name: &str) -> Result<i64, sqlx::Error> {
        let table_name = format!("pgmq.q_{}", queue_name);
        let query = format!("SELECT COUNT(*) as count FROM {}", table_name);

        let row = sqlx::query(&query).fetch_one(&self.pool).await?;

        Ok(row.get("count"))
    }

    /// Test namespace extraction for various queue patterns
    #[expect(dead_code, reason = "Test helper for pgmq queue namespace extraction tests")]
    pub async fn test_namespace_extraction(
        &self,
        queue_names: Vec<&str>,
    ) -> Result<HashMap<String, String>, sqlx::Error> {
        let mut results = HashMap::new();

        for queue_name in queue_names {
            let row = sqlx::query("SELECT extract_queue_namespace($1) as namespace")
                .bind(queue_name)
                .fetch_one(&self.pool)
                .await?;

            let namespace: String = row.get("namespace");
            results.insert(queue_name.to_string(), namespace);
        }

        Ok(results)
    }

    /// Read messages from a queue (for verification)
    #[expect(dead_code, reason = "Test helper for reading messages during integration tests")]
    pub async fn read_messages(
        &self,
        queue_name: &str,
        vt: i32,
        qty: i32,
    ) -> Result<Vec<(i64, Value)>, sqlx::Error> {
        let rows = sqlx::query("SELECT msg_id, message FROM pgmq.read($1, $2, $3)")
            .bind(queue_name)
            .bind(vt)
            .bind(qty)
            .fetch_all(&self.pool)
            .await?;

        let mut messages = Vec::new();
        for row in rows {
            let msg_id: i64 = row.get("msg_id");
            let message: Value = row.get("message");
            messages.push((msg_id, message));
        }

        Ok(messages)
    }

    /// Get notification channel for a queue
    pub fn get_notification_channel(&self, queue_name: &str) -> String {
        // This mirrors the logic in our wrapper functions
        let namespace = self.extract_namespace_sync(queue_name);
        format!("pgmq_message_ready.{}", namespace)
    }

    /// Extract namespace synchronously (for test assertions)
    fn extract_namespace_sync(&self, queue_name: &str) -> String {
        // Remove test suffix if present (e.g., test_complete_queue_cd691ed8 -> test_complete_queue)
        let clean_name = if let Some(last_underscore) = queue_name.rfind('_') {
            let suffix = &queue_name[last_underscore + 1..];
            // If suffix looks like a random ID (8+ alphanumeric chars), remove it
            if suffix.len() >= 8 && suffix.chars().all(|c| c.is_ascii_alphanumeric()) {
                &queue_name[..last_underscore]
            } else {
                queue_name
            }
        } else {
            queue_name
        };

        // Mirror the SQL extract_queue_namespace logic
        if clean_name.starts_with("worker_") && clean_name.ends_with("_queue") {
            // worker_rust_queue -> rust
            let middle = &clean_name[7..clean_name.len() - 6];
            middle.to_string()
        } else if clean_name.ends_with("_queue") {
            // some_namespace_queue -> some_namespace
            let namespace = clean_name.strip_suffix("_queue").unwrap();
            namespace.to_string()
        } else if clean_name.starts_with("orchestration") {
            // orchestration* -> orchestration
            "orchestration".to_string()
        } else {
            // For names like "test_complete_queue" that don't fit patterns,
            // use the clean name as the namespace
            clean_name.to_string()
        }
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        // Clean up test queues
        if !self.created_queues.is_empty() {
            tokio::spawn({
                let pool = self.pool.clone();
                let queues = self.created_queues.clone();
                async move {
                    for queue in queues {
                        let _ = sqlx::query("SELECT pgmq.drop_queue($1)")
                            .bind(queue)
                            .execute(&pool)
                            .await;
                    }
                }
            });
        }
    }
}

/// Helper to create test JSON messages
#[expect(dead_code, reason = "Test helper for creating pgmq test messages")]
pub fn create_test_message(msg_type: &str, data: Value) -> Value {
    json!({
        "type": msg_type,
        "data": data,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "test_id": Uuid::new_v4().to_string()
    })
}

/// Helper to create workflow step messages
#[expect(dead_code, reason = "Test helper for creating workflow step test messages")]
pub fn create_workflow_message(workflow: &str, step: &str, payload: Value) -> Value {
    json!({
        "workflow": workflow,
        "step": step,
        "payload": payload,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "test_id": Uuid::new_v4().to_string()
    })
}

/// Helper to create orchestration command messages
#[expect(dead_code, reason = "Test helper for creating orchestration command test messages")]
pub fn create_orchestration_message(command: &str, task_id: &str, additional: Value) -> Value {
    let mut message = json!({
        "command": command,
        "task_id": task_id,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "test_id": Uuid::new_v4().to_string()
    });

    if let Value::Object(additional_obj) = additional {
        if let Value::Object(ref mut message_obj) = message {
            for (key, value) in additional_obj {
                message_obj.insert(key, value);
            }
        }
    }

    message
}
