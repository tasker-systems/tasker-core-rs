use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

/// NamedTasksNamedStep represents the join table between NamedTask and NamedStep
/// Maps to `tasker_named_tasks_named_steps` table - critical for task-step relationships (2.8KB Rails model)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, FromRow)]
pub struct NamedTasksNamedStep {
    pub id: i32,
    pub named_task_id: i32,
    pub named_step_id: i32,
    pub skippable: bool,
    pub default_retryable: bool,
    pub default_retry_limit: i32,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// New NamedTasksNamedStep for creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewNamedTasksNamedStep {
    pub named_task_id: i32,
    pub named_step_id: i32,
    pub skippable: Option<bool>,
    pub default_retryable: Option<bool>,
    pub default_retry_limit: Option<i32>,
}

/// Template structure for associate_named_step_with_named_task method
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepTemplate {
    pub name: String,
    pub dependent_system: String,
    pub default_retry_limit: i32,
    pub default_retryable: bool,
    pub skippable: bool,
}

/// Association options for find_or_create
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssociationOptions {
    pub default_retry_limit: i32,
    pub default_retryable: bool,
    pub skippable: bool,
}

impl Default for AssociationOptions {
    fn default() -> Self {
        Self {
            default_retry_limit: 3,
            default_retryable: true,
            skippable: false,
        }
    }
}

impl NamedTasksNamedStep {
    /// Create a new named task-step association
    pub async fn create(pool: &PgPool, new_association: NewNamedTasksNamedStep) -> Result<NamedTasksNamedStep, sqlx::Error> {
        let association = sqlx::query_as!(
            NamedTasksNamedStep,
            r#"
            INSERT INTO tasker_named_tasks_named_steps 
            (named_task_id, named_step_id, skippable, default_retryable, default_retry_limit)
            VALUES ($1, $2, $3, $4, $5)
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

    /// Find an association by ID
    pub async fn find_by_id(pool: &PgPool, id: i32) -> Result<Option<NamedTasksNamedStep>, sqlx::Error> {
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

    /// Find or create an association (Rails find_or_create method)
    pub async fn find_or_create(
        pool: &PgPool,
        named_task_id: i32,
        named_step_id: i32,
        options: Option<AssociationOptions>,
    ) -> Result<NamedTasksNamedStep, sqlx::Error> {
        // First try to find existing association
        if let Some(existing) = Self::find_by_task_and_step(pool, named_task_id, named_step_id).await? {
            return Ok(existing);
        }

        // Create new association if not found
        let opts = options.unwrap_or_default();
        let new_association = NewNamedTasksNamedStep {
            named_task_id,
            named_step_id,
            skippable: Some(opts.skippable),
            default_retryable: Some(opts.default_retryable),
            default_retry_limit: Some(opts.default_retry_limit),
        };

        Self::create(pool, new_association).await
    }

    /// Find association by named_task_id and named_step_id
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

    /// Get all named steps for a named task (Rails scope equivalent)
    pub async fn named_steps_for_named_task(pool: &PgPool, named_task_id: i32) -> Result<Vec<NamedTasksNamedStep>, sqlx::Error> {
        let associations = sqlx::query_as!(
            NamedTasksNamedStep,
            r#"
            SELECT ntns.id, ntns.named_task_id, ntns.named_step_id, ntns.skippable, 
                   ntns.default_retryable, ntns.default_retry_limit, ntns.created_at, ntns.updated_at
            FROM tasker_named_tasks_named_steps ntns
            WHERE ntns.named_task_id = $1
            ORDER BY ntns.id
            "#,
            named_task_id
        )
        .fetch_all(pool)
        .await?;

        Ok(associations)
    }

    /// Get named task-step associations with step names
    pub async fn get_with_step_details(pool: &PgPool, named_task_id: i32) -> Result<Vec<(NamedTasksNamedStep, String)>, sqlx::Error> {
        let results = sqlx::query!(
            r#"
            SELECT ntns.id, ntns.named_task_id, ntns.named_step_id, ntns.skippable, 
                   ntns.default_retryable, ntns.default_retry_limit, ntns.created_at, ntns.updated_at,
                   ns.name as step_name
            FROM tasker_named_tasks_named_steps ntns
            INNER JOIN tasker_named_steps ns ON ns.named_step_id = ntns.named_step_id
            WHERE ntns.named_task_id = $1
            ORDER BY ntns.id
            "#,
            named_task_id
        )
        .fetch_all(pool)
        .await?;

        let associations = results
            .into_iter()
            .map(|row| {
                let association = NamedTasksNamedStep {
                    id: row.id,
                    named_task_id: row.named_task_id,
                    named_step_id: row.named_step_id,
                    skippable: row.skippable,
                    default_retryable: row.default_retryable,
                    default_retry_limit: row.default_retry_limit,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                };
                (association, row.step_name)
            })
            .collect();

        Ok(associations)
    }

    /// Complex Rails method: associate_named_step_with_named_task
    /// This handles the complex logic from the Rails model with dependent system creation
    pub async fn associate_named_step_with_named_task(
        pool: &PgPool,
        named_task_id: i32,
        template: &StepTemplate,
    ) -> Result<NamedTasksNamedStep, sqlx::Error> {
        // Start a transaction to ensure atomicity
        let mut tx = pool.begin().await?;

        // First check if association already exists by step name
        let existing = sqlx::query_as!(
            NamedTasksNamedStep,
            r#"
            SELECT ntns.id, ntns.named_task_id, ntns.named_step_id, ntns.skippable, 
                   ntns.default_retryable, ntns.default_retry_limit, ntns.created_at, ntns.updated_at
            FROM tasker_named_tasks_named_steps ntns
            INNER JOIN tasker_named_steps ns ON ns.named_step_id = ntns.named_step_id
            WHERE ntns.named_task_id = $1 AND ns.name = $2
            "#,
            named_task_id,
            template.name
        )
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(existing_association) = existing {
            tx.commit().await?;
            return Ok(existing_association);
        }

        // Find or create dependent system
        let existing_system = sqlx::query!(
            r#"
            SELECT dependent_system_id
            FROM tasker_dependent_systems 
            WHERE name = $1
            "#,
            template.dependent_system
        )
        .fetch_optional(&mut *tx)
        .await?;

        let dependent_system_id = if let Some(system) = existing_system {
            system.dependent_system_id
        } else {
            let new_system = sqlx::query!(
                r#"
                INSERT INTO tasker_dependent_systems (name)
                VALUES ($1)
                RETURNING dependent_system_id
                "#,
                template.dependent_system
            )
            .fetch_one(&mut *tx)
            .await?;
            new_system.dependent_system_id
        };

        // Find or create named step
        let existing_step = sqlx::query!(
            r#"
            SELECT named_step_id
            FROM tasker_named_steps 
            WHERE name = $1 AND dependent_system_id = $2
            "#,
            template.name,
            dependent_system_id
        )
        .fetch_optional(&mut *tx)
        .await?;

        let named_step_id = if let Some(step) = existing_step {
            step.named_step_id
        } else {
            let new_step = sqlx::query!(
                r#"
                INSERT INTO tasker_named_steps (name, dependent_system_id)
                VALUES ($1, $2)
                RETURNING named_step_id
                "#,
                template.name,
                dependent_system_id
            )
            .fetch_one(&mut *tx)
            .await?;
            new_step.named_step_id
        };

        // Create the association
        let association = sqlx::query_as!(
            NamedTasksNamedStep,
            r#"
            INSERT INTO tasker_named_tasks_named_steps 
            (named_task_id, named_step_id, skippable, default_retryable, default_retry_limit)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, named_task_id, named_step_id, skippable, default_retryable, 
                      default_retry_limit, created_at, updated_at
            "#,
            named_task_id,
            named_step_id,
            template.skippable,
            template.default_retryable,
            template.default_retry_limit
        )
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(association)
    }

    /// Update an association
    pub async fn update(
        &mut self,
        pool: &PgPool,
        skippable: Option<bool>,
        default_retryable: Option<bool>,
        default_retry_limit: Option<i32>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE tasker_named_tasks_named_steps
            SET skippable = COALESCE($2, skippable),
                default_retryable = COALESCE($3, default_retryable),
                default_retry_limit = COALESCE($4, default_retry_limit),
                updated_at = NOW()
            WHERE id = $1
            "#,
            self.id,
            skippable,
            default_retryable,
            default_retry_limit
        )
        .execute(pool)
        .await?;

        if let Some(val) = skippable {
            self.skippable = val;
        }
        if let Some(val) = default_retryable {
            self.default_retryable = val;
        }
        if let Some(val) = default_retry_limit {
            self.default_retry_limit = val;
        }

        Ok(())
    }

    /// Delete an association
    pub async fn delete(pool: &PgPool, id: i32) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_named_tasks_named_steps
            WHERE id = $1
            "#,
            id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Delete association by task and step
    pub async fn delete_by_task_and_step(
        pool: &PgPool,
        named_task_id: i32,
        named_step_id: i32,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            DELETE FROM tasker_named_tasks_named_steps
            WHERE named_task_id = $1 AND named_step_id = $2
            "#,
            named_task_id,
            named_step_id
        )
        .execute(pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Get task name (Rails delegation method)
    pub async fn get_task_name(&self, pool: &PgPool) -> Result<Option<String>, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            SELECT nt.name
            FROM tasker_named_tasks nt
            WHERE nt.named_task_id = $1
            "#,
            self.named_task_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(result.map(|row| row.name))
    }

    /// Get step name (Rails delegation method)
    pub async fn get_step_name(&self, pool: &PgPool) -> Result<Option<String>, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            SELECT ns.name
            FROM tasker_named_steps ns
            WHERE ns.named_step_id = $1
            "#,
            self.named_step_id
        )
        .fetch_optional(pool)
        .await?;

        Ok(result.map(|row| row.name))
    }

    /// Get all associations for a named step
    pub async fn list_by_named_step(pool: &PgPool, named_step_id: i32) -> Result<Vec<NamedTasksNamedStep>, sqlx::Error> {
        let associations = sqlx::query_as!(
            NamedTasksNamedStep,
            r#"
            SELECT id, named_task_id, named_step_id, skippable, default_retryable, 
                   default_retry_limit, created_at, updated_at
            FROM tasker_named_tasks_named_steps
            WHERE named_step_id = $1
            ORDER BY named_task_id
            "#,
            named_step_id
        )
        .fetch_all(pool)
        .await?;

        Ok(associations)
    }

    /// Count associations for a named task
    pub async fn count_for_task(pool: &PgPool, named_task_id: i32) -> Result<i64, sqlx::Error> {
        let count = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM tasker_named_tasks_named_steps
            WHERE named_task_id = $1
            "#,
            named_task_id
        )
        .fetch_one(pool)
        .await?
        .count.unwrap_or(0);

        Ok(count)
    }

    /// Check if an association exists
    pub async fn exists(pool: &PgPool, named_task_id: i32, named_step_id: i32) -> Result<bool, sqlx::Error> {
        let count = sqlx::query!(
            r#"
            SELECT COUNT(*) as count
            FROM tasker_named_tasks_named_steps
            WHERE named_task_id = $1 AND named_step_id = $2
            "#,
            named_task_id,
            named_step_id
        )
        .fetch_one(pool)
        .await?
        .count.unwrap_or(0);

        Ok(count > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::DatabaseConnection;

    #[tokio::test]
    async fn test_named_tasks_named_step_crud() {
        let db = DatabaseConnection::new().await.expect("Failed to connect to database");
        let pool = db.pool();

        // Test find_or_create
        let association1 = NamedTasksNamedStep::find_or_create(pool, 1, 1, None)
            .await
            .expect("Failed to create association");
        assert_eq!(association1.named_task_id, 1);
        assert_eq!(association1.named_step_id, 1);
        assert!(!association1.skippable); // Default value
        assert!(association1.default_retryable); // Default value
        assert_eq!(association1.default_retry_limit, 3); // Default value

        // Test find_or_create again (should find existing)
        let association2 = NamedTasksNamedStep::find_or_create(pool, 1, 1, None)
            .await
            .expect("Failed to find existing association");
        assert_eq!(association1.id, association2.id);

        // Test find_by_task_and_step
        let found = NamedTasksNamedStep::find_by_task_and_step(pool, 1, 1)
            .await
            .expect("Failed to find association")
            .expect("Association not found");
        assert_eq!(found.id, association1.id);

        // Test named_steps_for_named_task
        let associations = NamedTasksNamedStep::named_steps_for_named_task(pool, 1)
            .await
            .expect("Failed to get associations for task");
        assert!(!associations.is_empty());

        // Test update
        let mut association = association1.clone();
        association.update(pool, Some(true), Some(false), Some(5))
            .await
            .expect("Failed to update association");
        assert!(association.skippable);
        assert!(!association.default_retryable);
        assert_eq!(association.default_retry_limit, 5);

        // Test deletion
        let deleted = NamedTasksNamedStep::delete(pool, association1.id)
            .await
            .expect("Failed to delete association");
        assert!(deleted);

        db.close().await;
    }

    #[test]
    fn test_association_options_default() {
        let opts = AssociationOptions::default();
        assert_eq!(opts.default_retry_limit, 3);
        assert!(opts.default_retryable);
        assert!(!opts.skippable);
    }

    #[test]
    fn test_step_template_structure() {
        let template = StepTemplate {
            name: "test_step".to_string(),
            dependent_system: "test_system".to_string(),
            default_retry_limit: 5,
            default_retryable: false,
            skippable: true,
        };

        assert_eq!(template.name, "test_step");
        assert_eq!(template.dependent_system, "test_system");
        assert_eq!(template.default_retry_limit, 5);
        assert!(!template.default_retryable);
        assert!(template.skippable);
    }
}