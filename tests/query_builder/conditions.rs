use tasker_core::query_builder::conditions::*;

#[test]
fn test_simple_condition() {
    let condition = Condition::Simple {
        field: "name".to_string(),
        operator: "=".to_string(),
        value: serde_json::Value::String("test".to_string()),
    };
    assert_eq!(condition.to_sql(), "name = 'test'");
}

#[test]
fn test_in_condition() {
    let condition = Condition::In {
        field: "id".to_string(),
        values: vec![
            serde_json::Value::Number(serde_json::Number::from(1)),
            serde_json::Value::Number(serde_json::Number::from(2)),
            serde_json::Value::Number(serde_json::Number::from(3)),
        ],
    };
    assert_eq!(condition.to_sql(), "id IN (1, 2, 3)");
}

#[test]
fn test_json_contains_condition() {
    let condition = Condition::JsonContains {
        field: "metadata".to_string(),
        value: serde_json::json!({"key": "value"}),
    };
    assert_eq!(
        condition.to_sql(),
        "metadata @> '{\"key\":\"value\"}'::jsonb"
    );
}

#[test]
fn test_exists_condition() {
    let condition = Condition::Exists {
        subquery: "SELECT 1 FROM related_table WHERE id = outer.id".to_string(),
    };
    assert_eq!(
        condition.to_sql(),
        "EXISTS (SELECT 1 FROM related_table WHERE id = outer.id)"
    );
}
