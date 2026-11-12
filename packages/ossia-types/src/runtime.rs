use crate::*;
use schemars::JsonSchema;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInfo {
    pub runtime_version: String,
    pub protocol_version: u32,
    pub schema_version: u32,
    pub instance_id: String,
    pub home: String,
    pub web_base_url: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub document_id: DocumentId,
    pub source_hash: ContentHash,
    pub locators: Vec<Locator>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct RenderReference {
    pub render_key: ContentHash,
    pub manifest: RenderManifest,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct DocumentContext {
    pub instance_id: String,
    pub seq: u64,
    pub document: DocumentState,
    pub selection: Option<Selection>,
    pub regions: Vec<LocatedRegion>,
    pub render: Option<RenderReference>,
    pub edit_activity: Option<EditActivity>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEvent {
    pub instance_id: String,
    pub seq: u64,
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub document_id: Option<DocumentId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub source_hash: Option<ContentHash>,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportArgs {
    pub title: Option<String>,
    pub format: Option<InputFormat>,
    pub path: String,
    pub confirmation: Option<ContentHash>,
    pub author: Option<String>,
    pub expect: Option<MusicExpectation>,
    pub reference: Option<ReferenceExpectation>,
    pub wait_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenArgs {
    pub document_id: Option<DocumentId>,
}

/// Identify one document for a context read.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextArgs {
    pub document_id: DocumentId,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum EditAction {
    Begin,
    Resume,
    End,
    Discard,
    ResolveRecoveryConflict,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditArgs {
    pub action: EditAction,
    pub document_id: DocumentId,
    pub base_version_id: Option<VersionId>,
    pub expected_source_hash: ContentHash,
    pub edit_id: Option<EditId>,
    #[serde(default)]
    pub adopt_workspace: bool,
    #[serde(default = "default_author")]
    pub author: String,
}
/// Attribution used when an operation omits its author.
pub const DEFAULT_AUTHOR: &str = "Ossia";
fn default_author() -> String {
    DEFAULT_AUTHOR.into()
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckArgs {
    #[serde(default)]
    pub expect: Option<MusicExpectation>,
    #[serde(default)]
    pub reference: Option<ReferenceExpectation>,
    pub document_id: Option<DocumentId>,
    pub edit_id: Option<EditId>,
    pub expected_source_hash: ContentHash,
    pub wait_ms: Option<u64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommitArgs {
    pub edit_id: EditId,
    pub expected_source_hash: ContentHash,
    pub summary: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoryArgs {
    pub document_id: DocumentId,
    pub limit: Option<usize>,
    #[serde(default)]
    pub offset: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestoreArgs {
    pub version_id: VersionId,
    pub base_version_id: Option<VersionId>,
    pub expected_source_hash: ContentHash,
    pub author: Option<String>,
    pub summary: String,
    pub wait_ms: Option<u64>,
}

/// Immutable snapshot source for a generated output file.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum OutputSource {
    Job { job_id: JobId },
    Version { version_id: VersionId },
}

/// Output file format for a job or version snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Ly,
    Svg,
    Png,
    Pdf,
    Midi,
}

/// Write one output file from a completed job snapshot or an immutable version.
/// PDF may use an open password; PNG may use lossy quality and transparency options.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutputArgs {
    pub source: OutputSource,
    pub format: OutputFormat,
    pub page: Option<u32>,
    pub width: Option<u32>,
    pub pdf_password: Option<String>,
    pub png_quality: Option<u8>,
    pub png_transparent: Option<bool>,
    pub output_path: Option<String>,
    pub wait_ms: Option<u64>,
}

/// Verified local output file and immutable provenance.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct OutputOutcome {
    pub source: OutputSource,
    pub document_id: DocumentId,
    pub source_hash: ContentHash,
    pub engine_build_id: String,
    pub format: OutputFormat,
    pub page: Option<u32>,
    pub path: String,
    pub bytes: u64,
    pub sha256: ContentHash,
    pub media_type: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum JobAction {
    Get,
    Cancel,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JobArgs {
    #[serde(default)]
    pub detail: DetailLevel,
    pub job_id: JobId,
    pub action: JobAction,
    pub wait_ms: Option<u64>,
    pub section: Option<AnalysisSection>,
    #[serde(default)]
    pub offset: usize,
    pub limit: Option<usize>,
    pub measure_range: Option<MeasureRange>,
    pub voice: Option<String>,
    pub track: Option<usize>,
    pub channel: Option<u8>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocateArgs {
    pub source_hash: ContentHash,
    pub page: u32,
    pub x: f64,
    pub y: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct LocateOutcome {
    pub capability: LocationCapability,
    pub reason: Option<String>,
    pub region: Option<LocatedRegion>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EmptyArgs {}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct RpcRequest {
    pub method: String,
    pub params: serde_json::Value,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct RpcResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<AppError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct EditActivity {
    pub author: String,
    pub status: EditStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct GeometryBounds {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct GeometryRegion {
    pub locator: Locator,
    pub system_index: usize,
    pub bounds: GeometryBounds,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct GeometryPage {
    pub page: u32,
    pub view_box: Option<GeometryBounds>,
    pub capability: LocationCapability,
    pub reason: Option<String>,
    pub regions: Vec<GeometryRegion>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct RenderGeometry {
    pub render_key: ContentHash,
    pub source_hash: ContentHash,
    pub pages: Vec<GeometryPage>,
}

/// One rendered staff within a system, in SVG user units.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct LayoutStaff {
    pub top: f64,
    pub bot: f64,
    pub x0: f64,
    pub x1: f64,
}

/// One rendered system and its staves.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct LayoutSystem {
    pub top: f64,
    pub bot: f64,
    pub bars: usize,
    pub measure_start: Option<u32>,
    pub measure_end: Option<u32>,
    pub staves: Vec<LayoutStaff>,
}

/// Reliability of source-measure mapping for rendered pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum MeasurementCapability {
    None,
    Partial,
    Complete,
}

/// Rendered page structure independent of measure regions.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct LayoutPage {
    pub page: u32,
    pub view_box: Option<GeometryBounds>,
    pub staff_space: f64,
    pub systems: Vec<LayoutSystem>,
    pub capability: LocationCapability,
    pub measure_mapping: MeasurementCapability,
    pub reason: Option<String>,
}

/// Page, system and staff structure from one immutable score snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct LayoutOutcome {
    pub job_id: JobId,
    pub document_id: DocumentId,
    pub source_hash: ContentHash,
    pub page_count: usize,
    pub system_count: usize,
    pub staff_count: usize,
    pub measure_mapping: MeasurementCapability,
    pub pages: Vec<LayoutPage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayoutArgs {
    pub job_id: JobId,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct EngineHealth {
    pub ok: bool,
    pub build_id: Option<String>,
    pub error: Option<AppError>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeHealth {
    pub ok: bool,
    pub info: Option<RuntimeInfo>,
    pub error: Option<AppError>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct DoctorReport {
    #[serde(default)]
    pub documentation: Option<DocumentationStatus>,
    pub home: String,
    pub engine: EngineHealth,
    pub runtime: RuntimeHealth,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UiTheme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema, TS, PartialEq, Eq)]
pub enum UiLanguage {
    #[serde(rename = "en-US")]
    #[default]
    EnUs,
    #[serde(rename = "zh-CN")]
    ZhCn,
}

impl UiLanguage {
    pub fn from_locale(locale: &str) -> Option<Self> {
        let primary = locale
            .split(['-', '_'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        match primary.as_str() {
            "en" => Some(Self::EnUs),
            "zh" => Some(Self::ZhCn),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::EnUs => "en-US",
            Self::ZhCn => "zh-CN",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TrayClickAction {
    Menu,
    #[default]
    Window,
}

impl TrayClickAction {
    pub fn effective(self) -> Self {
        if cfg!(target_os = "linux") {
            Self::Menu
        } else {
            self
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiPreferences {
    pub theme: UiTheme,
    pub language: Option<UiLanguage>,
    pub tray_click_action: TrayClickAction,
    #[serde(default)]
    pub storage_cache_limit: StorageCacheLimit,
    #[serde(default = "default_storage_auto_cleanup")]
    pub storage_auto_cleanup: bool,
}

fn default_storage_auto_cleanup() -> bool {
    true
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            theme: UiTheme::System,
            language: None,
            tray_click_action: TrayClickAction::Window,
            storage_cache_limit: StorageCacheLimit::Gib1,
            storage_auto_cleanup: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ManagementPreferences {
    pub theme: UiTheme,
    pub language: Option<UiLanguage>,
    pub tray_click_action: TrayClickAction,
    pub tray_click_action_editable: bool,
    pub storage_cache_limit: StorageCacheLimit,
    pub storage_auto_cleanup: bool,
    pub launch_at_login: bool,
    pub launch_at_login_editable: bool,
    pub app_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagementPreferencesChange {
    pub theme: UiTheme,
    pub language: Option<UiLanguage>,
    pub tray_click_action: TrayClickAction,
    pub storage_cache_limit: StorageCacheLimit,
    pub storage_auto_cleanup: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ManagementSnapshot {
    pub home: String,
    pub engine_root: String,
    pub runtime: Option<RuntimeInfo>,
    pub error: Option<AppError>,
}
