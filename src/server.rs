//! The MCP server: its identity, its capabilities, and the handlers the SDK
//! routes protocol requests to.

use rmcp::{
    ServerHandler,
    model::{Implementation, ServerCapabilities, ServerConfig},
};

#[derive(Debug, Clone, Default)]
pub struct NotesServer;

impl ServerHandler for NotesServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().build())
            .with_server_info(Implementation::new(
                env!("CARGO_PKG_NAME"),
                env!("CARGO_PKG_VERSION"),
            ))
            .with_instructions("A notes server: create, read, list, search and delete notes.")
    }
}
