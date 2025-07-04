use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// NamedTasksNamedStep represents the association between NamedTask and NamedStep with configuration
/// Maps to `tasker_named_tasks_named_steps` table - junction table with step configuration
///
/// This table defines which steps belong to which tasks and includes configuration
/// for how those steps should behave (skippable, retry settings, etc.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct NamedTasksNamedStep {
    pub id: i32, // Primary key
    pub named_task_id: i32,
    pub named_step_id: i32,
    pub skippable: bool,          // Whether this step can be skipped
    pub default_retryable: bool,  // Default retry behavior for this step
    pub default_retry_limit: i32, // Default retry limit for this step
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New NamedTasksNamedStep for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewNamedTasksNamedStep {
    pub named_task_id: i32,
    pub named_step_id: i32,
    pub skippable: Option<bool>,          // Defaults to false
    pub default_retryable: Option<bool>,  // Defaults to true
    pub default_retry_limit: Option<i32>, // Defaults to 3
}

/// NamedTasksNamedStep with related step and task details
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NamedTasksNamedStepWithDetails {
    pub id: i32,
    pub named_task_id: i32,
    pub named_step_id: i32,
    pub skippable: bool,
    pub default_retryable: bool,
    pub default_retry_limit: i32,
    pub task_name: String,
    pub step_name: String,
    pub step_system_name: String, // Name of the dependent system instead
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

impl NamedTasksNamedStep {
    /// Create a new named task-step association
    pub async fn create(
        pool: &PgPool,
        new_association: NewNamedTasksNamedStep,
    ) -> Result<NamedTasksNamedStep, sqlx::Error> {
        let association = sqlx::query_as!(
            NamedTasksNamedStep,
            r#"
            INSERT INTO tasker_named_tasks_named_steps 
            (named_task_id, named_step_id, skippable, default_retryable, default_retry_limit, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, NOW(), NOW())
            RETURNING id, named_task_id, named_step_id, skippable, default_retryable, 
                      default_retry_limit, created_at, updated_at
            "#,
            new_association.named_task_id,
            new_association.named_step_id,
            new_association.skippable.unwrap_or(false),
            new_association.default_retryable.unwrap_or(true),
            new_association.default_retry_limit.unwrap_or(3)
        )
        .fetch_one(pool)
        .await?;

        Ok(association)
    }

    /// Find association by ID
    pub async fn find_by_id(
        pool: &PgPool,
        id: i32,
    ) -> Result<Option<NamedTasksNamedStep>, sqlx::Error> {
        let association = sqlx::query_as!(
            NamedTasksNamedStep,
            r#"
            SELECT id, named_task_id, named_step_id, skippable, default_retryable,
                   default_retry_limit, created_at, updated_at
            FROM tasker_named_tasks_named_steps
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(association)
    }

    /// Find association by task and step IDs (uses unique constraint)
    pub async fn find_by_task_and_step(
        pool: &PgPool,
        named_task_id: i32,
        named_step_id: i32,
    ) -> Result<Option<NamedTasksNamedStep>, sqlx::Error> {
        let association = sqlx::query_as!(
            NamedTasksNamedStep,
            r#"
            SELECT id, named_task_id, named_step_id, skippable, default_retryable,
                   default_retry_limit, created_at, updated_at
            FROM tasker_named_tasks_named_steps
            WHERE named_task_id = $1 AND named_step_id = $2
            "#,
            named_task_id,
            named_step_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(association)
    }

    /// Find all steps for a specific task
    pub async fn find_by_task(
        pool: &PgPool,
        named_task_id: i32,
    ) -> Result<Vec<NamedTasksNamedStep>, sqlx::Error> {
        let associations = sqlx::query_as!(
            NamedTasksNamedStep,
            r#"
            SELECT id, named_task_id, named_step_id, skippable, default_retryable,
                   default_retry_limit, created_at, updated_at
            FROM tasker_named_tasks_named_steps
            WHERE named_task_id = $1
            ORDER BY id
            "#,
            named_task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(associations)
    }

    /// Find all tasks for a specific step  
    pub async fn find_by_step(
        pool: &PgPool,
        named_step_id: i32,
    ) -> Result<Vec<NamedTasksNamedStep>, sqlx::Error> {
        let associations = sqlx::query_as!(
            NamedTasksNamedStep,
            r#"
            SELECT id, named_task_id, named_step_id, skippable, default_retryable,
                   default_retry_limit, created_at, updated_at
            FROM tasker_named_tasks_named_steps
            WHERE named_step_id = $1
            ORDER BY id
            "#,
            named_step_id
        )
        .fetch_all(pool)
        .await?;

        Ok(associations)
    }

    /// Find associations with task and step details
    pub async fn find_with_details(
        pool: &PgPool,
        limit: Option<i32>,
    ) -> Result<Vec<NamedTasksNamedStepWithDetails>, sqlx::Error> {
        let limit_clause = limit.unwrap_or(100);

        let associations = sqlx::query_as!(
            NamedTasksNamedStepWithDetails,
            r#"
            SELECT 
                ntns.id,
                ntns.named_task_id,
                ntns.named_step_id,
                ntns.skippable,
                ntns.default_retryable,
                ntns.default_retry_limit,
                nt.name as task_name,
                ns.name as step_name,
                ds.name as step_system_name,
                ntns.created_at,
                ntns.updated_at
            FROM tasker_named_tasks_named_steps ntns
            INNER JOIN tasker_named_tasks nt ON nt.named_task_id = ntns.named_task_id
            INNER JOIN tasker_named_steps ns ON ns.named_step_id = ntns.named_step_id
            INNER JOIN tasker_dependent_systems ds ON ds.dependent_system_id = ns.dependent_system_id
            ORDER BY ntns.id
            LIMIT $1
            "#,
            limit_clause as i64
        )
        .fetch_all(pool)
        .await?;

        Ok(associations)
    }

    /// Update association configuration
    pub async fn update(
        pool: &PgPool,
        id: i32,
        new_association: NewNamedTasksNamedStep,
    ) -> Result<Option<NamedTasksNamedStep>, sqlx::Error> {
        let association = sqlx::query_as!(
            NamedTasksNamedStep,
            r#"
            UPDATE tasker_named_tasks_named_steps 
            SET named_task_id = $2,
                named_step_id = $3,
                skippable = $4,
                default_retryable = $5,
                default_retry_limit = $6,
                updated_at = NOW()
            WHERE id = $1
            RETURNING id, named_task_id, named_step_id, skippable, default_retryable,
                      default_retry_limit, created_at, updated_at
            "#,
            id,
            new_association.named_task_id,
            new_association.named_step_id,
            new_association.skippable.unwrap_or(false),
            new_association.default_retryable.unwrap_or(true),
            new_association.default_retry_limit.unwrap_or(3)
        )
        .fetch_optional(pool)
        .await?;

        Ok(association)
    }

    /// Delete an association
    pub async fn delete(pool: &PgPool, id: i32) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            "DELETE FROM tasker_named_tasks_named_steps WHERE id = $1",
            id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Create or find existing association (idempotent)
    pub async fn find_or_create(
        pool: &PgPool,
        new_association: NewNamedTasksNamedStep,
    ) -> Result<NamedTasksNamedStep, sqlx::Error> {
        // Try to find existing association first
        if let Some(existing) = Self::find_by_task_and_step(
            pool,
            new_association.named_task_id,
            new_association.named_step_id,
        )
        .await?
        {
            return Ok(existing);
        }

        // Create new association if not found
        Self::create(pool, new_association).await
    }

    /// List all associations with pagination
    pub async fn list_all(
        pool: &PgPool,
        offset: Option<i32>,
        limit: Option<i32>,
    ) -> Result<Vec<NamedTasksNamedStep>, sqlx::Error> {
        let offset_val = offset.unwrap_or(0);
        let limit_val = limit.unwrap_or(50);

        let associations = sqlx::query_as!(
            NamedTasksNamedStep,
            r#"
            SELECT id, named_task_id, named_step_id, skippable, default_retryable,
                   default_retry_limit, created_at, updated_at
            FROM tasker_named_tasks_named_steps
            ORDER BY id
            LIMIT $1 OFFSET $2
            "#,
            limit_val as i64,
            offset_val as i64
        )
        .fetch_all(pool)
        .await?;

        Ok(associations)
    }

    /// Find skippable associations for a task
    pub async fn find_skippable_by_task(
        pool: &PgPool,
        named_task_id: i32,
    ) -> Result<Vec<NamedTasksNamedStep>, sqlx::Error> {
        let associations = sqlx::query_as!(
            NamedTasksNamedStep,
            r#"
            SELECT id, named_task_id, named_step_id, skippable, default_retryable,
                   default_retry_limit, created_at, updated_at
            FROM tasker_named_tasks_named_steps
            WHERE named_task_id = $1 AND skippable = true
            ORDER BY id
            "#,
            named_task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(associations)
    }

    /// Find non-retryable associations for a task  
    pub async fn find_non_retryable_by_task(
        pool: &PgPool,
        named_task_id: i32,
    ) -> Result<Vec<NamedTasksNamedStep>, sqlx::Error> {
        let associations = sqlx::query_as!(
            NamedTasksNamedStep,
            r#"
            SELECT id, named_task_id, named_step_id, skippable, default_retryable,
                   default_retry_limit, created_at, updated_at
            FROM tasker_named_tasks_named_steps
            WHERE named_task_id = $1 AND default_retryable = false
            ORDER BY id
            "#,
            named_task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(associations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DatabaseConnection;

    #[tokio::test]
    async fn test_named_tasks_named_step_crud() {
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

        let dependent_system = crate::models::dependent_system::DependentSystem::create(
            pool,
            crate::models::dependent_system::NewDependentSystem {
                name: format!(
                    "test_system_{}",
                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
                ),
                description: None,
            },
        )
        .await
        .expect("Failed to create dependent system");

        let named_step = crate::models::named_step::NamedStep::create(
            pool,
            crate::models::named_step::NewNamedStep {
                name: format!(
                    "test_step_{}",
                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
                ),
                description: None,
                dependent_system_id: dependent_system.dependent_system_id,
            },
        )
        .await
        .expect("Failed to create named step");

        // Test creation with custom configuration
        let new_association = NewNamedTasksNamedStep {
            named_task_id: named_task.named_task_id,
            named_step_id: named_step.named_step_id,
            skippable: Some(true),
            default_retryable: Some(false),
            default_retry_limit: Some(5),
        };

        let association = NamedTasksNamedStep::create(pool, new_association.clone())
            .await
            .expect("Failed to create association");
        assert_eq!(association.named_task_id, named_task.named_task_id);
        assert_eq!(association.skippable, true);
        assert_eq!(association.default_retryable, false);
        assert_eq!(association.default_retry_limit, 5);

        // Test find by ID
        let found = NamedTasksNamedStep::find_by_id(pool, association.id)
            .await
            .expect("Failed to find association");
        assert!(found.is_some());
        assert_eq!(found.unwrap().named_step_id, named_step.named_step_id);

        // Test find by task and step
        let found_specific = NamedTasksNamedStep::find_by_task_and_step(
            pool,
            named_task.named_task_id,
            named_step.named_step_id,
        )
        .await
        .expect("Failed to find specific association");
        assert!(found_specific.is_some());

        // Test find_or_create (should find existing)
        let found_or_created = NamedTasksNamedStep::find_or_create(pool, new_association)
            .await
            .expect("Failed to find or create");
        assert_eq!(found_or_created.id, association.id);

        // Test finding skippable associations
        let skippable = NamedTasksNamedStep::find_skippable_by_task(pool, named_task.named_task_id)
            .await
            .expect("Failed to find skippable");
        assert!(!skippable.is_empty());

        // Test finding non-retryable associations
        let non_retryable =
            NamedTasksNamedStep::find_non_retryable_by_task(pool, named_task.named_task_id)
                .await
                .expect("Failed to find non-retryable");
        assert!(!non_retryable.is_empty());

        // Cleanup
        let deleted = NamedTasksNamedStep::delete(pool, association.id)
            .await
            .expect("Failed to delete association");
        assert!(deleted);
    }

    #[tokio::test]
    async fn test_default_values() {
        let db = DatabaseConnection::new()
            .await
            .expect("Failed to connect to database");
        let pool = db.pool();

        // Create test dependencies with minimal data
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

        let dependent_system = crate::models::dependent_system::DependentSystem::create(
            pool,
            crate::models::dependent_system::NewDependentSystem {
                name: format!(
                    "test_system_{}",
                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
                ),
                description: None,
            },
        )
        .await
        .expect("Failed to create dependent system");

        let named_step = crate::models::named_step::NamedStep::create(
            pool,
            crate::models::named_step::NewNamedStep {
                name: format!(
                    "test_step_{}",
                    chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
                ),
                description: None,
                dependent_system_id: dependent_system.dependent_system_id,
            },
        )
        .await
        .expect("Failed to create named step");

        // Test creation with default values
        let new_association = NewNamedTasksNamedStep {
            named_task_id: named_task.named_task_id,
            named_step_id: named_step.named_step_id,
            skippable: None,           // Should default to false
            default_retryable: None,   // Should default to true
            default_retry_limit: None, // Should default to 3
        };

        let association = NamedTasksNamedStep::create(pool, new_association)
            .await
            .expect("Failed to create association with defaults");
        assert_eq!(association.skippable, false);
        assert_eq!(association.default_retryable, true);
        assert_eq!(association.default_retry_limit, 3);

        // Cleanup
        NamedTasksNamedStep::delete(pool, association.id)
            .await
            .expect("Failed to delete association");
    }
}
