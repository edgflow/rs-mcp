//! The MCP server: its identity, its capabilities, and the handlers the SDK
//! routes protocol requests to. The tools live in `tools/`; the one resource
//! and the one prompt are small enough to live here.

use rmcp::{
    ErrorData, RoleServer, ServerHandler,
    handler::server::{
        router::{prompt::PromptRouter, tool::ToolRouter},
        wrapper::Parameters,
    },
    model::{
        Implementation, ListResourcesResult, PaginatedRequestParams, PromptMessage,
        ReadResourceRequestParams, ReadResourceResponse, ReadResourceResult, Resource,
        ResourceContents, Role, ServerCapabilities, ServerConfig,
    },
    prompt, prompt_handler, prompt_router,
    service::RequestContext,
    tool_handler,
};
use schemars::JsonSchema;
use serde::Deserialize;
use sqlx::PgPool;

use crate::db::notes_repository;

/// The resource with the newest notes.
pub const RECENT_NOTES_URI: &str = "notes://recent";
/// How many notes the recent-notes resource returns.
pub const RECENT_NOTES_LIMIT: i64 = 10;
const MAX_TOPIC_CHARS: usize = 200;

#[derive(Debug, Clone)]
pub struct NotesServer {
    pool: PgPool,
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
}

impl NotesServer {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            tool_router: Self::note_tools(),
            prompt_router: Self::note_prompts(),
        }
    }

    pub(crate) fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SummarizeNotesArgs {
    /// Optional topic, for example "project ideas"; without it, the most recent notes.
    pub topic: Option<String>,
}

#[prompt_router(router = "note_prompts")]
impl NotesServer {
    /// Instructions for summarizing notes with this server's tools.
    #[prompt(description = "Summarize notes, optionally only those about a topic.")]
    async fn summarize_notes(
        &self,
        Parameters(args): Parameters<SummarizeNotesArgs>,
    ) -> Result<Vec<PromptMessage>, ErrorData> {
        let topic = args
            .topic
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty());
        if topic.is_some_and(|t| t.chars().count() > MAX_TOPIC_CHARS) {
            return Err(ErrorData::invalid_params(
                format!("topic must be at most {MAX_TOPIC_CHARS} characters"),
                None,
            ));
        }
        let find_notes = match topic {
            Some(topic) => format!(
                "Summarize my notes about \"{topic}\". First call search_notes with a query \
                 for that topic. If nothing matches, say so and stop."
            ),
            None => format!(
                "Summarize my recent notes. First read the {RECENT_NOTES_URI} resource \
                 (or call list_notes)."
            ),
        };
        Ok(vec![PromptMessage::new_text(
            Role::User,
            format!(
                "{find_notes} Then write a short summary: the main points as bullet points, \
                 any open questions or to-dos, and the titles of the notes you used. \
                 Do not invent anything that is not in the notes."
            ),
        )])
    }
}

#[tool_handler(router = self.tool_router)]
#[prompt_handler(router = self.prompt_router)]
impl ServerHandler for NotesServer {
    fn get_info(&self) -> ServerConfig {
        let capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_prompts()
            .build();
        ServerConfig::new(capabilities)
            .with_server_info(Implementation::new(
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "A notes server. Use create_note, get_note, list_notes, search_notes and \
                 delete_note to work with the user's notes; notes://recent has the newest ones.",
            )
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, ErrorData> {
        let recent = Resource::new(RECENT_NOTES_URI, "recent-notes")
            .with_title("Recent notes")
            .with_description(format!(
                "The {RECENT_NOTES_LIMIT} most recent notes, newest first, as JSON."
            ))
            .with_mime_type("application/json");
        Ok(ListResourcesResult::with_all_items(vec![recent]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        if request.uri != RECENT_NOTES_URI {
            return Err(ErrorData::resource_not_found(
                format!("unknown resource; this server has {RECENT_NOTES_URI}"),
                None,
            ));
        }
        let notes = notes_repository::list_notes(&self.pool, RECENT_NOTES_LIMIT, 0)
            .await
            .map_err(|error| {
                tracing::error!(%error, "database error reading {RECENT_NOTES_URI}");
                ErrorData::internal_error("the server could not complete the request", None)
            })?;
        let json = serde_json::to_string_pretty(&notes)
            .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
        let contents =
            ResourceContents::text(json, RECENT_NOTES_URI).with_mime_type("application/json");
        Ok(ReadResourceResult::new(vec![contents]).into())
    }
}
