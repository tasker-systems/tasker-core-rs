/// Represents different types of SQL conditions
#[derive(Debug, Clone)]
pub enum Condition {
    Simple {
        field: String,
        operator: String,
        value: serde_json::Value,
    },
    In {
        field: String,
        values: Vec<serde_json::Value>,
    },
    NotIn {
        field: String,
        values: Vec<serde_json::Value>,
    },
    Between {
        field: String,
        start: serde_json::Value,
        end: serde_json::Value,
    },
    IsNull {
        field: String,
    },
    IsNotNull {
        field: String,
    },
    Exists {
        subquery: String,
    },
    NotExists {
        subquery: String,
    },
    JsonPath {
        field: String,
        path: String,
        operator: String,
        value: serde_json::Value,
    },
    JsonContains {
        field: String,
        value: serde_json::Value,
    },
    JsonContainedBy {
        field: String,
        value: serde_json::Value,
    },
    JsonHasKey {
        field: String,
        key: String,
    },
    JsonHasAnyKeys {
        field: String,
        keys: Vec<String>,
    },
    JsonHasAllKeys {
        field: String,
        keys: Vec<String>,
    },
    Raw {
        sql: String,
    },
}

impl Condition {
    /// Convert condition to SQL string
    pub fn to_sql(&self) -> String {
        match self {
            Condition::Simple {
                field,
                operator,
                value,
            } => {
                format!("{} {} {}", field, operator, format_value(value))
            }
            Condition::In { field, values } => {
                let value_list = values
                    .iter()
                    .map(format_value)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{field} IN ({value_list})")
            }
            Condition::NotIn { field, values } => {
                let value_list = values
                    .iter()
                    .map(format_value)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{field} NOT IN ({value_list})")
            }
            Condition::Between { field, start, end } => {
                format!(
                    "{} BETWEEN {} AND {}",
                    field,
                    format_value(start),
                    format_value(end)
                )
            }
            Condition::IsNull { field } => {
                format!("{field} IS NULL")
            }
            Condition::IsNotNull { field } => {
                format!("{field} IS NOT NULL")
            }
            Condition::Exists { subquery } => {
                format!("EXISTS ({subquery})")
            }
            Condition::NotExists { subquery } => {
                format!("NOT EXISTS ({subquery})")
            }
            Condition::JsonPath {
                field,
                path,
                operator,
                value,
            } => {
                format!(
                    "{}->>'{}' {} {}",
                    field,
                    path,
                    operator,
                    format_value(value)
                )
            }
            Condition::JsonContains { field, value } => {
                format!("{} @> {}", field, format_json_value(value))
            }
            Condition::JsonContainedBy { field, value } => {
                format!("{} <@ {}", field, format_json_value(value))
            }
            Condition::JsonHasKey { field, key } => {
                format!(
                    "{} ? {}",
                    field,
                    format_value(&serde_json::Value::String(key.clone()))
                )
            }
            Condition::JsonHasAnyKeys { field, keys } => {
                let key_array = serde_json::Value::Array(
                    keys.iter()
                        .map(|k| serde_json::Value::String(k.clone()))
                        .collect(),
                );
                format!("{} ?| {}", field, format_json_value(&key_array))
            }
            Condition::JsonHasAllKeys { field, keys } => {
                let key_array = serde_json::Value::Array(
                    keys.iter()
                        .map(|k| serde_json::Value::String(k.clone()))
                        .collect(),
                );
                format!("{} ?& {}", field, format_json_value(&key_array))
            }
            Condition::Raw { sql } => sql.clone(),
        }
    }
}

/// Represents a WHERE clause that can contain multiple conditions
#[derive(Debug, Clone)]
pub struct WhereClause {
    pub conditions: Vec<Condition>,
    pub operator: LogicalOperator,
}

#[derive(Debug, Clone)]
pub enum LogicalOperator {
    And,
    Or,
}

impl WhereClause {
    /// Create a simple WHERE clause with a single condition
    pub fn simple(field: &str, operator: &str, value: serde_json::Value) -> Self {
        Self {
            conditions: vec![Condition::Simple {
                field: field.to_string(),
                operator: operator.to_string(),
                value,
            }],
            operator: LogicalOperator::And,
        }
    }

    /// Create WHERE IN clause
    pub fn in_condition(field: &str, values: Vec<serde_json::Value>) -> Self {
        Self {
            conditions: vec![Condition::In {
                field: field.to_string(),
                values,
            }],
            operator: LogicalOperator::And,
        }
    }

    /// Create WHERE NOT IN clause
    pub fn not_in_condition(field: &str, values: Vec<serde_json::Value>) -> Self {
        Self {
            conditions: vec![Condition::NotIn {
                field: field.to_string(),
                values,
            }],
            operator: LogicalOperator::And,
        }
    }

    /// Create WHERE EXISTS clause
    pub fn exists(subquery: &str) -> Self {
        Self {
            conditions: vec![Condition::Exists {
                subquery: subquery.to_string(),
            }],
            operator: LogicalOperator::And,
        }
    }

    /// Create WHERE NOT EXISTS clause
    pub fn not_exists(subquery: &str) -> Self {
        Self {
            conditions: vec![Condition::NotExists {
                subquery: subquery.to_string(),
            }],
            operator: LogicalOperator::And,
        }
    }

    /// Create JSONB condition (for ->>, @>, ?, etc.)
    pub fn jsonb(field: &str, operator: &str, value: serde_json::Value) -> Self {
        let condition = match operator {
            "@>" => Condition::JsonContains {
                field: field.to_string(),
                value,
            },
            "<@" => Condition::JsonContainedBy {
                field: field.to_string(),
                value,
            },
            "?" => {
                if let serde_json::Value::String(key) = value {
                    Condition::JsonHasKey {
                        field: field.to_string(),
                        key,
                    }
                } else {
                    Condition::Raw {
                        sql: format!("{} ? {}", field, format_value(&value)),
                    }
                }
            }
            "?|" => {
                if let serde_json::Value::Array(keys) = value {
                    let string_keys: Vec<String> = keys
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();
                    Condition::JsonHasAnyKeys {
                        field: field.to_string(),
                        keys: string_keys,
                    }
                } else {
                    Condition::Raw {
                        sql: format!("{} ?| {}", field, format_value(&value)),
                    }
                }
            }
            "?&" => {
                if let serde_json::Value::Array(keys) = value {
                    let string_keys: Vec<String> = keys
                        .iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect();
                    Condition::JsonHasAllKeys {
                        field: field.to_string(),
                        keys: string_keys,
                    }
                } else {
                    Condition::Raw {
                        sql: format!("{} ?& {}", field, format_value(&value)),
                    }
                }
            }
            _ => Condition::Raw {
                sql: format!("{} {} {}", field, operator, format_value(&value)),
            },
        };

        Self {
            conditions: vec![condition],
            operator: LogicalOperator::And,
        }
    }

    /// Create WHERE IS NULL clause
    pub fn is_null(field: &str) -> Self {
        Self {
            conditions: vec![Condition::IsNull {
                field: field.to_string(),
            }],
            operator: LogicalOperator::And,
        }
    }

    /// Create WHERE IS NOT NULL clause
    pub fn is_not_null(field: &str) -> Self {
        Self {
            conditions: vec![Condition::IsNotNull {
                field: field.to_string(),
            }],
            operator: LogicalOperator::And,
        }
    }

    /// Create raw SQL condition
    pub fn raw(sql: &str) -> Self {
        Self {
            conditions: vec![Condition::Raw {
                sql: sql.to_string(),
            }],
            operator: LogicalOperator::And,
        }
    }

    /// Combine multiple conditions with AND
    pub fn and(conditions: Vec<Condition>) -> Self {
        Self {
            conditions,
            operator: LogicalOperator::And,
        }
    }

    /// Combine multiple conditions with OR
    pub fn or(conditions: Vec<Condition>) -> Self {
        Self {
            conditions,
            operator: LogicalOperator::Or,
        }
    }

    /// Convert to SQL string
    pub fn to_sql(&self) -> String {
        if self.conditions.is_empty() {
            return "1=1".to_string();
        }

        if self.conditions.len() == 1 {
            return self.conditions[0].to_sql();
        }

        let operator_str = match self.operator {
            LogicalOperator::And => " AND ",
            LogicalOperator::Or => " OR ",
        };

        let condition_sqls: Vec<String> = self.conditions.iter().map(|c| c.to_sql()).collect();

        format!("({})", condition_sqls.join(operator_str))
    }
}

/// Format a JSON value for SQL
fn format_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::Null => "NULL".to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => format!("'{}'", s.replace('\'', "''")),
        _ => format!("'{}'", value.to_string().replace('\'', "''")),
    }
}

/// Format a JSON value for JSONB operations
fn format_json_value(value: &serde_json::Value) -> String {
    format!("'{}'::jsonb", value.to_string().replace('\'', "''"))
}
