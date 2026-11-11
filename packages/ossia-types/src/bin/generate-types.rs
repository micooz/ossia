use ossia_types::*;
use std::{collections::BTreeMap, path::PathBuf};
use ts_rs::TS;

fn main() {
    let cfg = ts_rs::Config::new().with_large_int("number");
    let mut declarations = BTreeMap::new();
    macro_rules! register {($($ty:ty),+ $(,)?)=>{$(declarations.insert(<$ty>::ident(&cfg),<$ty>::decl(&cfg));)+};}
    register!(
        DocumentId,
        VersionId,
        EditId,
        ViewId,
        JobId,
        ContentHash,
        SourcePosition,
        SourceSpan,
        SourceSnapshot,
        Locator,
        LocationCapability,
        LocatedRegion,
        ErrorCode,
        AppError,
        InputFormat,
        ImportStatus,
        ImportReport,
        RenderFormat,
        Severity,
        EnginePosition,
        Diagnostic,
        ArtifactRef,
        RenderPage,
        FormatError,
        RenderManifest,
        EditStatus,
        EditSession,
        DocumentEditState,
        DocumentState,
        VersionOrigin,
        Version,
        JobState,
        Job,
        ImportOutcome,
        CheckOutcome,
        RuntimeInfo,
        Selection,
        RenderReference,
        ViewSnapshot,
        EditActivity,
        EngineHealth,
        RuntimeHealth,
        DoctorReport,
        ManagementSnapshot,
        UiTheme,
        UiLanguage,
        TrayClickAction,
        UiPreferences,
        ManagementPreferences,
        ManagementPreferencesChange,
        GeometryBounds,
        GeometryRegion,
        GeometryPage,
        RenderGeometry,
        OpenView,
        ExchangeRequest,
        ExchangeResponse,
        RuntimeEvent,
        ImportArgs,
        OpenArgs,
        ContextArgs,
        EditAction,
        EditArgs,
        CheckArgs,
        CommitArgs,
        HistoryArgs,
        RestoreArgs,
        ExportFormat,
        ExportArgs,
        ExportOutcome,
        JobAction,
        JobArgs,
        LocateArgs,
        LocateOutcome,
        StatusSnapshot,
        EmptyArgs,
        RpcRequest,
        RpcResponse,
        InstallMethod,
        AgentState,
        AgentConflict,
        AgentIntegration,
        AgentsSnapshot,
        AgentsChange,
        AgentProblem,
        AgentsMutation,
        serde_json::Value
    );
    let mut text =
        String::from("// Generated from ossia-types by pnpm generate:types. Do not edit.\n\n");
    for declaration in declarations.values() {
        text.push_str("export ");
        text.push_str(declaration);
        text.push_str("\n\n");
    }
    let text = format!(
        "{}\n",
        text.lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n")
            .trim_end()
    );
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/ui/src/generated/protocol.ts");
    if std::env::args().any(|arg| arg == "--check") {
        if std::fs::read_to_string(&path).ok().as_deref() != Some(&text) {
            eprintln!("protocol types differ; run pnpm generate:types");
            std::process::exit(1);
        }
    } else {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}
