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
    pub view_id: ViewId,
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
pub struct ViewSnapshot {
    pub instance_id: String,
    pub seq: u64,
    pub view_id: ViewId,
    pub document: Option<DocumentState>,
    pub selection: Option<Selection>,
    pub regions: Vec<LocatedRegion>,
    pub render: Option<RenderReference>,
    pub edit_activity: Option<EditActivity>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct OpenView {
    pub view_id: ViewId,
    pub url: String,
    pub ticket: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeRequest {
    pub ticket: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeResponse {
    pub token: String,
    pub view: ViewSnapshot,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub language: Option<UiLanguage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEvent {
    pub instance_id: String,
    pub seq: u64,
    pub r#type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub view_id: Option<ViewId>,
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
    pub title: String,
    pub format: InputFormat,
    pub path: String,
    pub confirmation: Option<ContentHash>,
    pub author: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenArgs {
    pub document_id: Option<DocumentId>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextArgs {
    #[serde(default)]
    pub detail: DetailLevel,
    pub view_id: ViewId,
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
    pub view_id: ViewId,
    pub base_version_id: Option<VersionId>,
    pub expected_source_hash: ContentHash,
    pub edit_id: Option<EditId>,
    #[serde(default)]
    pub adopt_workspace: bool,
    #[serde(default = "default_author")]
    pub author: String,
}
fn default_author() -> String {
    "agent".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckArgs {
    #[serde(default)]
    pub expect: Option<MusicExpectation>,
    pub document_id: DocumentId,
    pub view_id: ViewId,
    pub edit_id: Option<EditId>,
    pub expected_source_hash: ContentHash,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommitArgs {
    pub document_id: DocumentId,
    pub view_id: ViewId,
    pub edit_id: EditId,
    pub base_version_id: Option<VersionId>,
    pub expected_source_hash: ContentHash,
    pub summary: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoryArgs {
    pub document_id: DocumentId,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestoreArgs {
    pub document_id: DocumentId,
    pub version_id: VersionId,
    pub base_version_id: Option<VersionId>,
    pub expected_source_hash: ContentHash,
    pub author: String,
    pub summary: String,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "kebab-case")]
pub enum ExportFormat {
    Ly,
    Pdf,
    Midi,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportArgs {
    pub document_id: DocumentId,
    pub version_id: VersionId,
    pub format: ExportFormat,
    pub output_path: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ExportOutcome {
    pub document_id: DocumentId,
    pub version_id: VersionId,
    pub source_hash: ContentHash,
    pub format: ExportFormat,
    pub path: String,
    pub bytes: u64,
    pub sha256: ContentHash,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
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
#[serde(rename_all = "camelCase")]
pub struct StatusSnapshot {
    pub runtime: RuntimeInfo,
    pub engine_build_id: String,
    pub jobs: Vec<Job>,
    pub instance_id: String,
    pub seq: u64,
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
    pub owned_by_view: bool,
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
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            theme: UiTheme::System,
            language: None,
            tray_click_action: TrayClickAction::Window,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ManagementSnapshot {
    pub home: String,
    pub engine_root: String,
    pub runtime: Option<RuntimeInfo>,
    pub error: Option<AppError>,
}
