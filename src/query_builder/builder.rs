use super::{Join, Pagination, WhereClause};
use sqlx::{PgPool, Row};
use std::collections::HashMap;

/// Main query builder for complex SQL operations
/// Supports ActiveRecord-style scopes with advanced PostgreSQL features
#[derive(Debug, Clone)]
pub struct QueryBuilder {
    base_table: String,
    select_fields: Vec<String>,
    joins: Vec<Join>,
    where_clauses: Vec<WhereClause>,
    group_by: Vec<String>,
    having: Vec<WhereClause>,
    order_by: Vec<String>,
    pagination: Option<Pagination>,
    with_clauses: Vec<String>, // For CTEs
    distinct_on: Vec<String>,  // For window functions
    #[allow(dead_code)]
    parameters: HashMap<String, sqlx::types::Json<serde_json::Value>>,
    #[allow(dead_code)]
    parameter_count: usize,
}

impl QueryBuilder {
    /// Create a new query builder for the given table
    pub fn new(table: &str) -> Self {
        Self {
            base_table: table.to_string(),
            select_fields: vec!["*".to_string()],
            joins: Vec::new(),
            where_clauses: Vec::new(),
            group_by: Vec::new(),
            having: Vec::new(),
            order_by: Vec::new(),
            pagination: None,
            with_clauses: Vec::new(),
            distinct_on: Vec::new(),
            parameters: HashMap::new(),
            parameter_count: 0,
        }
    }

    /// Set specific fields to select
    pub fn select(mut self, fields: &[&str]) -> Self {
        self.select_fields = fields.iter().map(|f| f.to_string()).collect();
        self
    }

    /// Add a JOIN clause
    pub fn join(mut self, join: Join) -> Self {
        self.joins.push(join);
        self
    }

    /// Add an INNER JOIN
    pub fn inner_join(self, table: &str, on_condition: &str) -> Self {
        self.join(Join::inner(table, on_condition))
    }

    /// Add a LEFT JOIN
    pub fn left_join(self, table: &str, on_condition: &str) -> Self {
        self.join(Join::left(table, on_condition))
    }

    /// Add a WHERE clause
    pub fn where_clause(mut self, clause: WhereClause) -> Self {
        self.where_clauses.push(clause);
        self
    }

    /// Add a simple WHERE condition
    pub fn where_eq(self, field: &str, value: serde_json::Value) -> Self {
        self.where_clause(WhereClause::simple(field, "=", value))
    }

    /// Add WHERE IN condition
    pub fn where_in(self, field: &str, values: Vec<serde_json::Value>) -> Self {
        self.where_clause(WhereClause::in_condition(field, values))
    }

    /// Add WHERE EXISTS subquery
    pub fn where_exists(self, subquery: &str) -> Self {
        self.where_clause(WhereClause::exists(subquery))
    }

    /// Add WHERE NOT EXISTS subquery
    pub fn where_not_exists(self, subquery: &str) -> Self {
        self.where_clause(WhereClause::not_exists(subquery))
    }

    /// Add JSONB query condition (->>, ?, @>, etc.)
    pub fn where_jsonb(self, field: &str, operator: &str, value: serde_json::Value) -> Self {
        self.where_clause(WhereClause::jsonb(field, operator, value))
    }

    /// Add GROUP BY clause
    pub fn group_by(mut self, fields: &[&str]) -> Self {
        self.group_by.extend(fields.iter().map(|f| f.to_string()));
        self
    }

    /// Add HAVING clause
    pub fn having_clause(mut self, clause: WhereClause) -> Self {
        self.having.push(clause);
        self
    }

    /// Add ORDER BY clause
    pub fn order_by(mut self, field: &str, direction: &str) -> Self {
        self.order_by.push(format!("{} {}", field, direction));
        self
    }

    /// Add ORDER BY ASC
    pub fn order_asc(self, field: &str) -> Self {
        self.order_by(field, "ASC")
    }

    /// Add ORDER BY DESC
    pub fn order_desc(self, field: &str) -> Self {
        self.order_by(field, "DESC")
    }

    /// Add pagination (LIMIT/OFFSET)
    pub fn paginate(mut self, page: u32, per_page: u32) -> Self {
        self.pagination = Some(Pagination::new(page, per_page));
        self
    }

    /// Add LIMIT clause
    pub fn limit(mut self, limit: u32) -> Self {
        if let Some(ref mut pagination) = self.pagination {
            pagination.limit = Some(limit);
        } else {
            self.pagination = Some(Pagination::limit_only(limit));
        }
        self
    }

    /// Add OFFSET clause
    pub fn offset(mut self, offset: u32) -> Self {
        if let Some(ref mut pagination) = self.pagination {
            pagination.offset = Some(offset);
        } else {
            self.pagination = Some(Pagination::offset_only(offset));
        }
        self
    }

    /// Add WITH clause for CTEs (Common Table Expressions)
    pub fn with_cte(mut self, name: &str, query: &str) -> Self {
        self.with_clauses.push(format!("{} AS ({})", name, query));
        self
    }

    /// Add recursive CTE
    pub fn with_recursive_cte(
        mut self,
        name: &str,
        base_query: &str,
        recursive_query: &str,
    ) -> Self {
        let cte = format!("{} AS ({} UNION ALL {})", name, base_query, recursive_query);
        self.with_clauses.push(cte);
        self
    }

    /// Add DISTINCT ON clause (PostgreSQL window function)
    pub fn distinct_on(mut self, fields: &[&str]) -> Self {
        self.distinct_on
            .extend(fields.iter().map(|f| f.to_string()));
        self
    }

    /// Build the complete SQL query string
    pub fn build_sql(&self) -> String {
        let mut sql = String::new();

        // WITH clauses (CTEs)
        if !self.with_clauses.is_empty() {
            sql.push_str("WITH ");
            if self
                .with_clauses
                .iter()
                .any(|cte| cte.contains("UNION ALL"))
            {
                sql.push_str("RECURSIVE ");
            }
            sql.push_str(&self.with_clauses.join(", "));
            sql.push(' ');
        }

        // SELECT clause
        sql.push_str("SELECT ");

        // DISTINCT ON
        if !self.distinct_on.is_empty() {
            sql.push_str(&format!("DISTINCT ON ({}) ", self.distinct_on.join(", ")));
        }

        sql.push_str(&self.select_fields.join(", "));

        // FROM clause
        sql.push_str(&format!(" FROM {}", self.base_table));

        // JOIN clauses
        for join in &self.joins {
            sql.push(' ');
            sql.push_str(&join.to_sql());
        }

        // WHERE clauses
        if !self.where_clauses.is_empty() {
            sql.push_str(" WHERE ");
            let where_parts: Vec<String> = self
                .where_clauses
                .iter()
                .map(|clause| clause.to_sql())
                .collect();
            sql.push_str(&where_parts.join(" AND "));
        }

        // GROUP BY
        if !self.group_by.is_empty() {
            sql.push_str(&format!(" GROUP BY {}", self.group_by.join(", ")));
        }

        // HAVING
        if !self.having.is_empty() {
            sql.push_str(" HAVING ");
            let having_parts: Vec<String> =
                self.having.iter().map(|clause| clause.to_sql()).collect();
            sql.push_str(&having_parts.join(" AND "));
        }

        // ORDER BY
        if !self.order_by.is_empty() {
            sql.push_str(&format!(" ORDER BY {}", self.order_by.join(", ")));
        }

        // LIMIT/OFFSET
        if let Some(ref pagination) = self.pagination {
            sql.push_str(&pagination.to_sql());
        }

        sql
    }

    /// Execute the query and return all rows
    pub async fn fetch_all<T>(&self, pool: &PgPool) -> Result<Vec<T>, sqlx::Error>
    where
        T: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin,
    {
        let sql = self.build_sql();
        sqlx::query_as::<_, T>(&sql).fetch_all(pool).await
    }

    /// Execute the query and return one row
    pub async fn fetch_one<T>(&self, pool: &PgPool) -> Result<T, sqlx::Error>
    where
        T: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin,
    {
        let sql = self.build_sql();
        sqlx::query_as::<_, T>(&sql).fetch_one(pool).await
    }

    /// Execute the query and return optional row
    pub async fn fetch_optional<T>(&self, pool: &PgPool) -> Result<Option<T>, sqlx::Error>
    where
        T: for<'r> sqlx::FromRow<'r, sqlx::postgres::PgRow> + Send + Unpin,
    {
        let sql = self.build_sql();
        sqlx::query_as::<_, T>(&sql).fetch_optional(pool).await
    }

    /// Execute count query
    pub async fn count(&self, pool: &PgPool) -> Result<i64, sqlx::Error> {
        let mut count_builder = self.clone();
        count_builder.select_fields = vec!["COUNT(*)".to_string()];
        count_builder.order_by.clear();
        count_builder.pagination = None;
        count_builder.distinct_on.clear();

        let sql = count_builder.build_sql();
        let row = sqlx::query(&sql).fetch_one(pool).await?;

        Ok(row.get::<i64, _>(0))
    }

    /// Check if any rows exist
    pub async fn exists(&self, pool: &PgPool) -> Result<bool, sqlx::Error> {
        let count = self.clone().limit(1).count(pool).await?;
        Ok(count > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_query_building() {
        let query = QueryBuilder::new("tasker_tasks")
            .select(&["task_id", "context", "named_task_id"])
            .where_eq(
                "named_task_id",
                serde_json::Value::Number(serde_json::Number::from(1)),
            )
            .order_desc("created_at")
            .limit(10);

        let sql = query.build_sql();
        assert!(sql.contains("SELECT task_id, context, named_task_id"));
        assert!(sql.contains("FROM tasker_tasks"));
        assert!(sql.contains("ORDER BY created_at DESC"));
        assert!(sql.contains("LIMIT 10"));
    }

    #[test]
    fn test_join_query_building() {
        let query = QueryBuilder::new("tasker_tasks t")
            .inner_join(
                "tasker_named_tasks nt",
                "t.named_task_id = nt.named_task_id",
            )
            .left_join(
                "tasker_task_namespaces tn",
                "nt.task_namespace_id = tn.task_namespace_id",
            )
            .where_eq("tn.name", serde_json::Value::String("default".to_string()));

        let sql = query.build_sql();
        assert!(sql.contains("INNER JOIN tasker_named_tasks nt"));
        assert!(sql.contains("LEFT JOIN tasker_task_namespaces tn"));
    }

    #[test]
    fn test_cte_query_building() {
        let query = QueryBuilder::new("current_transitions")
            .with_cte(
                "current_transitions",
                "SELECT DISTINCT ON (task_id) task_id, to_state FROM tasker_task_transitions ORDER BY task_id, sort_key DESC"
            )
            .inner_join("tasker_tasks t", "current_transitions.task_id = t.task_id");

        let sql = query.build_sql();
        assert!(sql.contains("WITH current_transitions AS"));
        assert!(sql.contains("DISTINCT ON (task_id)"));
    }
}
