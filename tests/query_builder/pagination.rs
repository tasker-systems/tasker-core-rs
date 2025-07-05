use tasker_core::query_builder::pagination::Pagination;

#[test]
fn test_page_based_pagination() {
    let pagination = Pagination::new(2, 10); // Page 2, 10 per page
    assert_eq!(pagination.limit, Some(10));
    assert_eq!(pagination.offset, Some(10));
    assert_eq!(pagination.to_sql(), " LIMIT 10 OFFSET 10");
}

#[test]
fn test_first_page_pagination() {
    let pagination = Pagination::new(1, 20); // Page 1, 20 per page
    assert_eq!(pagination.limit, Some(20));
    assert_eq!(pagination.offset, Some(0));
    assert_eq!(pagination.to_sql(), " LIMIT 20 OFFSET 0");
}

#[test]
fn test_limit_only() {
    let pagination = Pagination::limit_only(5);
    assert_eq!(pagination.limit, Some(5));
    assert_eq!(pagination.offset, None);
    assert_eq!(pagination.to_sql(), " LIMIT 5");
}

#[test]
fn test_offset_only() {
    let pagination = Pagination::offset_only(15);
    assert_eq!(pagination.limit, None);
    assert_eq!(pagination.offset, Some(15));
    assert_eq!(pagination.to_sql(), " OFFSET 15");
}

#[test]
fn test_total_pages_calculation() {
    let pagination = Pagination::new(1, 10);
    assert_eq!(pagination.total_pages(25), 3); // 25 items, 10 per page = 3 pages
    assert_eq!(pagination.total_pages(30), 3); // 30 items, 10 per page = 3 pages
    assert_eq!(pagination.total_pages(31), 4); // 31 items, 10 per page = 4 pages
}

#[test]
fn test_current_page() {
    let pagination = Pagination::new(3, 10);
    assert_eq!(pagination.current_page(), 3);
}

#[test]
fn test_has_next_page() {
    let pagination = Pagination::new(2, 10); // Page 2, 10 per page (offset 10)
    assert!(pagination.has_next_page(25)); // 25 total items, so there's a next page
    assert!(!pagination.has_next_page(20)); // 20 total items, no next page
}

#[test]
fn test_has_previous_page() {
    let pagination1 = Pagination::new(1, 10); // Page 1
    assert!(!pagination1.has_previous_page());

    let pagination2 = Pagination::new(2, 10); // Page 2
    assert!(pagination2.has_previous_page());
}
