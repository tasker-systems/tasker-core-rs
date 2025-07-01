use sqlx::{PgPool, Row};
use std::env;

pub struct DatabaseConnection {
    pool: PgPool,
}

impl DatabaseConnection {
    pub async fn new() -> Result<Self, sqlx::Error> {
        let database_url = env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://tasker:tasker@localhost/tasker_rust_development".to_string());
        
        let pool = PgPool::connect(&database_url).await?;
        
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn health_check(&self) -> Result<bool, sqlx::Error> {
        let row = sqlx::query("SELECT 1 as health")
            .fetch_one(&self.pool)
            .await?;
        
        let health: i32 = row.get("health");
        Ok(health == 1)
    }

    pub async fn close(self) {
        self.pool.close().await;
    }
}