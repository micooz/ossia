use ossia_types::*;
use std::{collections::BTreeMap, path::PathBuf};
use ts_rs::TS;

fn main() {
    let cfg = ts_rs::Config::new().with_large_int("number");
    let mut declarations = BTreeMap::new();
    macro_rules! register {($($ty:ty),+ $(,)?)=>{$(declarations.insert(<$ty>::ident(&cfg),<$ty>::decl(&cfg));)+};}
    register!(
        DocsRequest,
        DocsHit,
        DocsOutcome,
        DocumentationStatus,
        DetailLevel,
        ListArgs,
        DocumentSummary,
        DocumentPage,
        VersionPage,
        AgentStatus,
        AgentDocument,
        AgentMusicSummary,
        AgentCheckSummary,
        AgentRender,
        AgentDocumentContext,
        AgentOpenOutcome,
        JobOutcome,
        AnalysisSection,
        AnalysisPage,
        WriteSourceArgs,
        WriteSourceOutcome,
        SourceReadStart,
        ReadSourceArgs,
        ReadSourceOutcome,
        SourceEdit,
        SourceWriteChange,
        AnalysisStatus,
        QuarterTime,
        WrittenEvent,
        SourceAnalysis,
        PerformedNote,
        PerformanceAnalysis,
        MusicReport,
        ExpectedNote,
        TrackExpectation,
        MusicExpectation,
        EvidenceSource,
        EvidenceSection,
        EvidenceSummary,
        EvidenceReport,
        EvidenceArgs,
        EvidencePage,
        EvidenceOutcome,
        ComparisonMode,
        ReferenceExpectation,
        ReferenceMusic,
        ReferenceVerification,
        VerificationStatus,
        VerificationDifferenceKind,
        VerificationDifference,
        Verification,
        CheckResult,
        DocumentId,
        VersionId,
        EditId,
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
        DocumentContext,
        EditActivity,
        EngineHealth,
        RuntimeHealth,
        DoctorReport,
        ManagementSnapshot,
        UiTheme,
        UiLanguage,
        TrayClickAction,
        UiPreferences,
        StorageCacheLimit,
        StorageStatus,
        StorageCleanupReport,
        ManagementPreferences,
        ManagementPreferencesChange,
        GeometryBounds,
        GeometryRegion,
        GeometryPage,
        RenderGeometry,
        LayoutStaff,
        LayoutSystem,
        LayoutPage,
        MeasurementCapability,
        LayoutOutcome,
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
        OutputSource,
        OutputFormat,
        OutputArgs,
        OutputOutcome,
        JobAction,
        JobArgs,
        LayoutArgs,
        LocateArgs,
        LocateOutcome,
        EmptyArgs,
        RpcRequest,
        RpcResponse,
        InstallMethod,
        AgentState,
        AgentConflict,
        AgentNotice,
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
