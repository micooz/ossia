use crate::*;
use rmcp::{model::*, service::RequestContext, RoleServer, ServerHandler, ServiceExt};

#[derive(Clone)]
pub struct Mcp {
    pub discovery: ipc::Discovery,
}

// Keep the first 250 characters self-contained: Pi's server list shows only
// that prefix, and clients that do not inject MCP instructions into model
// context see only tool descriptions and guide.md. DSH rejects instructions
// over 32 KiB.
const INSTRUCTIONS: &str = concat!(
    "Ossia edits one local LilyPond score at a time. ",
    "Start with ossia_guide; read its text and call it for relevant linked guides. Use ossia_status and ossia_list as needed. ",
    "Keep standalone IDs, hashes and editId private; score URLs are user-facing, so include them for score tasks. ",
    "Describe results in musical terms, not field names. ",
    "Mutate only on an explicit request. ",
    "Use section pages for long evidence; detail:\"full\" is diagnostics only. ",
    "Do not infer pitch or timing from preview pixels.",
);

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
    Tool::new(name, description, schema::<T>()).with_annotations(annotations)
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
        read_tool::<crate::guide::GuideArgs>(
            "ossia_guide",
            "Read the Ossia guide index, or pass document as a linked guide path to read a chapter. Returns text, path, byte size and line count.",
        ),
        read_tool::<EmptyArgs>(
            "ossia_status",
            "Inspect backend, engine, library, documentation, active jobs and the desktop updater's last check. update is null when unknown; checkedAt is Unix seconds and stale marks an outdated result. Returns AgentStatus.",
        ),
        read_tool::<ListArgs>(
            "ossia_list",
            "List compact documents with limit/offset. Returns DocumentPage; does not load render or check history.",
        ),
        tool::<UpdateMetadataArgs>(
            "ossia_update_metadata",
            "Update one active score's library metadata. Currently supports title only. Trims surrounding whitespace, rejects blank, control-character and over-200-character titles, and does not modify source, rendered output or version history. Returns DocumentSummary.",
            ToolAnnotations::new()
                .read_only(false)
                .destructive(false)
                .idempotent(true)
                .open_world(false),
        ),
        read_tool::<ConvertArgs>(
            "ossia_convert",
            "Inspect or migrate a local score file without importing it. Returns an ImportReport; set includeSource to read the candidate. See the import guide for formats and migration.",
        ),
        write_tool::<ImportArgs>(
            "ossia_import",
            "Import a local score file as a background job. Optional expect or reference verifies musical content before publication. See the import guide for formats and confirmation.",
            false,
        ),
        read_tool::<ContextArgs>(
            "ossia_context",
            "Read this document's stable browser URL, compact selection, source spans, edit state and check summary.",
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
            "Read a bounded UTF-8-safe source page at an exact source hash. Set includeLineSpans to return exact editable line spans. Returns ReadSourceOutcome with the next span.",
        ),
        read_tool::<ReadDrawingArgs>(
            "ossia_read_drawing",
            "Read a revision-guarded page of indexed drawing strokes or export the complete drawing to a new absolute JSON path. Offsets are global stroke indices; preserve expectedRevision between pages. A page filter uses one-based page numbers.",
        ),
        write_tool::<WriteDrawingArgs>(
            "ossia_write_drawing",
            "Atomically modify drawing strokes or replace the drawing from an array or absolute JSON file. Read the destination revision first. Indices refer to the original drawing at expectedRevision; additions append. Page-relative coordinates can shift when score layout changes.",
            true,
        ),
        read_tool::<LayoutArgs>(
            "ossia_layout",
            "Inspect page, system, staff and reliable measure structure from a completed import or check job. Returns LayoutOutcome.",
        ),
        write_tool::<EvidenceArgs>(
            "ossia_evidence",
            "Read bounded source/performance music evidence from an immutable version or completed job. Optional measureRange, voice, track and channel filters. The returned job contains only status metadata; music is in page. Returns EvidenceOutcome.",
            false,
        ),
        read_tool::<DocsRequest>(
            "ossia_docs",
            "Search or read the bundled offline LilyPond documentation. Search takes query, limit and matchMode=all|any; read takes chapterId/offset/limit. Zero-hit searches include querySuggestions. Returns DocsOutcome.",
        ),
        write_tool::<JobArgs>(
            "ossia_job",
            "Get or cancel a durable job. Optionally wait with waitMs; section pages performance, source or differences evidence with the same measure filters. Default results summarize render and output files; detail=full returns the full persisted job. Returns JobOutcome.",
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
        write_tool::<DeleteArgs>(
            "ossia_delete",
            "Move a score into the recoverable trash. Rejects active work unless force=true, which cancels tasks and ends edits after they stop while retaining the current source.",
            true,
        ),
        read_tool::<TrashArgs>(
            "ossia_trash",
            "Page through recoverable scores with deletedAt, retained source hash and head version.",
        ),
        write_tool::<DocumentIdArgs>(
            "ossia_untrash",
            "Restore a trashed score with its original document ID, source and version history.",
            false,
        ),
        write_tool::<DocumentIdArgs>(
            "ossia_purge",
            "Permanently delete one score from the trash, including its source, versions, jobs and derived evidence. External imported and exported files are unaffected.",
            true,
        ),
        write_tool::<RestoreArgs>(
            "ossia_restore",
            "Queue restoration of versionId as a new version. baseVersionId is the current head and expectedSourceHash is the current workspace hash. By default requires a clean workspace and no active edit. Set discardCurrentEdit to true only when explicitly authorized to discard the current draft and end its edit; returns a Job.",
            false,
        ),
        write_tool::<OutputArgs>(
            "ossia_output",
            "Write SVG, PNG, MIDI, PDF, WAV or LilyPond output from a completed job snapshot or immutable version. WAV is a 44.1 kHz stereo PCM render of the score playback mix. MIDI and WAV accept audioRange {start,end} for inclusive one-based measures. Set includeDrawing to include the document's current freehand drawing in PDF, PNG or SVG; the job captures it at submission. For PDF, pages selects pages in the single PDF (all by default). For SVG/PNG, pages chooses which pages to write; one page may use outputPath, while multiple pages return OutputBatchOutcome. Crop writes one SVG/PNG per affected page; a one-item pages list filters it. Omitted outputPath or outputDirectory writes to the system temporary directory; set them for files to keep. Never overwrites.",
            false,
        ),
    ]
}

impl ServerHandler for Mcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(INSTRUCTIONS)
    }
    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, rmcp::ErrorData> {
        Ok(ListToolsResult::with_all_items(tools()))
    }
    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResponse, rmcp::ErrorData> {
        if !tools().iter().any(|t| t.name == request.name) {
            return Err(rmcp::ErrorData::invalid_params("unknown tool", None));
        }
        let method = request.name.strip_prefix("ossia_").expect("tool prefix");
        let args = serde_json::Value::Object(request.arguments.unwrap_or_default());
        let result = match ipc::call(&self.discovery.endpoint, method, args.clone()).await {
            Ok(data) => crate::agent_response::compact(method, &args, data),
            Err(error) => Err(error),
        };
        Ok(match result {
            Ok(data) => CallToolResult::success(vec![ContentBlock::text(
                serde_json::to_string(&data).expect("JSON result"),
            )]),
            Err(error) => CallToolResult::error(vec![ContentBlock::text(
                serde_json::json!({"error":error}).to_string(),
            )]),
        }
        .into())
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
    fn instructions_fit_the_pi_summary_budget() {
        assert!(!INSTRUCTIONS.contains('\n'));
        assert!(INSTRUCTIONS.len() <= 500);
        let head = INSTRUCTIONS.chars().take(250).collect::<String>();
        for required in [
            "ossia_guide",
            "ossia_status",
            "ossia_list",
            "editId",
            "score URL",
        ] {
            assert!(head.contains(required), "{required}");
        }
    }

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
                "ossia_update_metadata",
                "ossia_convert",
                "ossia_import",
                "ossia_context",
                "ossia_edit",
                "ossia_write_source",
                "ossia_read_source",
                "ossia_read_drawing",
                "ossia_write_drawing",
                "ossia_layout",
                "ossia_evidence",
                "ossia_docs",
                "ossia_job",
                "ossia_check",
                "ossia_commit",
                "ossia_history",
                "ossia_delete",
                "ossia_trash",
                "ossia_untrash",
                "ossia_purge",
                "ossia_restore",
                "ossia_output",
            ]
        );
        assert!(tools.iter().all(|tool| tool.output_schema.is_none()));
        assert!(tools.iter().all(|tool| tool.annotations.is_some()));
        assert!(tools.iter().all(|tool| {
            tool.input_schema.get("type").and_then(Value::as_str) == Some("object")
        }));
        for name in ["ossia_delete", "ossia_purge"] {
            let tool = tools.iter().find(|tool| tool.name == name).unwrap();
            assert_eq!(
                tool.annotations
                    .as_ref()
                    .and_then(|value| value.destructive_hint),
                Some(true)
            );
        }
    }

    #[test]
    fn import_tools_remain_concise_and_point_to_the_guide() {
        let tools = tools();
        for name in ["ossia_convert", "ossia_import"] {
            let description = tools
                .iter()
                .find(|tool| tool.name == name)
                .and_then(|tool| tool.description.as_deref())
                .unwrap();
            assert!(description.len() <= 240, "{name} description is too long");
            assert!(
                description.contains("guide"),
                "{name} must point to the guide"
            );
        }
    }
}
