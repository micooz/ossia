//! Evidence and expectations for written music and performed MIDI events.
use crate::*;

/// Coverage of an analysis; an empty unavailable result is not an empty score.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "kebab-case")]
pub enum AnalysisStatus {
    Complete,
    Partial,
    Unavailable,
}

/// Exact nonnegative time measured in quarter notes. Denominator must be nonzero.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    ts_rs::TS,
)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QuarterTime {
    pub numerator: u64,
    pub denominator: u64,
}
impl QuarterTime {
    pub fn new(numerator: u64, denominator: u64) -> Result<Self, AppError> {
        if denominator == 0 {
            return Err(AppError::invalid(
                "quarter time denominator must be positive",
            ));
        }
        let (mut a, mut b) = (numerator, denominator);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        Ok(Self {
            numerator: numerator / a,
            denominator: denominator / a,
        })
    }
    pub fn equivalent(self, other: Self) -> bool {
        self.denominator > 0
            && other.denominator > 0
            && u128::from(self.numerator) * u128::from(other.denominator)
                == u128::from(other.numerator) * u128::from(self.denominator)
    }
}

/// Predicted sounding pitches plus the literal source tokens that produced them.
/// Repeats and tie merging are not performed here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct WrittenEvent {
    pub voice: String,
    pub span: SourceSpan,
    pub pitches: Vec<i32>,
    /// Literal source note tokens, including duration and articulation text.
    #[serde(default)]
    pub pitch_tokens: Vec<String>,
    /// One-based source measure, when the source grid is reliable.
    #[serde(default)]
    pub measure: Option<u32>,
}

/// Counts are absent when the source cannot be reliably indexed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct SourceAnalysis {
    pub status: AnalysisStatus,
    pub reasons: Vec<String>,
    pub measures: Option<u32>,
    pub voices: Option<usize>,
    pub note_tokens: Option<usize>,
    pub events: Vec<WrittenEvent>,
}

/// A MIDI note-on with its key release, excluding pedal acoustics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct PerformedNote {
    pub track: usize,
    pub channel: u8,
    pub pitch: u8,
    pub start: Option<QuarterTime>,
    pub duration: Option<QuarterTime>,
    /// Canonical sharp spelling for the MIDI pitch.
    #[serde(default)]
    pub pitch_name: String,
    /// One-based measure on the performed MIDI timeline.
    #[serde(default)]
    pub measure: Option<u32>,
}

/// Performance event coverage is independent of written-source coverage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceAnalysis {
    pub status: AnalysisStatus,
    pub reasons: Vec<String>,
    pub notes: Vec<PerformedNote>,
    /// Coverage of MIDI time-signature based measure assignment.
    #[serde(default = "unavailable_analysis_status")]
    pub measure_status: AnalysisStatus,
    #[serde(default)]
    pub measure_reason: Option<String>,
}

fn unavailable_analysis_status() -> AnalysisStatus {
    AnalysisStatus::Unavailable
}

/// Source and MIDI evidence from one immutable source snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct MusicReport {
    pub source: SourceAnalysis,
    pub performance: PerformanceAnalysis,
}

/// Expected MIDI pitch and exact quarter-note timing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExpectedNote {
    pub pitch: u8,
    pub start: QuarterTime,
    pub duration: QuarterTime,
}

/// An exact ordered sequence in one MIDI track/channel; simultaneous pitches sort ascending.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TrackExpectation {
    pub track: usize,
    pub channel: u8,
    pub pitches: Option<Vec<u8>>,
    pub notes: Option<Vec<ExpectedNote>>,
}

/// Only explicitly supplied conditions are checked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MusicExpectation {
    pub note_count: Option<usize>,
    #[serde(default)]
    pub tracks: Vec<TrackExpectation>,
}

/// Immutable source addressed by an evidence or comparison request.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum EvidenceSource {
    Job { job_id: JobId },
    Version { version_id: VersionId },
}

/// Evidence page to return, or a summary without any event arrays.
#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    ts_rs::TS,
)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceSection {
    #[default]
    Summary,
    Performance,
    Source,
}

/// Compact coverage information for one immutable evidence report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceSummary {
    pub source_status: AnalysisStatus,
    pub source_reasons: Vec<String>,
    pub measures: Option<u32>,
    pub voices: Option<usize>,
    pub note_tokens: Option<usize>,
    pub source_events: usize,
    pub performance_status: AnalysisStatus,
    pub performance_reasons: Vec<String>,
    pub performed_notes: usize,
    pub performance_measures: Option<u32>,
    pub measure_reason: Option<String>,
}

/// Derived music evidence bound to one source snapshot and engine build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceReport {
    pub document_id: DocumentId,
    pub source_hash: ContentHash,
    pub engine_build_id: String,
    pub analysis_version: u32,
    pub music: MusicReport,
}

/// Read bounded evidence from an immutable version or completed job snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidenceArgs {
    pub source: EvidenceSource,
    #[serde(default)]
    pub section: EvidenceSection,
    #[serde(default)]
    pub offset: usize,
    pub limit: Option<usize>,
    pub wait_ms: Option<u64>,
    pub measure_range: Option<MeasureRange>,
    pub voice: Option<String>,
    pub track: Option<usize>,
    pub channel: Option<u8>,
}

/// Inclusive one-based source or performance measure range.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MeasureRange {
    pub start: u32,
    pub end: u32,
}

impl MeasureRange {
    pub fn validate(self) -> Result<Self, AppError> {
        if self.start == 0 || self.end < self.start {
            return Err(AppError::invalid("measureRange requires 1 <= start <= end"));
        }
        Ok(self)
    }
}

/// One bounded evidence page, including its immutable provenance.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct EvidencePage {
    pub source: EvidenceSource,
    pub document_id: DocumentId,
    pub source_hash: ContentHash,
    pub engine_build_id: String,
    pub analysis_version: u32,
    pub section: EvidenceSection,
    pub summary: EvidenceSummary,
    pub total: usize,
    pub offset: usize,
    pub next_offset: Option<usize>,
    pub items: Vec<serde_json::Value>,
}

/// A cached evidence page, or a durable job that is still producing it.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceOutcome {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub job: Option<Job>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub page: Option<EvidencePage>,
}

/// Which performed-note fields participate in reference equality.
#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    ts_rs::TS,
)]
#[serde(rename_all = "kebab-case")]
pub enum ComparisonMode {
    /// Pitch, start and duration; track order and channel are ignored.
    #[default]
    Performance,
    /// Performance fields plus track and channel.
    Tracks,
}

/// Reference performance used to verify a checked score.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReferenceExpectation {
    pub source: EvidenceSource,
    #[serde(default)]
    pub comparison: ComparisonMode,
}

/// Durable reference data recorded with a check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceMusic {
    pub source: EvidenceSource,
    pub source_hash: ContentHash,
    pub engine_build_id: String,
    pub comparison: ComparisonMode,
    pub performance: PerformanceAnalysis,
}

/// Provenance and aggregate counts for a reference comparison.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceVerification {
    pub source: EvidenceSource,
    pub source_hash: ContentHash,
    pub engine_build_id: String,
    pub comparison: ComparisonMode,
    pub expected_notes: usize,
    pub actual_notes: usize,
    pub missing: usize,
    pub extra: usize,
    pub differences_truncated: bool,
}

/// Compilation success does not imply that an expectation passed.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
)]
#[serde(rename_all = "kebab-case")]
pub enum VerificationStatus {
    NotRequested,
    Passed,
    Failed,
    Unavailable,
}

/// Machine-readable classification of one verification difference.
#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
    ts_rs::TS,
)]
#[serde(rename_all = "kebab-case")]
pub enum VerificationDifferenceKind {
    #[default]
    Mismatch,
    Missing,
    Extra,
    Unavailable,
}

/// One mismatching or unavailable condition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct VerificationDifference {
    #[serde(default)]
    pub kind: VerificationDifferenceKind,
    pub path: String,
    pub expected: serde_json::Value,
    pub actual: serde_json::Value,
    pub message: String,
}

/// Verification of the explicitly declared musical intent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct Verification {
    pub status: VerificationStatus,
    pub differences: Vec<VerificationDifference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reference: Option<ReferenceVerification>,
}

/// A check's evidence is stored separately from the engine render manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    #[serde(default)]
    pub analysis_version: u32,
    pub render: RenderManifest,
    pub music: MusicReport,
    pub verification: Verification,
}
