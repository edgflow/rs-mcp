//! PostgreSQL access: the connection pool, migrations and the notes repository.

pub mod notes_repository;

use std::time::Duration;

use sqlx::{PgPool, postgres::PgPoolOptions};

const MAX_CONNECTIONS: u32 = 5;

/// Opens the connection pool. Fails fast when the database is unreachable.
pub async fn connect(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(MAX_CONNECTIONS)
        .acquire_timeout(Duration::from_secs(5))
        .connect(database_url)
        .await
}

/// Applies the migrations in `migrations/` that have not run yet.
pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}
