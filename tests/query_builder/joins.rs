use tasker_core::query_builder::joins::{Join, JoinType};

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
