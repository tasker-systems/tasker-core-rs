/// Represents pagination parameters for SQL queries
#[derive(Debug, Clone)]
pub struct Pagination {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

impl Pagination {
    /// Create pagination with page number and per-page count
    pub fn new(page: u32, per_page: u32) -> Self {
        let offset = if page > 0 {
            Some((page - 1) * per_page)
        } else {
            None
        };
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
            sql.push_str(&format!(" LIMIT {limit}"));
        }

        if let Some(offset) = self.offset {
            sql.push_str(&format!(" OFFSET {offset}"));
        }

        sql
    }

    /// Calculate total pages given a total count
    pub fn total_pages(&self, total_count: u32) -> u32 {
        if let Some(limit) = self.limit {
            total_count.div_ceil(limit) // Ceiling division
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
        self.offset.is_some_and(|offset| offset > 0)
    }
}
