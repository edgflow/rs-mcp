# rs-mcp

A small, clean [Model Context Protocol](https://modelcontextprotocol.io) (MCP)
server in Rust. It stores notes in PostgreSQL and exposes them to AI clients
as MCP tools, plus one resource and one prompt.

## What MCP is, and what this server does

MCP is an open protocol that lets AI applications (MCP *clients*) discover and
use capabilities provided by MCP *servers*. A server offers:

- **tools**: functions the model can call, each with a typed JSON input schema;
- **resources**: data the client can read by URI;
- **prompts**: reusable prompt templates.

This server is the notes application's side of that contract. It does not
contain an AI model: the connected client decides when to call its tools.

| Kind | Name | What it does |
|---|---|---|
| Tool | `create_note` | Create a note with a title and content |
| Tool | `get_note` | Get a note by id |
| Tool | `list_notes` | List notes, newest first, with pagination |
| Tool | `search_notes` | Full-text search over titles and content |
| Tool | `delete_note` | Delete a note by id |
| Resource | `notes://recent` | The 10 most recent notes as JSON |
| Prompt | `summarize_notes` | Instructions for summarizing notes, optionally by topic |

## Architecture

```text
MCP client ──launches──► rs-mcp process
     ▲  stdin: JSON-RPC requests          │
     └─ stdout: JSON-RPC responses ◄──────┤   stderr: logs
                                          │
                 rmcp (protocol, lifecycle, schemas)
                                          │
        server.rs  ── identity, capabilities, resource, prompt
        tools/     ── typed inputs, validation, structured output
        db/        ── SQL (notes_repository.rs), pool, migrations
                                          │
                                     PostgreSQL
```

- **Transport: stdio.** The client starts `rs-mcp` as a subprocess and talks
  JSON-RPC over its stdin/stdout. Stdout carries only protocol messages; every
  log line goes to stderr.
- **Protocol: the [rmcp](https://crates.io/crates/rmcp) SDK.** It handles the
  lifecycle (the `initialize` handshake of protocol versions up to 2025-11-25,
  and `server/discover` of 2026-07-28), version negotiation, tool and prompt
  routing, and JSON Schemas generated from the Rust input types.
- **Layers.** MCP-specific code (`server.rs`, `tools/`) is separate from
  database code (`db/`), so another transport could be added without touching
  the business logic.
- **Redis is not used.** Every request is a single indexed query on a small
  table, so a cache would add moving parts without a measurable benefit. If it
  were ever needed (for example for hot `list_notes` pages), it would wrap
  `db::notes_repository` with keys built from the query parameters, a TTL, and
  invalidation on create and delete; PostgreSQL would stay the source of truth.

## Prerequisites

- Rust stable, 1.85 or newer (edition 2024): <https://rustup.rs>
- PostgreSQL 13 or newer, or Docker to run the one in `compose.yaml`
- Optional: Node.js 22.19 or newer, for the MCP Inspector

## Setup

```sh
# 1. Start PostgreSQL (data is kept in the notes-data volume)
docker compose up -d

# 2. Configure the environment
cp .env.example .env        # DATABASE_URL, RUST_LOG

# 3. Build
cargo build --release
```

`.env` is optional and git-ignored; variables already set in the environment
take precedence. `DATABASE_URL` is required and validated at startup, and it
is never logged.

### Database migrations

The schema lives in `migrations/001_create_notes.sql`. The migrations are
embedded in the binary and **run automatically at startup**, so a fresh
database is ready on the first launch. To apply them yourself instead (for
example as the table owner, before running the server as a restricted user
with `MIGRATE_ON_STARTUP=false`):

```sh
cargo install sqlx-cli --no-default-features --features rustls,postgres
sqlx migrate run            # uses DATABASE_URL
```

| Variable | Required | Meaning |
|---|---|---|
| `DATABASE_URL` | yes | `postgres://user:password@host:port/database` |
| `RUST_LOG` | no | Log filter, default `info` (logs go to stderr) |
| `MIGRATE_ON_STARTUP` | no | `true` (default) or `false` to skip migrations at startup |

## Running the server

An MCP client normally starts the server for you (see below). To check that it
starts, run it directly; it waits for JSON-RPC on stdin and stops at end of
input or on Ctrl-C:

```sh
./target/release/rs-mcp
```

## Connecting an MCP client

A stdio server is not a network service: the client **launches the binary as
a subprocess**, so the client configuration names the command to run and the
environment variables to give it. Use the absolute path of the built binary.

### MCP Inspector

The [MCP Inspector](https://modelcontextprotocol.io/docs/tools/inspector) is
the protocol's official testing tool. `-e` passes environment variables to the
server process:

```sh
# Web UI
npx @modelcontextprotocol/inspector /abs/path/to/rs-mcp/target/release/rs-mcp \
  -e DATABASE_URL=postgres://notes:notes@localhost:5432/notes

# CLI: list the tools, then call one
npx @modelcontextprotocol/inspector --cli /abs/path/to/rs-mcp/target/release/rs-mcp \
  -e DATABASE_URL=postgres://notes:notes@localhost:5432/notes --method tools/list
npx @modelcontextprotocol/inspector --cli /abs/path/to/rs-mcp/target/release/rs-mcp \
  -e DATABASE_URL=postgres://notes:notes@localhost:5432/notes \
  --method tools/call --tool-name create_note --tool-arg title="Shopping list"
```

### VS Code

VS Code reads a portable `.mcp.json` at the workspace root with a top-level
`mcpServers` object ([configuration reference](https://code.visualstudio.com/docs/agents/reference/mcp-configuration)).
`${DATABASE_URL}` is taken from the environment VS Code runs in, which keeps
credentials out of the file:

```json
{
  "mcpServers": {
    "notes": {
      "command": "/abs/path/to/rs-mcp/target/release/rs-mcp",
      "args": [],
      "env": {
        "DATABASE_URL": "${DATABASE_URL}",
        "RUST_LOG": "info"
      }
    }
  }
}
```

Other clients that support stdio servers take the same three things: the
command, its arguments, and its environment variables.

## Tools

All tools return structured JSON (`structuredContent`, with the same JSON as
text for older clients) and declare an output schema.

| Tool | Input | Output | Annotations |
|---|---|---|---|
| `create_note` | `title` (string, required, 1–200 chars), `content` (string, optional, up to 20,000 chars) | the note | writes, not destructive |
| `get_note` | `id` (UUID string, required) | the note | read-only |
| `list_notes` | `limit` (1–100, default 20), `offset` (0–10,000, default 0) | `notes`, `limit`, `offset`, `next_offset` | read-only |
| `search_notes` | `query` (1–200 chars, required), `limit` (1–50, default 10) | `query`, `notes` | read-only |
| `delete_note` | `id` (UUID string, required) | `id`, `deleted` | destructive, idempotent |

A note is `{ id, title, content, created_at, updated_at }` with UTC
timestamps. `list_notes` orders by `created_at` descending with the id as a
tiebreak, so pages are stable; `next_offset` is `null` on the last page.
`search_notes` uses PostgreSQL full-text search (best matches first) and
accepts web-search syntax: words, `"quoted phrases"`, `OR`, and `-word`.

### Examples

`create_note` with `{"title": "Shopping list", "content": "eggs, milk, coffee"}`:

```json
{
  "id": "5f8fbe5c-968b-480f-ad42-0f54c04fe3cf",
  "title": "Shopping list",
  "content": "eggs, milk, coffee",
  "created_at": "2026-10-25T12:26:01.525188Z",
  "updated_at": "2026-10-25T12:26:01.525188Z"
}
```

`list_notes` with `{"limit": 1}`:

```json
{
  "notes": [
    {
      "id": "111f25de-7741-4193-9b93-d0d053b0370c",
      "title": "Release v0.1",
      "content": "Tag the release and write the changelog.",
      "created_at": "2026-10-25T12:26:01.528817Z",
      "updated_at": "2026-10-25T12:26:01.528817Z"
    }
  ],
  "limit": 1,
  "offset": 0,
  "next_offset": 1
}
```

### Errors

The server follows the MCP specification's two kinds of tool errors:

- **Tool execution errors** (`isError: true`, with a readable message) for
  problems the model can fix: invalid input (`invalid input: limit must be
  between 1 and 100`, `invalid input: id must be a UUID ...`) and missing
  records (`no note with id ...`). `get_note` on a missing id is such an error;
  `delete_note` on a missing id is not an error, it returns `"deleted": false`.
- **Protocol errors** (JSON-RPC errors) for unknown tools, malformed requests
  and server failures. A database failure returns `internal_error` with the
  message `the server could not complete the request`; the cause is logged on
  stderr and never sent to the client (no SQL, connection strings or traces).

## Resource and prompt

- `notes://recent` (`application/json`): the 10 newest notes. Other URIs return
  a resource-not-found error.
- `summarize_notes` (optional argument `topic`, up to 200 characters): a user
  message telling the model to gather the notes with `search_notes` (or the
  `notes://recent` resource) and summarize them without inventing anything.

## Testing

The repository tests and the protocol tests need PostgreSQL. `#[sqlx::test]`
creates a **separate, freshly migrated database for every test** on the server
in `DATABASE_URL` and drops it afterwards, so tests never touch your data. The
user in `DATABASE_URL` must be allowed to create databases (the `notes` user
of `compose.yaml` is).

```sh
cargo test                  # unit, repository and MCP protocol tests
```

- `src/**` unit tests: input validation and configuration (no database).
- `tests/notes_repository_tests.rs`: the SQL, against PostgreSQL.
- `tests/mcp_protocol_tests.rs`: an rmcp client talks to the server through
  the protocol over an in-memory pipe: discovery, every tool, error mapping, a
  database failure, the resource and the prompt.

Formatting and lints:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

## Common errors

| Message | Cause |
|---|---|
| `configuration error: DATABASE_URL is not set` | No `DATABASE_URL` in the environment or `.env`; for a client, set it in the server's `env` |
| `cannot connect to PostgreSQL (check DATABASE_URL)` | PostgreSQL is not running or the URL is wrong; try `docker compose ps` |
| `database migrations failed` | The user cannot create tables (apply migrations as the owner and set `MIGRATE_ON_STARTUP=false`), or the database has a conflicting `notes` table |
| Tests panic with `DATABASE_URL must be set` | `#[sqlx::test]` needs `DATABASE_URL`; put it in `.env` or the environment |
| The client shows no tools | Check the absolute command path and the server's stderr output in the client's logs |

## Security

- **Trust model.** Over stdio the server runs as a local process with the
  permissions of the user who launched it, and it trusts that process's
  client. There is no authentication: anyone who can launch the binary with a
  valid `DATABASE_URL` can use every tool. Invoking a tool is not proof that
  the end user approved it; clients should ask before destructive calls
  (`delete_note` is annotated as destructive). An HTTP transport would need
  authentication and TLS before being exposed beyond localhost.
- **What the tools can do.** Read and write rows of the `notes` table, and
  nothing else: no arbitrary SQL, no shell commands, no file-system access.
- **Input.** Every input is validated (lengths, ranges, UUIDs) and every query
  is parameterized; the table's constraints back up the validation.
- **Bounded output.** Pages, search results and the resource have hard limits.
- **Credentials.** `DATABASE_URL` is never logged, and errors returned to
  clients contain no internal details. Never commit `.env`.
- **Least privilege.** In production, apply migrations as the table owner
  (`sqlx migrate run`) and run the server as a user that can only use the
  `notes` table, with `MIGRATE_ON_STARTUP=false`:

  ```sql
  CREATE ROLE notes_app LOGIN PASSWORD '...';
  GRANT USAGE ON SCHEMA public TO notes_app;
  GRANT SELECT, INSERT, DELETE ON notes TO notes_app;
  ```

## Project layout

```text
src/main.rs                     startup: config, database, stdio service, shutdown
src/lib.rs                      library root (lets tests/ use the modules)
src/config.rs                   environment configuration
src/error.rs                    application error type
src/server.rs                   MCP server: info, capabilities, resource, prompt
src/tools/mod.rs                error → MCP result mapping
src/tools/notes.rs              the five tools, input types, validation
src/db/mod.rs                   pool and migrations
src/db/notes_repository.rs      SQL for notes
migrations/001_create_notes.sql schema
tests/                          repository and protocol tests
compose.yaml                    local PostgreSQL
```

## License

MIT
