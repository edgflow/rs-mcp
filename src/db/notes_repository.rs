//! SQL for the notes table: plain functions over a connection pool. Input
//! rules (lengths, page sizes) are checked by the callers before they get here.

use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, FromRow)]
pub struct Note {
    pub id: Uuid,
    pub title: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn create_note(pool: &PgPool, title: &str, content: &str) -> Result<Note, sqlx::Error> {
    sqlx::query_as(
        "INSERT INTO notes (title, content) VALUES ($1, $2)
         RETURNING id, title, content, created_at, updated_at",
    )
    .bind(title)
    .bind(content)
    .fetch_one(pool)
    .await
}

pub async fn get_note(pool: &PgPool, id: Uuid) -> Result<Option<Note>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, title, content, created_at, updated_at
         FROM notes WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// Newest first; the id breaks ties so pagination is stable.
pub async fn list_notes(pool: &PgPool, limit: i64, offset: i64) -> Result<Vec<Note>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, title, content, created_at, updated_at
         FROM notes
         ORDER BY created_at DESC, id DESC
         LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

/// Full-text search over title and content, best matches first. `query`
/// uses web-search syntax: words, "quoted phrases", OR, and -excluded.
pub async fn search_notes(
    pool: &PgPool,
    query: &str,
    limit: i64,
) -> Result<Vec<Note>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, title, content, created_at, updated_at
         FROM notes, websearch_to_tsquery('english', $1) AS q
         WHERE search @@ q
         ORDER BY ts_rank(search, q) DESC, created_at DESC, id DESC
         LIMIT $2",
    )
    .bind(query)
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// Returns whether a note was deleted (false when no note had that id).
pub async fn delete_note(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM notes WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() == 1)
}
