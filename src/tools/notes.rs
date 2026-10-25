//! The note tools: create_note, get_note, list_notes, search_notes and
//! delete_note. Their JSON input schemas are generated from the input
//! structs below (doc comments become field descriptions).

use rmcp::{Json, handler::server::wrapper::Parameters, tool, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::notes_repository::{self as repo, Note};
use crate::error::AppError;
use crate::server::NotesServer;

pub const MAX_TITLE_CHARS: usize = 200;
pub const MAX_CONTENT_CHARS: usize = 20_000;
pub const DEFAULT_PAGE_SIZE: i64 = 20;
pub const MAX_PAGE_SIZE: i64 = 100;
pub const MAX_OFFSET: i64 = 10_000;
pub const MAX_QUERY_CHARS: usize = 200;
pub const DEFAULT_SEARCH_LIMIT: i64 = 10;
pub const MAX_SEARCH_LIMIT: i64 = 50;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateNoteInput {
    /// Short title, 1 to 200 characters.
    pub title: String,
    /// The note's text, up to 20,000 characters. May be empty.
    #[serde(default)]
    pub content: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NoteIdInput {
    /// The note's id: a UUID as returned by create_note or list_notes.
    pub id: String,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ListNotesInput {
    /// Notes per page, 1 to 100 (default 20).
    pub limit: Option<i64>,
    /// Number of notes to skip; use next_offset from the previous page (default 0).
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchNotesInput {
    /// Words to find in titles and content. Supports "quoted phrases", OR, and -word to exclude.
    pub query: String,
    /// Maximum number of results, 1 to 50 (default 10).
    pub limit: Option<i64>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct NotePage {
    pub notes: Vec<Note>,
    pub limit: i64,
    pub offset: i64,
    /// Offset of the next page, or null on the last page.
    pub next_offset: Option<i64>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SearchResults {
    pub query: String,
    pub notes: Vec<Note>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DeleteOutcome {
    pub id: Uuid,
    /// False when no note had this id.
    pub deleted: bool,
}

#[tool_router(router = note_tools, vis = "pub(crate)")]
impl NotesServer {
    #[tool(
        description = "Create a note with a title and optional content. Returns the saved note with its id.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            open_world_hint = false
        )
    )]
    async fn create_note(
        &self,
        Parameters(input): Parameters<CreateNoteInput>,
    ) -> Result<Json<Note>, AppError> {
        let title = validate_title(&input.title)?;
        validate_content(&input.content)?;
        let note = repo::create_note(self.pool(), title, &input.content).await?;
        Ok(Json(note))
    }

    #[tool(
        description = "Get one note by its id.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn get_note(
        &self,
        Parameters(input): Parameters<NoteIdInput>,
    ) -> Result<Json<Note>, AppError> {
        let id = parse_note_id(&input.id)?;
        repo::get_note(self.pool(), id)
            .await?
            .map(Json)
            .ok_or_else(|| AppError::NotFound(format!("no note with id {id}")))
    }

    #[tool(
        description = "List notes, newest first, one page at a time. Pass next_offset back as offset to get the next page.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn list_notes(
        &self,
        Parameters(input): Parameters<ListNotesInput>,
    ) -> Result<Json<NotePage>, AppError> {
        let limit = page_size(input.limit, DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE)?;
        let offset = page_offset(input.offset)?;
        // One extra row tells us whether another page exists.
        let mut notes = repo::list_notes(self.pool(), limit + 1, offset).await?;
        let has_more = notes.len() as i64 > limit;
        notes.truncate(limit as usize);
        Ok(Json(NotePage {
            notes,
            limit,
            offset,
            next_offset: has_more.then_some(offset + limit),
        }))
    }

    #[tool(
        description = "Search note titles and content (full-text, best matches first). Supports \"quoted phrases\", OR, and -word to exclude.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn search_notes(
        &self,
        Parameters(input): Parameters<SearchNotesInput>,
    ) -> Result<Json<SearchResults>, AppError> {
        let query = validate_query(&input.query)?;
        let limit = page_size(input.limit, DEFAULT_SEARCH_LIMIT, MAX_SEARCH_LIMIT)?;
        let notes = repo::search_notes(self.pool(), query, limit).await?;
        Ok(Json(SearchResults {
            query: query.to_string(),
            notes,
        }))
    }

    #[tool(
        description = "Delete a note by its id. Returns deleted: false when no note had that id.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn delete_note(
        &self,
        Parameters(input): Parameters<NoteIdInput>,
    ) -> Result<Json<DeleteOutcome>, AppError> {
        let id = parse_note_id(&input.id)?;
        let deleted = repo::delete_note(self.pool(), id).await?;
        Ok(Json(DeleteOutcome { id, deleted }))
    }
}

pub fn validate_title(title: &str) -> Result<&str, AppError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(AppError::InvalidInput("title must not be empty".into()));
    }
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(AppError::InvalidInput(format!(
            "title must be at most {MAX_TITLE_CHARS} characters"
        )));
    }
    Ok(title)
}

pub fn validate_content(content: &str) -> Result<(), AppError> {
    if content.chars().count() > MAX_CONTENT_CHARS {
        return Err(AppError::InvalidInput(format!(
            "content must be at most {MAX_CONTENT_CHARS} characters"
        )));
    }
    Ok(())
}

pub fn parse_note_id(id: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(id.trim()).map_err(|_| {
        AppError::InvalidInput(
            "id must be a UUID, for example 0b8f6a0e-5c1d-4a43-9d6e-2f4b8c7a1e90".into(),
        )
    })
}

pub fn validate_query(query: &str) -> Result<&str, AppError> {
    let query = query.trim();
    if query.is_empty() {
        return Err(AppError::InvalidInput("query must not be empty".into()));
    }
    if query.chars().count() > MAX_QUERY_CHARS {
        return Err(AppError::InvalidInput(format!(
            "query must be at most {MAX_QUERY_CHARS} characters"
        )));
    }
    Ok(query)
}

/// A page size within 1..=max, or the default when none is given.
pub fn page_size(limit: Option<i64>, default: i64, max: i64) -> Result<i64, AppError> {
    match limit {
        None => Ok(default),
        Some(n) if (1..=max).contains(&n) => Ok(n),
        Some(_) => Err(AppError::InvalidInput(format!(
            "limit must be between 1 and {max}"
        ))),
    }
}

pub fn page_offset(offset: Option<i64>) -> Result<i64, AppError> {
    match offset {
        None => Ok(0),
        Some(n) if (0..=MAX_OFFSET).contains(&n) => Ok(n),
        Some(_) => Err(AppError::InvalidInput(format!(
            "offset must be between 0 and {MAX_OFFSET}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invalid(result: Result<impl std::fmt::Debug, AppError>) -> String {
        match result {
            Err(AppError::InvalidInput(message)) => message,
            other => panic!("expected invalid input, got {other:?}"),
        }
    }

    #[test]
    fn title_is_trimmed_and_bounded() {
        assert_eq!(validate_title("  Groceries  ").unwrap(), "Groceries");
        assert!(invalid(validate_title("   ")).contains("must not be empty"));
        assert!(validate_title(&"x".repeat(MAX_TITLE_CHARS)).is_ok());
        assert!(invalid(validate_title(&"x".repeat(MAX_TITLE_CHARS + 1))).contains("at most 200"));
    }

    #[test]
    fn length_limits_count_characters_not_bytes() {
        // 200 multi-byte characters are allowed even though they exceed 200 bytes.
        assert!(validate_title(&"é".repeat(MAX_TITLE_CHARS)).is_ok());
        assert!(validate_content(&"é".repeat(MAX_CONTENT_CHARS)).is_ok());
        assert!(validate_content(&"é".repeat(MAX_CONTENT_CHARS + 1)).is_err());
    }

    #[test]
    fn content_may_be_empty() {
        assert!(validate_content("").is_ok());
    }

    #[test]
    fn note_ids_must_be_uuids() {
        let id = Uuid::new_v4();
        assert_eq!(parse_note_id(&format!(" {id} ")).unwrap(), id);
        assert!(invalid(parse_note_id("42")).contains("must be a UUID"));
        assert!(invalid(parse_note_id("")).contains("must be a UUID"));
    }

    #[test]
    fn search_query_is_required_and_bounded() {
        assert_eq!(validate_query(" rust ").unwrap(), "rust");
        assert!(invalid(validate_query("  ")).contains("must not be empty"));
        assert!(invalid(validate_query(&"q".repeat(MAX_QUERY_CHARS + 1))).contains("at most"));
    }

    #[test]
    fn page_size_has_a_default_and_bounds() {
        assert_eq!(
            page_size(None, DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE).unwrap(),
            20
        );
        assert_eq!(
            page_size(Some(100), DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE).unwrap(),
            100
        );
        assert!(
            invalid(page_size(Some(0), DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE))
                .contains("between 1 and 100")
        );
        assert!(
            invalid(page_size(Some(101), DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE))
                .contains("between 1 and 100")
        );
    }

    #[test]
    fn offset_has_a_default_and_bounds() {
        assert_eq!(page_offset(None).unwrap(), 0);
        assert!(invalid(page_offset(Some(-1))).contains("between 0 and"));
        assert!(invalid(page_offset(Some(MAX_OFFSET + 1))).contains("between 0 and"));
    }
}
