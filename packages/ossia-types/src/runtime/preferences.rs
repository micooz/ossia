use crate::*;
use schemars::JsonSchema;
use ts_rs::TS;

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

/// Initial management window behavior when the desktop application starts.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum StartupAction {
    #[default]
    Window,
    Tray,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiPreferences {
    pub theme: UiTheme,
    pub language: Option<UiLanguage>,
    pub tray_click_action: TrayClickAction,
    #[serde(default)]
    pub startup_action: StartupAction,
    #[serde(default)]
    pub storage_cache_limit: StorageCacheLimit,
    #[serde(default = "default_storage_auto_cleanup")]
    pub storage_auto_cleanup: bool,
    #[serde(default)]
    pub lan_access: bool,
    #[serde(default = "default_auto_download")]
    pub auto_download: bool,
    #[serde(default)]
    pub update_source: UpdateSource,
    #[serde(default)]
    pub update_channel: UpdateChannelPreference,
    /// Persisted only; management UI receives a masked snapshot.
    #[ts(skip)]
    #[serde(default)]
    pub github_token: Option<String>,
    #[serde(default)]
    pub omr_service: OmrService,
    /// None denotes preferences saved before the explicit OMR switch existed.
    #[serde(default)]
    pub omr_enabled: Option<bool>,
    /// Custom OMR server address; unused by the built-in Ossia OMR service.
    #[serde(default)]
    pub omr_url: Option<String>,
    /// Persisted only; management UI receives a masked snapshot.
    #[ts(skip)]
    #[serde(default)]
    pub omr_token: Option<String>,
    /// Persisted only; management UI receives a masked snapshot.
    #[ts(skip)]
    #[serde(default)]
    pub omr_custom_token: Option<String>,
}

fn default_storage_auto_cleanup() -> bool {
    true
}

fn default_auto_download() -> bool {
    true
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            theme: UiTheme::System,
            language: None,
            tray_click_action: TrayClickAction::Window,
            startup_action: StartupAction::Window,
            storage_cache_limit: StorageCacheLimit::Gib1,
            storage_auto_cleanup: true,
            lan_access: false,
            auto_download: true,
            update_source: UpdateSource::Ossia,
            update_channel: UpdateChannelPreference::Auto,
            github_token: None,
            omr_service: OmrService::Custom,
            omr_enabled: Some(false),
            omr_url: None,
            omr_token: None,
            omr_custom_token: None,
        }
    }
}

impl UiPreferences {
    /// Resolve OMR state for both current and legacy preferences.
    pub fn omr_enabled(&self) -> bool {
        self.omr_enabled
            .unwrap_or(self.omr_service == OmrService::Custom || self.omr_token.is_some())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ManagementPreferences {
    pub theme: UiTheme,
    pub language: Option<UiLanguage>,
    pub tray_click_action: TrayClickAction,
    pub startup_action: StartupAction,
    pub tray_click_action_editable: bool,
    pub storage_cache_limit: StorageCacheLimit,
    pub storage_auto_cleanup: bool,
    pub lan_access: bool,
    pub auto_download: bool,
    pub update_channel: UpdateChannel,
    pub update_channel_preference: UpdateChannelPreference,
    pub update_source: UpdateSource,
    pub github_token_masked: Option<String>,
    pub omr_service: OmrService,
    pub omr_enabled: bool,
    pub omr_ossia_url: String,
    pub omr_url: Option<String>,
    pub omr_token_masked: Option<String>,
    pub omr_custom_token_masked: Option<String>,
    pub launch_at_login: bool,
    pub launch_at_login_editable: bool,
    pub app_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagementPreferencesChange {
    #[serde(default)]
    #[ts(optional)]
    pub theme: Option<UiTheme>,
    #[serde(default)]
    #[ts(optional)]
    pub language: Option<UiLanguage>,
    #[serde(default)]
    #[ts(optional)]
    pub tray_click_action: Option<TrayClickAction>,
    #[serde(default)]
    #[ts(optional)]
    pub startup_action: Option<StartupAction>,
    #[serde(default)]
    #[ts(optional)]
    pub storage_cache_limit: Option<StorageCacheLimit>,
    #[serde(default)]
    #[ts(optional)]
    pub storage_auto_cleanup: Option<bool>,
    #[serde(default)]
    #[ts(optional)]
    pub lan_access: Option<bool>,
    #[serde(default)]
    #[ts(optional)]
    pub auto_download: Option<bool>,
    #[serde(default)]
    #[ts(optional)]
    pub update_channel: Option<UpdateChannelPreference>,
    #[serde(default)]
    #[ts(optional)]
    pub update_source: Option<UpdateSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct ManagementSnapshot {
    pub home: String,
    pub engine_root: String,
    pub runtime: Option<RuntimeInfo>,
    pub error: Option<AppError>,
}
