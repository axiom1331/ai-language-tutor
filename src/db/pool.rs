use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;
use tracing::info;

pub type DbPool = PgPool;

/// Creates a PostgreSQL connection pool
pub async fn create_pool(database_url: &str) -> Result<DbPool, sqlx::Error> {
    info!("Creating database connection pool");

    let pool = PgPoolOptions::new()
        .max_connections(20)
        .min_connections(5)
        .acquire_timeout(Duration::from_secs(10))
        .idle_timeout(Duration::from_secs(300))
        .max_lifetime(Duration::from_secs(1800))
        .connect(database_url)
        .await?;

    info!("Database connection pool created successfully");
    Ok(pool)
}

/// Runs database migrations
pub async fn run_migrations(pool: &DbPool) -> Result<(), sqlx::Error> {
    info!("Running database migrations");
    sqlx::migrate!("./migrations")
        .run(pool)
        .await?;
    info!("Database migrations completed successfully");
    Ok(())
}
