//! Shared values for the local core. Source offsets always address the original UTF-8 bytes.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fmt, str::FromStr};
use uuid::Uuid;

pub const PROTOCOL_VERSION: u32 = 1;
pub const SCHEMA_VERSION: u32 = 1;

macro_rules! identifier {
    ($($name:ident),+ $(,)?) => {$ (
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
        #[serde(transparent)]
        #[ts(type = "string")]
        pub struct $name(Uuid);
        impl FromStr for $name {
            type Err = uuid::Error;
            fn from_str(value: &str) -> Result<Self, Self::Err> { Uuid::parse_str(value).map(Self) }
        }
        impl $name {
            pub fn new() -> Self { Self(Uuid::new_v4()) }
        }
        impl Default for $name {
            fn default() -> Self { Self::new() }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { self.0.fmt(f) }
        }
    )+};
}
identifier!(DocumentId, VersionId, EditId, ViewId, JobId);

#[derive(
    Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(try_from = "String", into = "String")]
#[ts(type = "string")]
pub struct ContentHash(String);

impl ContentHash {
    pub fn of(bytes: &[u8]) -> Self {
        Self(format!("{:x}", Sha256::digest(bytes)))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl TryFrom<String> for ContentHash {
    type Error = AppError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            Ok(Self(value))
        } else {
            Err(AppError::invalid("expected a lowercase SHA-256 hash"))
        }
    }
}
impl From<ContentHash> for String {
    fn from(value: ContentHash) -> Self {
        value.0
    }
}
impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "camelCase")]
pub struct SourcePosition {
    pub byte: usize,
    /// One-based line and Unicode scalar column, including a retained BOM or CR.
    pub line: usize,
    pub column: usize,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "camelCase")]
pub struct SourceSpan {
    /// Half-open byte range. Consumers must validate it against the same source snapshot.
    pub byte_start: usize,
    pub byte_end: usize,
    pub start: SourcePosition,
    pub end: SourcePosition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(
    rename_all = "camelCase",
    try_from = "SnapshotWire",
    into = "SnapshotWire"
)]
pub struct SourceSnapshot {
    document_id: DocumentId,
    source_hash: ContentHash,
    bytes: Vec<u8>,
}

#[derive(Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
struct SnapshotWire {
    document_id: DocumentId,
    source_hash: ContentHash,
    bytes: Vec<u8>,
}

impl SourceSnapshot {
    pub fn new(document_id: DocumentId, bytes: Vec<u8>) -> Result<Self, AppError> {
        std::str::from_utf8(&bytes)
            .map_err(|_| AppError::new(ErrorCode::InvalidEncoding, "source must be UTF-8"))?;
        Ok(Self {
            document_id,
            source_hash: ContentHash::of(&bytes),
            bytes,
        })
    }
    pub fn document_id(&self) -> DocumentId {
        self.document_id
    }
    pub fn source_hash(&self) -> &ContentHash {
        &self.source_hash
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn source(&self) -> &str {
        std::str::from_utf8(&self.bytes).expect("validated snapshot")
    }
}
impl TryFrom<SnapshotWire> for SourceSnapshot {
    type Error = AppError;
    fn try_from(value: SnapshotWire) -> Result<Self, Self::Error> {
        let snapshot = Self::new(value.document_id, value.bytes)?;
        if snapshot.source_hash != value.source_hash {
            return Err(AppError::invalid("snapshot hash does not match bytes"));
        }
        Ok(snapshot)
    }
}
impl From<SourceSnapshot> for SnapshotWire {
    fn from(value: SourceSnapshot) -> Self {
        Self {
            document_id: value.document_id,
            source_hash: value.source_hash,
            bytes: value.bytes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct Locator {
    pub staff_index: usize,
    pub measure_start: u32,
    pub measure_end: u32,
    pub voice: Option<String>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "kebab-case")]
pub enum LocationCapability {
    Measure,
    AnchorOnly,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct LocatedRegion {
    pub locator: Locator,
    pub spans: Vec<SourceSpan>,
    pub capability: LocationCapability,
    pub reason: Option<String>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    InvalidRequest,
    InvalidEncoding,
    InvalidInput,
    NotFound,
    Conflict,
    EditBusy,
    DirtyWorkspace,
    StaleSelection,
    CompileFailed,
    UnsupportedSource,
    ImportConfirmRequired,
    ImportRefused,
    Cancelled,
    RuntimeIncompatible,
    LegacyDataPresent,
    StorageError,
    LibraryBusy,
    Unauthorized,
    Forbidden,
    RuntimeUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detail: Option<serde_json::Value>,
}
impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            detail: None,
        }
    }
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidInput, message)
    }
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::UnsupportedSource, message)
    }
}
impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(f)
    }
}
impl std::error::Error for AppError {}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "kebab-case")]
pub enum InputFormat {
    Ly,
    MusicXml,
    Midi,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "kebab-case")]
pub enum ImportStatus {
    Clean,
    NeedsConfirmation,
    Refused,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub status: ImportStatus,
    pub input_hash: ContentHash,
    pub engine_build_id: String,
    pub source: Option<String>,
    pub source_hash: Option<ContentHash>,
    pub confirmation: Option<ContentHash>,
    pub confirmed: bool,
    pub measures: u32,
    pub voices: usize,
    pub note_events: usize,
    pub losses: Vec<String>,
    pub blocking: Vec<String>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
pub enum RenderFormat {
    Svg,
    Pdf,
    Midi,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct EnginePosition {
    /// A normalized source name, never the machine's absolute path.
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub char_offset: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub raw_position: Option<EnginePosition>,
    pub position: Option<SourcePosition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactRef {
    /// Relative to the artifact root supplied by the caller.
    pub path: String,
    pub sha256: ContentHash,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct RenderPage {
    pub page: u32,
    #[serde(flatten)]
    pub artifact: ArtifactRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct FormatError {
    pub format: RenderFormat,
    pub error: AppError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct RenderManifest {
    pub source_hash: ContentHash,
    pub engine_build_id: String,
    pub options_hash: ContentHash,
    pub pages: Vec<RenderPage>,
    pub pdf: Option<ArtifactRef>,
    pub midi: Option<ArtifactRef>,
    pub diagnostics: Vec<Diagnostic>,
    pub format_errors: Vec<FormatError>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "kebab-case")]
pub enum EditStatus {
    Active,
    Interrupted,
    Committed,
    Ended,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct EditSession {
    pub edit_id: EditId,
    pub document_id: DocumentId,
    pub owner_view_id: ViewId,
    pub base_version_id: Option<VersionId>,
    pub status: EditStatus,
    pub started_at: String,
    pub source_path: String,
    pub author: String,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "kebab-case")]
pub enum DocumentEditState {
    Idle,
    Active,
    Interrupted,
    RecoveryConflict,
}

/// Public document state deliberately omits the edit capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct DocumentState {
    pub document_id: DocumentId,
    pub title: String,
    pub source_path: String,
    pub head_version_id: Option<VersionId>,
    pub source_hash: ContentHash,
    pub dirty: bool,
    pub edit_state: DocumentEditState,
    pub last_successful_render: Option<RenderManifest>,
    pub last_check: Option<Job>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "kebab-case")]
pub enum VersionOrigin {
    Import,
    Edit,
    Restore,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub version_id: VersionId,
    pub document_id: DocumentId,
    pub seq: u64,
    pub parent_version_id: Option<VersionId>,
    pub source_hash: ContentHash,
    pub origin: VersionOrigin,
    pub author: String,
    pub summary: String,
    pub created_at: String,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "kebab-case")]
pub enum JobState {
    Queued,
    Running,
    Done,
    Error,
    Cancelled,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub job_id: JobId,
    pub document_id: Option<DocumentId>,
    pub kind: String,
    pub state: JobState,
    pub source_hash: Option<ContentHash>,
    pub phase: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<AppError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct ImportOutcome {
    pub report: ImportReport,
    pub document: Option<DocumentState>,
    pub render: Option<RenderManifest>,
    pub job: Option<Job>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct CheckOutcome {
    pub job: Job,
    pub render: RenderManifest,
    /// False when the public file changed while this snapshot was compiling.
    pub current: bool,
}

mod runtime;
pub use runtime::*;
