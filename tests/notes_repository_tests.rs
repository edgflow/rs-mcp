//! Repository tests against PostgreSQL. `#[sqlx::test]` creates a fresh,
//! migrated database for every test (on the server in DATABASE_URL) and drops
//! it afterwards, so these never touch real data.

use rs_mcp::db::notes_repository::{
    Note, create_note, delete_note, get_note, list_notes, search_notes,
};
use sqlx::PgPool;
use uuid::Uuid;

#[sqlx::test]
async fn creates_and_gets_a_note(pool: PgPool) -> sqlx::Result<()> {
    let created = create_note(&pool, "Groceries", "eggs, milk").await?;
    assert_eq!(created.title, "Groceries");
    assert_eq!(created.content, "eggs, milk");
    assert_eq!(created.created_at, created.updated_at);

    let fetched = get_note(&pool, created.id).await?;
    assert_eq!(fetched, Some(created));
    Ok(())
}

#[sqlx::test]
async fn missing_note_is_none(pool: PgPool) -> sqlx::Result<()> {
    assert_eq!(get_note(&pool, Uuid::new_v4()).await?, None);
    Ok(())
}

#[sqlx::test]
async fn lists_newest_first_in_stable_pages(pool: PgPool) -> sqlx::Result<()> {
    for i in 0..5 {
        create_note(&pool, &format!("note {i}"), "").await?;
    }
    let expected = list_notes(&pool, 100, 0).await?;
    assert_eq!(expected.len(), 5);
    // Newest first, id as tiebreak.
    let mut sorted = expected.clone();
    sorted.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));
    assert_eq!(expected, sorted);

    let mut paged: Vec<Note> = Vec::new();
    for offset in [0, 2, 4] {
        paged.extend(list_notes(&pool, 2, offset).await?);
    }
    assert_eq!(paged, expected);
    assert!(list_notes(&pool, 2, 10).await?.is_empty());
    Ok(())
}

#[sqlx::test]
async fn searches_titles_and_content(pool: PgPool) -> sqlx::Result<()> {
    create_note(&pool, "Rust MCP server", "Tools over stdio").await?;
    create_note(&pool, "Groceries", "eggs, milk, apples").await?;
    create_note(&pool, "Trip", "Pack apples for the train").await?;

    let titles = |notes: Vec<Note>| notes.into_iter().map(|n| n.title).collect::<Vec<_>>();
    assert_eq!(
        titles(search_notes(&pool, "stdio", 10).await?),
        ["Rust MCP server"]
    );
    assert_eq!(
        titles(search_notes(&pool, "apples -milk", 10).await?),
        ["Trip"]
    );
    assert_eq!(search_notes(&pool, "apples", 10).await?.len(), 2);
    assert_eq!(search_notes(&pool, "apples", 1).await?.len(), 1);
    assert!(search_notes(&pool, "bananas", 10).await?.is_empty());
    Ok(())
}

#[sqlx::test]
async fn deletes_a_note_once(pool: PgPool) -> sqlx::Result<()> {
    let note = create_note(&pool, "Temporary", "").await?;
    assert!(delete_note(&pool, note.id).await?);
    assert!(!delete_note(&pool, note.id).await?);
    assert_eq!(get_note(&pool, note.id).await?, None);
    Ok(())
}

#[sqlx::test]
async fn database_rejects_invalid_titles(pool: PgPool) -> sqlx::Result<()> {
    // The table's CHECK constraints back up the tool-level validation.
    assert!(create_note(&pool, "   ", "").await.is_err());
    assert!(create_note(&pool, &"x".repeat(201), "").await.is_err());
    Ok(())
}

#[sqlx::test]
async fn database_errors_are_returned_not_panicked(pool: PgPool) -> sqlx::Result<()> {
    pool.close().await;
    assert!(create_note(&pool, "After close", "").await.is_err());
    assert!(list_notes(&pool, 10, 0).await.is_err());
    Ok(())
}
