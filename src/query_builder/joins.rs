/// Represents different types of SQL JOINs
#[derive(Debug, Clone)]
pub enum JoinType {
    Inner,
    Left,
    Right,
    Full,
    Cross,
}

impl JoinType {
    pub fn to_sql(&self) -> &'static str {
        match self {
            JoinType::Inner => "INNER JOIN",
            JoinType::Left => "LEFT JOIN",
            JoinType::Right => "RIGHT JOIN",
            JoinType::Full => "FULL OUTER JOIN",
            JoinType::Cross => "CROSS JOIN",
        }
    }
}

/// Represents a SQL JOIN clause
#[derive(Debug, Clone)]
pub struct Join {
    pub join_type: JoinType,
    pub table: String,
    pub on_condition: Option<String>,
    pub using_columns: Option<Vec<String>>,
}

impl Join {
    /// Create an INNER JOIN
    pub fn inner(table: &str, on_condition: &str) -> Self {
        Self {
            join_type: JoinType::Inner,
            table: table.to_string(),
            on_condition: Some(on_condition.to_string()),
            using_columns: None,
        }
    }

    /// Create a LEFT JOIN
    pub fn left(table: &str, on_condition: &str) -> Self {
        Self {
            join_type: JoinType::Left,
            table: table.to_string(),
            on_condition: Some(on_condition.to_string()),
            using_columns: None,
        }
    }

    /// Create a RIGHT JOIN
    pub fn right(table: &str, on_condition: &str) -> Self {
        Self {
            join_type: JoinType::Right,
            table: table.to_string(),
            on_condition: Some(on_condition.to_string()),
            using_columns: None,
        }
    }

    /// Create a FULL OUTER JOIN
    pub fn full(table: &str, on_condition: &str) -> Self {
        Self {
            join_type: JoinType::Full,
            table: table.to_string(),
            on_condition: Some(on_condition.to_string()),
            using_columns: None,
        }
    }

    /// Create a CROSS JOIN
    pub fn cross(table: &str) -> Self {
        Self {
            join_type: JoinType::Cross,
            table: table.to_string(),
            on_condition: None,
            using_columns: None,
        }
    }

    /// Create a JOIN with USING clause
    pub fn using_columns(join_type: JoinType, table: &str, columns: &[&str]) -> Self {
        Self {
            join_type,
            table: table.to_string(),
            on_condition: None,
            using_columns: Some(columns.iter().map(|c| c.to_string()).collect()),
        }
    }

    /// Convert to SQL string
    pub fn to_sql(&self) -> String {
        let mut sql = format!("{} {}", self.join_type.to_sql(), self.table);

        if let Some(ref condition) = self.on_condition {
            sql.push_str(&format!(" ON {}", condition));
        } else if let Some(ref columns) = self.using_columns {
            sql.push_str(&format!(" USING ({})", columns.join(", ")));
        }

        sql
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inner_join() {
        let join = Join::inner("users u", "u.id = posts.user_id");
        assert_eq!(join.to_sql(), "INNER JOIN users u ON u.id = posts.user_id");
    }

    #[test]
    fn test_left_join() {
        let join = Join::left("profiles p", "p.user_id = u.id");
        assert_eq!(join.to_sql(), "LEFT JOIN profiles p ON p.user_id = u.id");
    }

    #[test]
    fn test_cross_join() {
        let join = Join::cross("categories");
        assert_eq!(join.to_sql(), "CROSS JOIN categories");
    }

    #[test]
    fn test_using_join() {
        let join = Join::using_columns(JoinType::Inner, "users", &["id", "email"]);
        assert_eq!(join.to_sql(), "INNER JOIN users USING (id, email)");
    }
}
