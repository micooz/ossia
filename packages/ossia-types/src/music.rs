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
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS,
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

/// Predicted written pitches; repeats and tie merging are not performed here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct WrittenEvent {
    pub voice: String,
    pub span: SourceSpan,
    pub pitches: Vec<i32>,
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
}

/// Performance event coverage is independent of written-source coverage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceAnalysis {
    pub status: AnalysisStatus,
    pub reasons: Vec<String>,
    pub notes: Vec<PerformedNote>,
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

/// One mismatching or unavailable condition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct VerificationDifference {
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
}

/// A check's evidence is stored separately from the engine render manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub render: RenderManifest,
    pub music: MusicReport,
    pub verification: Verification,
}
