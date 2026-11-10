// Generated from ossia-types by pnpm generate:types. Do not edit.

export type AppError = { code: ErrorCode, message: string, detail?: JsonValue, };

export type ArtifactRef = {
/**
 * Relative to the artifact root supplied by the caller.
 */
path: string, sha256: ContentHash, bytes: number, };

export type CheckArgs = { documentId: DocumentId, viewId: ViewId, editId: EditId | null, expectedSourceHash: ContentHash, };

export type CheckOutcome = { job: Job, render: RenderManifest,
/**
 * False when the public file changed while this snapshot was compiling.
 */
current: boolean, };

export type CommitArgs = { documentId: DocumentId, viewId: ViewId, editId: EditId, baseVersionId: VersionId | null, expectedSourceHash: ContentHash, summary: string, };

export type ContentHash = string;

export type ContextArgs = { viewId: ViewId, };

export type Diagnostic = { severity: Severity, message: string, rawPosition: EnginePosition | null, position: SourcePosition | null, };

export type DocumentEditState = "idle" | "active" | "interrupted" | "recovery-conflict";

export type DocumentId = string;

export type DocumentState = { documentId: DocumentId, title: string, sourcePath: string, headVersionId: VersionId | null, sourceHash: ContentHash, dirty: boolean, editState: DocumentEditState, lastSuccessfulRender: RenderManifest | null, lastCheck: Job | null, };

export type EditAction = "begin" | "resume" | "end" | "discard" | "resolve-recovery-conflict";

export type EditArgs = { action: EditAction, documentId: DocumentId, viewId: ViewId, baseVersionId: VersionId | null, expectedSourceHash: ContentHash, editId: EditId | null, adoptWorkspace: boolean, author: string, };

export type EditId = string;

export type EditSession = { editId: EditId, documentId: DocumentId, ownerViewId: ViewId, baseVersionId: VersionId | null, status: EditStatus, startedAt: string, sourcePath: string, author: string, };

export type EditStatus = "active" | "interrupted" | "committed" | "ended";

export type EmptyArgs = Record<symbol, never>;

export type EnginePosition = {
/**
 * A normalized source name, never the machine's absolute path.
 */
file: string, line: number, column: number, charOffset: number | null, };

export type ErrorCode = "invalid-request" | "invalid-encoding" | "invalid-input" | "not-found" | "conflict" | "edit-busy" | "dirty-workspace" | "stale-selection" | "compile-failed" | "unsupported-source" | "import-confirm-required" | "import-refused" | "cancelled" | "runtime-incompatible" | "legacy-data-present" | "storage-error" | "library-busy" | "unauthorized" | "forbidden" | "runtime-unavailable";

export type ExchangeRequest = { ticket: string, };

export type ExchangeResponse = { token: string, view: ViewSnapshot, };

export type ExportArgs = { documentId: DocumentId, versionId: VersionId, format: ExportFormat, outputPath: string | null, };

export type ExportFormat = "ly" | "pdf" | "midi";

export type ExportOutcome = { documentId: DocumentId, versionId: VersionId, sourceHash: ContentHash, format: ExportFormat, path: string, bytes: number, sha256: ContentHash, };

export type FormatError = { format: RenderFormat, error: AppError, };

export type HistoryArgs = { documentId: DocumentId, };

export type ImportArgs = { title: string, format: InputFormat, path: string, confirmation: ContentHash | null, author: string, };

export type ImportOutcome = { report: ImportReport, document: DocumentState | null, render: RenderManifest | null, job: Job | null, };

export type ImportReport = { status: ImportStatus, inputHash: ContentHash, engineBuildId: string, source: string | null, sourceHash: ContentHash | null, confirmation: ContentHash | null, confirmed: boolean, measures: number, voices: number, noteEvents: number, losses: Array<string>, blocking: Array<string>, diagnostics: Array<Diagnostic>, };

export type ImportStatus = "clean" | "needs-confirmation" | "refused";

export type InputFormat = "ly" | "music-xml" | "midi";

export type Job = { jobId: JobId, documentId: DocumentId | null, kind: string, state: JobState, sourceHash: ContentHash | null, phase: string, result: JsonValue | null, error: AppError | null, };

export type JobAction = "get" | "cancel";

export type JobArgs = { jobId: JobId, action: JobAction, };

export type JobId = string;

export type JobState = "queued" | "running" | "done" | "error" | "cancelled" | "interrupted";

export type JsonValue = number | string | boolean | Array<JsonValue> | { [key in string]: JsonValue } | null;

export type LocateArgs = { sourceHash: ContentHash, page: number, x: number, y: number, };

export type LocateOutcome = { capability: LocationCapability, reason: string | null, region: LocatedRegion | null, };

export type LocatedRegion = { locator: Locator, spans: Array<SourceSpan>, capability: LocationCapability, reason: string | null, };

export type LocationCapability = "measure" | "anchor-only" | "none";

export type Locator = { staffIndex: number, measureStart: number, measureEnd: number, voice: string | null, };

export type OpenArgs = { documentId: DocumentId | null, };

export type OpenView = { viewId: ViewId, url: string, ticket: string, };

export type RenderFormat = "svg" | "pdf" | "midi";

export type RenderManifest = { sourceHash: ContentHash, engineBuildId: string, optionsHash: ContentHash, pages: Array<RenderPage>, pdf: ArtifactRef | null, midi: ArtifactRef | null, diagnostics: Array<Diagnostic>, formatErrors: Array<FormatError>, };

export type RenderPage = { page: number,
/**
 * Relative to the artifact root supplied by the caller.
 */
path: string, sha256: ContentHash, bytes: number, };

export type RenderReference = { renderKey: ContentHash, manifest: RenderManifest, };

export type RestoreArgs = { documentId: DocumentId, versionId: VersionId, baseVersionId: VersionId | null, expectedSourceHash: ContentHash, author: string, summary: string, };

export type RpcRequest = { method: string, params: JsonValue, };

export type RpcResponse = { result?: JsonValue, error?: AppError, };

export type RuntimeEvent = { instanceId: string, seq: number, type: string, viewId?: ViewId, documentId?: DocumentId, sourceHash?: ContentHash, payload: JsonValue, };

export type RuntimeInfo = { runtimeVersion: string, protocolVersion: number, schemaVersion: number, instanceId: string, home: string, webBaseUrl: string, capabilities: Array<string>, };

export type Selection = { viewId: ViewId, documentId: DocumentId, sourceHash: ContentHash, locators: Array<Locator>, };

export type Severity = "info" | "warning" | "error";

export type SourcePosition = { byte: number,
/**
 * One-based line and Unicode scalar column, including a retained BOM or CR.
 */
line: number, column: number, };

export type SourceSnapshot = { documentId: DocumentId, sourceHash: ContentHash, bytes: Array<number>, };

export type SourceSpan = {
/**
 * Half-open byte range. Consumers must validate it against the same source snapshot.
 */
byteStart: number, byteEnd: number, start: SourcePosition, end: SourcePosition, };

export type StatusSnapshot = { runtime: RuntimeInfo, engineBuildId: string, jobs: Array<Job>, instanceId: string, seq: number, };

export type Version = { versionId: VersionId, documentId: DocumentId, seq: number, parentVersionId: VersionId | null, sourceHash: ContentHash, origin: VersionOrigin, author: string, summary: string, createdAt: string, };

export type VersionId = string;

export type VersionOrigin = "import" | "edit" | "restore";

export type ViewId = string;

export type ViewSnapshot = { instanceId: string, seq: number, viewId: ViewId, document: DocumentState | null, selection: Selection | null, regions: Array<LocatedRegion>, render: RenderReference | null, };
