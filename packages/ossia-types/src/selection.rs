use crate::*;

/// A source object selected on the score; element IDs belong to one source hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum SelectionTarget {
    Measure { locator: Locator },
    Element { element_id: String },
}

/// Authoritative edit context for one selected object, including generated symbols.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct SelectionRegion {
    pub target: SelectionTarget,
    pub label: String,
    pub element_kind: Option<String>,
    pub spans: Vec<SourceSpan>,
    pub insertion_anchor: Option<SourceSpan>,
    pub staff_index: Option<usize>,
    pub voice: Option<String>,
    pub reason: Option<String>,
    pub occurrences: Vec<SelectionOccurrence>,
}

/// A printed location affected by editing the selected source object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct SelectionOccurrence {
    pub page: u32,
    pub staff_index: Option<usize>,
    pub bounds: GeometryBounds,
}

/// A visible occurrence of an element. Several occurrences can share one target.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct GeometryElement {
    pub region: SelectionRegion,
    pub bounds: GeometryBounds,
    /// Local symbols precede spanning symbols during hit testing.
    pub priority: u8,
    /// Ink hit regions for curves; empty means the element's full bounds.
    pub hit_bounds: Vec<GeometryBounds>,
    /// Actual SVG groups colored when this source object is selected.
    pub graphic_ids: Vec<String>,
    /// Authoritative note-chain and attachment selection, including this target.
    pub linked_targets: Vec<SelectionTarget>,
}
