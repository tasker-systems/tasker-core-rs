/// Represents pagination parameters for SQL queries
#[derive(Debug, Clone)]
pub struct Pagination {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

impl Pagination {
    /// Create pagination with page number and per-page count
    pub fn new(page: u32, per_page: u32) -> Self {
        let offset = if page > 0 { Some((page - 1) * per_page) } else { None };
        Self {
            limit: Some(per_page),
            offset,
        }
    }

    /// Create pagination with only limit
    pub fn limit_only(limit: u32) -> Self {
        Self {
            limit: Some(limit),
            offset: None,
        }
    }

    /// Create pagination with only offset
    pub fn offset_only(offset: u32) -> Self {
        Self {
            limit: None,
            offset: Some(offset),
        }
    }

    /// Create pagination with both limit and offset
    pub fn limit_offset(limit: u32, offset: u32) -> Self {
        Self {
            limit: Some(limit),
            offset: Some(offset),
        }
    }

    /// Convert to SQL string
    pub fn to_sql(&self) -> String {
        let mut sql = String::new();

        if let Some(limit) = self.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        if let Some(offset) = self.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        sql
    }

    /// Calculate total pages given a total count
    pub fn total_pages(&self, total_count: u32) -> u32 {
        if let Some(limit) = self.limit {
            (total_count + limit - 1) / limit // Ceiling division
        } else {
            1
        }
    }

    /// Get current page number (1-indexed)
    pub fn current_page(&self) -> u32 {
        if let (Some(limit), Some(offset)) = (self.limit, self.offset) {
            (offset / limit) + 1
        } else {
            1
        }
    }

    /// Check if there's a next page
    pub fn has_next_page(&self, total_count: u32) -> bool {
        if let (Some(limit), Some(offset)) = (self.limit, self.offset) {
            offset + limit < total_count
        } else {
            false
        }
    }

    /// Check if there's a previous page
    pub fn has_previous_page(&self) -> bool {
        self.offset.map_or(false, |offset| offset > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}