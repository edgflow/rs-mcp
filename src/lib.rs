//! rs-mcp: an MCP server that lets AI clients work with a notes database.
//!
//! - [`server`]: the MCP server (identity, capabilities, handlers).
//! - [`db`]: PostgreSQL access (pool, migrations, notes repository).
//! - [`config`] and [`error`]: settings and the application error type.

pub mod config;
pub mod db;
pub mod error;
pub mod server;
