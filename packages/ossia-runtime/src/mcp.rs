use crate::*;
use rmcp::{model::*, service::RequestContext, RoleServer, ServerHandler, ServiceExt};
use std::sync::Arc;

#[derive(Clone)]
pub struct Mcp {
    pub discovery: ipc::Discovery,
}
fn schema<T: schemars::JsonSchema>() -> serde_json::Map<String, serde_json::Value> {
    let schema = schemars::schema_for!(T);
    serde_json::to_value(schema)
        .expect("schema serialization")
        .as_object()
        .expect("object schema")
        .clone()
}
fn tool<T: schemars::JsonSchema>(name: &'static str, description: &'static str) -> Tool {
    Tool {
        name: name.into(),
        title: None,
        description: Some(description.into()),
        input_schema: Arc::new(schema::<T>()),
        output_schema: None,
        annotations: None,
        icons: None,
    }
}
pub fn tools() -> Vec<Tool> {
    vec![
    tool::<EmptyArgs>("ossia_guide","Read the local score editing workflow and supported capabilities."),
    tool::<EmptyArgs>("ossia_doctor","Inspect the connected runtime and verified engine build; use CLI doctor for offline resource diagnostics."),
    tool::<StatusArgs>("ossia_status","Inspect runtime, engine, library and durable jobs."),
    tool::<EmptyArgs>("ossia_list","List documents in the selected library."),
    tool::<ImportArgs>("ossia_import","Queue an absolute-path file import; poll jobId for result and conversion confirmation."),
    tool::<OpenArgs>("ossia_open","Create an isolated view and a one-use local browser link."),
    tool::<ContextArgs>("ossia_context","Read this explicit view's document, selection, source spans and event cursor."),
    tool::<EditArgs>("ossia_edit","Explicitly begin, resume, end, discard or acknowledge recovery; keep editId private."),
    tool::<WriteSourceArgs>("ossia_write_source","Atomically replace the active owned edit source and return its computed hash."),
    tool::<DocsArgs>("ossia_docs","Search or read the bundled offline LilyPond documentation matching this engine."),
    tool::<AnalysisArgs>("ossia_analysis","Read paginated notes, written pitch events or verification differences from a fixed check."),
    tool::<ArtifactArgs>("ossia_artifact","Queue a verified SVG, PNG, MIDI or PDF from a completed import/check snapshot."),
    tool::<CheckArgs>("ossia_check","Queue a check of exact expectedSourceHash; editId binds successful checks to commit."),
    tool::<CommitArgs>("ossia_commit","Commit the exact checked source and base version; no human acceptance step."),
    tool::<HistoryArgs>("ossia_history","List distinct version nodes in order."),
    tool::<RestoreArgs>("ossia_restore","Queue restoration as a new version; requires clean workspace and no active edit."),
    tool::<ExportArgs>("ossia_export","Queue immutable version export; never overwrite an existing destination."),
    tool::<JobArgs>("ossia_job","Query or cancel a durable job; terminal states are immutable."),
]
}
impl ServerHandler for Mcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some(GUIDE.into()),
            ..Default::default()
        }
    }
    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParam>,
        _: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, rmcp::ErrorData> {
        Ok(ListToolsResult {
            tools: tools(),
            next_cursor: None,
        })
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        _: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, rmcp::ErrorData> {
        if !tools().iter().any(|t| t.name == request.name) {
            return Err(rmcp::ErrorData::invalid_params("unknown tool", None));
        }
        let method = request.name.strip_prefix("ossia_").expect("tool prefix");
        let args = serde_json::Value::Object(request.arguments.unwrap_or_default());
        let result = ipc::call(&self.discovery.endpoint, method, args).await;
        Ok(match result {
            Ok(data) => CallToolResult::success(vec![Content::text(
                serde_json::to_string(&data).expect("JSON result"),
            )]),
            Err(error) => CallToolResult::error(vec![Content::text(
                serde_json::json!({"error":error}).to_string(),
            )]),
        })
    }
}
pub async fn serve(discovery: ipc::Discovery) -> Result<()> {
    let service = Mcp { discovery }
        .serve(rmcp::transport::stdio())
        .await
        .map_err(storage)?;
    service.waiting().await.map_err(storage)?;
    Ok(())
}
