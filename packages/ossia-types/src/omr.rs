use crate::*;

/// Fixed remote OMR endpoint provided by Ossia.
pub const OSSIA_OMR_URL: &str = "https://ossia.maisow.com/omr";

/// Remote OMR endpoint selected by the user.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    ts_rs::TS,
)]
#[serde(rename_all = "snake_case")]
pub enum OmrService {
    #[default]
    Ossia,
    Custom,
}

/// Input formats accepted by the remote Audiveris OMR service.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "lowercase")]
pub enum OmrSourceFormat {
    Pdf,
    Png,
    Jpeg,
    Tiff,
}

impl From<OmrSourceFormat> for InputFormat {
    fn from(format: OmrSourceFormat) -> Self {
        match format {
            OmrSourceFormat::Pdf => Self::Pdf,
            OmrSourceFormat::Png => Self::Png,
            OmrSourceFormat::Jpeg => Self::Jpeg,
            OmrSourceFormat::Tiff => Self::Tiff,
        }
    }
}

impl TryFrom<InputFormat> for OmrSourceFormat {
    type Error = AppError;

    fn try_from(format: InputFormat) -> Result<Self, Self::Error> {
        match format {
            InputFormat::Pdf => Ok(Self::Pdf),
            InputFormat::Png => Ok(Self::Png),
            InputFormat::Jpeg => Ok(Self::Jpeg),
            InputFormat::Tiff => Ok(Self::Tiff),
            _ => Err(AppError::unsupported(
                "input format is not supported by OMR",
            )),
        }
    }
}

/// Durable state of one remote OMR job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct OmrJob {
    pub job_id: JobId,
    pub state: JobState,
    pub phase: String,
    pub queue_position: Option<u32>,
    pub source_format: OmrSourceFormat,
    pub input_hash: ContentHash,
    pub output_hash: Option<ContentHash>,
    pub audiveris_version: Option<String>,
    pub languages: Vec<String>,
    #[serde(default)]
    pub diagnostics: Vec<Diagnostic>,
    pub error: Option<AppError>,
}

/// Public capabilities of one remote OMR service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct OmrCapabilities {
    pub service_version: String,
    pub audiveris_version: String,
    pub formats: Vec<OmrSourceFormat>,
    pub languages: Vec<String>,
    pub max_upload_bytes: u64,
    pub max_job_seconds: u64,
}

/// Remote OMR configuration submitted by the management window.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OmrSettingsChange {
    #[serde(default)]
    #[ts(optional)]
    pub enabled: Option<bool>,
    pub service: OmrService,
    pub url: Option<String>,
    pub token: Option<String>,
    #[serde(default)]
    pub clear_url: bool,
    #[serde(default)]
    pub clear_token: bool,
}
