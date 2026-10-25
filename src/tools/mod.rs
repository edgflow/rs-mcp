//! MCP tools. Each tool validates its typed input, calls the repository and
//! returns structured JSON.

pub mod notes;

use rmcp::{
    ErrorData,
    handler::server::tool::IntoCallToolResult,
    model::{CallToolResponse, CallToolResult, ContentBlock},
};

use crate::error::AppError;

/// How an application error reaches the client, following the MCP spec's two
/// kinds of tool errors:
/// - invalid input and missing records are tool execution errors
///   (`isError: true`), which the model reads and can correct;
/// - server failures become a JSON-RPC internal error with a generic message;
///   the details are logged on stderr and never sent to the client.
impl IntoCallToolResult for AppError {
    fn into_call_tool_result(self) -> Result<CallToolResponse, ErrorData> {
        match self {
            AppError::InvalidInput(_) | AppError::NotFound(_) => {
                Ok(CallToolResult::error(vec![ContentBlock::text(self.to_string())]).into())
            }
            AppError::Database(error) => {
                tracing::error!(%error, "database error in a tool call");
                Err(internal_error())
            }
            AppError::Config(message) => {
                tracing::error!(%message, "configuration error in a tool call");
                Err(internal_error())
            }
        }
    }
}

fn internal_error() -> ErrorData {
    ErrorData::internal_error("the server could not complete the request", None)
}
