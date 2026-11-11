mod browser;
pub mod cli;
mod core;
pub mod http;
pub mod ipc;
pub mod mcp;
pub mod preferences;
pub mod server;
pub use core::*;
pub use ossia_types::*;
use serde::Serialize;
use serde_json::Value;

pub type Result<T> = std::result::Result<T, AppError>;
pub(crate) fn storage(error: impl std::fmt::Display) -> AppError {
    AppError::new(ErrorCode::StorageError, error.to_string())
}
pub(crate) fn invalid(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::InvalidRequest, message)
}
pub(crate) fn value<T: Serialize>(data: T) -> Result<Value> {
    serde_json::to_value(data).map_err(storage)
}
pub(crate) fn decode<T: serde::de::DeserializeOwned>(data: Value) -> Result<T> {
    serde_json::from_value(data).map_err(|e| invalid(e.to_string()))
}
pub const GUIDE: &str = "Ossia edits one self-contained LilyPond file locally. Import/list, then open a view; use its explicit viewId for context and editing. Begin with current baseVersionId and expectedSourceHash; keep editId private. Edit sourcePath using your file tools. Check the exact new hash, poll ossia_job until done, inspect diagnostics, then commit with editId, baseVersionId, expectedSourceHash and summary. Auto-preview does not authorize commit. End preserves files; discard is explicit and hash-guarded. Resume requires the original interrupted editId. Lossy imports need the returned confirmation hash; refused inputs cannot be forced. History and export address versionId; restore appends history and requires a clean workspace. Locations use UTF-8 byte spans, one-based pages/measures, zero-based staves and SVG user units. MIDI/token differences are diagnostics, not proof of musical intent. Runtime restarts invalidate views and page tokens; re-open, then explicitly resume interrupted work. The browser shows score previews and history. Host integration is managed from the desktop window or `ossia integrate`; installed Codex and DSH entries launch this same local runtime.";
