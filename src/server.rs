//! The MCP server: its identity, its capabilities, and the handlers the SDK
//! routes protocol requests to. The tools themselves live in `tools/`.

use rmcp::{
    ServerHandler,
    handler::server::router::tool::ToolRouter,
    model::{Implementation, ServerCapabilities, ServerConfig},
    tool_handler,
};
use sqlx::PgPool;

#[derive(Debug, Clone)]
pub struct NotesServer {
    pool: PgPool,
    tool_router: ToolRouter<Self>,
}

impl NotesServer {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            tool_router: Self::note_tools(),
        }
    }

    pub(crate) fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for NotesServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new(
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions(
                "A notes server. Use create_note, get_note, list_notes, search_notes and \
                 delete_note to work with the user's notes.",
            )
    }
}
