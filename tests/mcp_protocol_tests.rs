//! End-to-end tests through the MCP protocol: an rmcp client talks to the
//! server over an in-memory duplex pipe, exactly as it would over stdio.
//! Each test gets its own migrated database from `#[sqlx::test]`.

use rmcp::{
    RoleClient, ServiceExt,
    model::{
        CallToolRequestParams, CallToolResult, GetPromptRequestParams, ReadResourceRequestParams,
        ResourceContents,
    },
    service::{RunningService, ServiceError},
};
use rs_mcp::server::NotesServer;
use serde_json::{Value, json};
use sqlx::PgPool;

type Client = RunningService<RoleClient, ()>;

async fn connect(pool: PgPool) -> anyhow::Result<Client> {
    let (server_side, client_side) = tokio::io::duplex(64 * 1024);
    tokio::spawn(async move {
        if let Ok(service) = NotesServer::new(pool).serve(server_side).await {
            let _ = service.waiting().await;
        }
    });
    Ok(().serve(client_side).await?)
}

async fn call(
    client: &Client,
    tool: &'static str,
    args: Value,
) -> Result<CallToolResult, ServiceError> {
    let args = args.as_object().cloned().unwrap_or_default();
    client
        .call_tool(CallToolRequestParams::new(tool).with_arguments(args))
        .await
}

fn text(result: &CallToolResult) -> &str {
    result.content[0]
        .as_text()
        .map(|t| t.text.as_str())
        .unwrap_or("")
}

#[sqlx::test]
async fn lists_the_five_tools_with_schemas(pool: PgPool) -> anyhow::Result<()> {
    let client = connect(pool).await?;
    let mut tools = client.list_all_tools().await?;
    tools.sort_by(|a, b| a.name.cmp(&b.name));
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    assert_eq!(
        names,
        [
            "create_note",
            "delete_note",
            "get_note",
            "list_notes",
            "search_notes"
        ]
    );
    for tool in &tools {
        assert!(
            tool.description.is_some(),
            "{} has no description",
            tool.name
        );
        assert!(
            tool.output_schema.is_some(),
            "{} has no output schema",
            tool.name
        );
    }
    let create = &tools[0];
    assert_eq!(create.input_schema.get("required"), Some(&json!(["title"])));
    client.cancel().await?;
    Ok(())
}

#[sqlx::test]
async fn creates_gets_and_deletes_through_the_protocol(pool: PgPool) -> anyhow::Result<()> {
    let client = connect(pool).await?;

    let created = call(
        &client,
        "create_note",
        json!({"title": "Plan", "content": "Write tests"}),
    )
    .await?;
    assert_eq!(created.is_error, Some(false));
    let note = created.structured_content.expect("structured output");
    let id = note["id"].as_str().expect("id").to_string();
    assert_eq!(note["title"], "Plan");

    let fetched = call(&client, "get_note", json!({"id": id})).await?;
    assert_eq!(
        fetched.structured_content.expect("note")["content"],
        "Write tests"
    );

    let deleted = call(&client, "delete_note", json!({"id": id})).await?;
    assert_eq!(
        deleted.structured_content.expect("outcome")["deleted"],
        true
    );

    let missing = call(&client, "get_note", json!({"id": id})).await?;
    assert_eq!(missing.is_error, Some(true));
    assert!(text(&missing).contains("no note with id"));
    client.cancel().await?;
    Ok(())
}

#[sqlx::test]
async fn pages_and_searches(pool: PgPool) -> anyhow::Result<()> {
    let client = connect(pool).await?;
    for title in ["alpha", "beta", "gamma"] {
        call(
            &client,
            "create_note",
            json!({"title": title, "content": "searchable words"}),
        )
        .await?;
    }
    let page = call(&client, "list_notes", json!({"limit": 2}))
        .await?
        .structured_content
        .expect("page");
    assert_eq!(page["notes"].as_array().map(Vec::len), Some(2));
    assert_eq!(page["next_offset"], 2);
    let last = call(&client, "list_notes", json!({"limit": 2, "offset": 2}))
        .await?
        .structured_content
        .expect("page");
    assert_eq!(last["next_offset"], Value::Null);

    let found = call(
        &client,
        "search_notes",
        json!({"query": "searchable", "limit": 5}),
    )
    .await?;
    assert_eq!(
        found.structured_content.expect("results")["notes"]
            .as_array()
            .map(Vec::len),
        Some(3)
    );
    client.cancel().await?;
    Ok(())
}

#[sqlx::test]
async fn invalid_input_is_a_tool_error_the_model_can_read(pool: PgPool) -> anyhow::Result<()> {
    let client = connect(pool).await?;
    let cases = [
        (
            "create_note",
            json!({"title": " "}),
            "title must not be empty",
        ),
        ("get_note", json!({"id": "not-a-uuid"}), "id must be a UUID"),
        (
            "list_notes",
            json!({"limit": 1000}),
            "limit must be between 1 and 100",
        ),
        (
            "search_notes",
            json!({"query": ""}),
            "query must not be empty",
        ),
    ];
    for (tool, args, message) in cases {
        let result = call(&client, tool, args).await?;
        assert_eq!(result.is_error, Some(true), "{tool}");
        assert!(text(&result).contains(message), "{tool}: {}", text(&result));
    }
    client.cancel().await?;
    Ok(())
}

#[sqlx::test]
async fn database_failure_is_a_generic_internal_error(pool: PgPool) -> anyhow::Result<()> {
    let client = connect(pool.clone()).await?;
    pool.close().await;
    match call(&client, "list_notes", json!({})).await {
        Err(ServiceError::McpError(error)) => {
            assert_eq!(error.message, "the server could not complete the request");
            assert!(error.data.is_none(), "no internal details in the error");
        }
        other => panic!("expected an internal error, got {other:?}"),
    }
    client.cancel().await?;
    Ok(())
}

#[sqlx::test]
async fn unknown_tool_is_a_protocol_error(pool: PgPool) -> anyhow::Result<()> {
    let client = connect(pool).await?;
    assert!(matches!(
        call(&client, "run_sql", json!({})).await,
        Err(ServiceError::McpError(_))
    ));
    client.cancel().await?;
    Ok(())
}

#[sqlx::test]
async fn serves_the_recent_notes_resource_and_the_prompt(pool: PgPool) -> anyhow::Result<()> {
    let client = connect(pool).await?;
    call(&client, "create_note", json!({"title": "Newest"})).await?;

    let resources = client.list_all_resources().await?;
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].uri, "notes://recent");
    let read = client
        .read_resource(ReadResourceRequestParams::new("notes://recent"))
        .await?;
    let ResourceContents::TextResourceContents { text, .. } = &read.contents[0] else {
        panic!("expected text contents");
    };
    let notes: Value = serde_json::from_str(text)?;
    assert_eq!(notes[0]["title"], "Newest");

    let prompts = client.list_all_prompts().await?;
    assert_eq!(prompts[0].name, "summarize_notes");
    let args = json!({"topic": "releases"})
        .as_object()
        .cloned()
        .unwrap_or_default();
    let prompt = client
        .get_prompt(GetPromptRequestParams::new("summarize_notes").with_arguments(args))
        .await?;
    assert_eq!(prompt.messages.len(), 1);
    client.cancel().await?;
    Ok(())
}
