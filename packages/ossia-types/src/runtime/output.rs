use crate::*;
use schemars::JsonSchema;
use ts_rs::TS;

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
    Wav,
    MusicXml,
}

/// A visual region selected from an immutable rendered score.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum OutputCrop {
    Measures {
        range: MeasureRange,
        staff: Option<usize>,
    },
    System {
        page: u32,
        system_index: usize,
        staff: Option<usize>,
    },
}

/// Write one output file or a page batch from a completed job snapshot or an immutable version.
/// PDF may use an open password; PNG may use lossy quality and transparency options.
/// MusicXML exports editable notation for the whole snapshot.
/// MIDI and WAV may select an inclusive one-based measure range; WAV contains the playback mix.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutputArgs {
    pub source: OutputSource,
    pub format: OutputFormat,
    /// One-based PDF/SVG/PNG pages; None selects all PDF pages or page 1 for SVG/PNG.
    pub pages: Option<Vec<u32>>,
    pub width: Option<u32>,
    pub pdf_password: Option<String>,
    pub png_quality: Option<u8>,
    pub png_transparent: Option<bool>,
    /// Include the document's current freehand drawing in visual output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub include_drawing: Option<bool>,
    /// Absolute file path. Omit to write a transient file in the system temporary directory.
    pub output_path: Option<String>,
    /// Absolute parent directory for a multi-page or crop batch. Omit to use the system temporary directory.
    pub output_directory: Option<String>,
    pub crop: Option<OutputCrop>,
    pub padding_staff_spaces: Option<f64>,
    /// Inclusive measures for MIDI/WAV snippets; None exports the complete snapshot.
    pub audio_range: Option<MeasureRange>,
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

/// One verified file produced by a visual crop batch.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct OutputFile {
    pub page: u32,
    pub bounds: GeometryBounds,
    pub path: String,
    pub bytes: u64,
    pub sha256: ContentHash,
    pub media_type: String,
}

/// A complete, atomically published visual crop batch.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
pub struct OutputBatchOutcome {
    pub source: OutputSource,
    pub document_id: DocumentId,
    pub source_hash: ContentHash,
    pub engine_build_id: String,
    pub format: OutputFormat,
    pub directory: String,
    pub files: Vec<OutputFile>,
}
