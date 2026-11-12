use crate::*;
use rmcp::{model::*, service::RequestContext, RoleServer, ServerHandler, ServiceExt};
use std::sync::Arc;

#[derive(Clone)]
pub struct Mcp {
    pub discovery: ipc::Discovery,
}

const INSTRUCTIONS: &str = "Ossia edits one local LilyPond score at a time. \
Call ossia_guide for the complete workflow, then inspect ossia_status and ossia_list. \
Use ossia_import, ossia_open and ossia_context before editing; keep IDs private. \
All score mutations are explicit. Validation and asynchronous work use the returned \
job IDs and ossia_job; use section paging instead of full job results. Use \
ossia_evidence to read immutable version or job snapshots, ossia_layout for \
page/system/staff and source-measure structure, and ossia_output for preview/export \
files. Use measureRange on source or performance evidence for pitch and timing \
questions; do not infer pitch from preview pixels. The browser score page is optional.";

fn schema<T: schemars::JsonSchema>() -> serde_json::Map<String, serde_json::Value> {
    let mut schema = serde_json::to_value(schemars::schema_for!(T))
        .expect("schema serialization")
        .as_object()
        .expect("object schema")
        .clone();
    // MCP clients require an object at the schema root; tagged enum arguments
    // otherwise serialize as a root-level oneOf.
    if !schema.contains_key("type")
        && schema
            .get("oneOf")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|variants| {
                variants.iter().all(|variant| {
                    variant.get("type").and_then(serde_json::Value::as_str) == Some("object")
                })
            })
    {
        schema.insert("type".into(), "object".into());
    }
    schema
}

fn tool<T: schemars::JsonSchema>(
    name: &'static str,
    description: &'static str,
    annotations: ToolAnnotations,
) -> Tool {
    Tool {
        name: name.into(),
        title: None,
        description: Some(description.into()),
        input_schema: Arc::new(schema::<T>()),
        output_schema: None,
        annotations: Some(annotations),
        icons: None,
    }
}

fn read_tool<T: schemars::JsonSchema>(name: &'static str, description: &'static str) -> Tool {
    tool::<T>(
        name,
        description,
        ToolAnnotations::new()
            .read_only(true)
            .destructive(false)
            .idempotent(true)
            .open_world(false),
    )
}

fn write_tool<T: schemars::JsonSchema>(
    name: &'static str,
    description: &'static str,
    destructive: bool,
) -> Tool {
    tool::<T>(
        name,
        description,
        ToolAnnotations::new()
            .read_only(false)
            .destructive(destructive)
            .idempotent(false)
            .open_world(false),
    )
}

pub fn tools() -> Vec<Tool> {
    vec![
        read_tool::<EmptyArgs>(
            "ossia_guide",
            "Read the complete local score workflow and tool contract. Returns a Markdown string.",
        ),
        read_tool::<EmptyArgs>(
            "ossia_status",
            "Inspect runtime, engine, library, documentation and active jobs. Returns AgentStatus.",
        ),
        read_tool::<ListArgs>(
            "ossia_list",
            "List compact documents with limit/offset. Returns DocumentPage; does not load render or check history.",
        ),
        write_tool::<ImportArgs>(
            "ossia_import",
            "Queue an absolute-path import. format/title/author are inferred or defaulted. Optional expect or reference (mutually exclusive) rejects the import before publishing. Returns a bounded Job summary with top-level verification; pass waitMs to wait.",
            false,
        ),
        read_tool::<OpenArgs>(
            "ossia_open",
            "Return the direct browser URL for one document and its initial compact context. Opens the score chooser when documentId is omitted. Returns AgentOpenOutcome.",
        ),
        read_tool::<ContextArgs>(
            "ossia_context",
            "Read this document's compact selection, source spans and event cursor. Returns AgentDocumentContext.",
        ),
        write_tool::<EditArgs>(
            "ossia_edit",
            "Begin, resume, end or discard an owned edit; recovery actions are advanced. Returns EditSession or null. Keep editId private.",
            true,
        ),
        write_tool::<WriteSourceArgs>(
            "ossia_write_source",
            "Atomically replace or batch-edit the active owned source and return the computed hash. The document is derived from editId. Set preview=false when an explicit check follows.",
            true,
        ),
        read_tool::<ReadSourceArgs>(
            "ossia_read_source",
            "Read a bounded UTF-8-safe source page at an exact source hash. Returns ReadSourceOutcome with the next span.",
        ),
        read_tool::<LayoutArgs>(
            "ossia_layout",
            "Inspect page, system, staff and reliable measure structure from a completed import or check job. Returns LayoutOutcome.",
        ),
        write_tool::<EvidenceArgs>(
            "ossia_evidence",
            "Read bounded source/performance music evidence from an immutable version or completed job. Optional measureRange, voice, track and channel filters. Creates a durable evidence job only on a cache miss. Returns EvidenceOutcome.",
            false,
        ),
        read_tool::<DocsRequest>(
            "ossia_docs",
            "Search or read the bundled offline LilyPond documentation. Search takes query/limit; read takes chapterId/offset/limit. Returns DocsOutcome.",
        ),
        write_tool::<JobArgs>(
            "ossia_job",
            "Get or cancel a durable job. Optionally wait with waitMs; section pages performance, source or differences evidence with the same measure filters. Returns JobOutcome.",
            true,
        ),
        write_tool::<CheckArgs>(
            "ossia_check",
            "Queue a check of exact expectedSourceHash; documentId is derived from editId when supplied. Optional expect or reference (mutually exclusive); reference compares performed notes from immutable evidence. Returns a bounded Job summary.",
            false,
        ),
        write_tool::<CommitArgs>(
            "ossia_commit",
            "Commit the exact checked source through editId. Creates a Version; base version is derived from the edit.",
            false,
        ),
        read_tool::<HistoryArgs>(
            "ossia_history",
            "Page through distinct version nodes. Returns VersionPage with limit/offset.",
        ),
        write_tool::<RestoreArgs>(
            "ossia_restore",
            "Queue restoration of a version as a new version. Requires a clean workspace and no active edit; returns a Job.",
            false,
        ),
        write_tool::<OutputArgs>(
            "ossia_output",
            "Write SVG, PNG, MIDI, PDF or LilyPond output from a completed job snapshot or immutable version. Never overwrites; returns a Job whose result is OutputOutcome.",
            false,
        ),
    ]
}

impl ServerHandler for Mcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some(INSTRUCTIONS.into()),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_tool_surface_is_stable() {
        let tools = tools();
        assert_eq!(
            tools
                .iter()
                .map(|tool| tool.name.as_ref())
                .collect::<Vec<_>>(),
            vec![
                "ossia_guide",
                "ossia_status",
                "ossia_list",
                "ossia_import",
                "ossia_open",
                "ossia_context",
                "ossia_edit",
                "ossia_write_source",
                "ossia_read_source",
                "ossia_layout",
                "ossia_evidence",
                "ossia_docs",
                "ossia_job",
                "ossia_check",
                "ossia_commit",
                "ossia_history",
                "ossia_restore",
                "ossia_output",
            ]
        );
        assert!(tools.iter().all(|tool| tool.output_schema.is_none()));
        assert!(tools.iter().all(|tool| tool.annotations.is_some()));
        assert!(tools.iter().all(|tool| {
            tool.input_schema.get("type").and_then(Value::as_str) == Some("object")
        }));
    }
}
