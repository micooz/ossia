mod agent;
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
pub(crate) fn validate_wait_ms(value: Option<u64>) -> Result<u64> {
    let value = value.unwrap_or(0);
    if value > 30_000 {
        return Err(invalid("waitMs must be 0..30000"));
    }
    Ok(value)
}

pub(crate) fn filter_music_items(
    items: &[Value],
    source: bool,
    measure_range: Option<MeasureRange>,
    voice: Option<&str>,
    track: Option<usize>,
    channel: Option<u8>,
) -> Result<Vec<Value>> {
    if let Some(range) = measure_range {
        range.validate()?;
    }
    if source && (track.is_some() || channel.is_some()) {
        return Err(invalid(
            "track and channel filters are valid only for performance evidence",
        ));
    }
    if channel.is_some_and(|channel| channel > 15) {
        return Err(invalid("channel filter requires 0..15"));
    }
    if !source && voice.is_some() {
        return Err(invalid("voice filter is valid only for source evidence"));
    }
    Ok(items
        .iter()
        .filter(|item| {
            if let Some(range) = measure_range {
                let measure = item
                    .get("measure")
                    .and_then(Value::as_u64)
                    .and_then(|measure| u32::try_from(measure).ok());
                if measure.is_none_or(|measure| measure < range.start || measure > range.end) {
                    return false;
                }
            }
            if source {
                if let Some(voice) = voice {
                    if item.get("voice").and_then(Value::as_str) != Some(voice) {
                        return false;
                    }
                }
            } else {
                if let Some(track) = track {
                    if item.get("track").and_then(Value::as_u64) != Some(track as u64) {
                        return false;
                    }
                }
                if let Some(channel) = channel {
                    if item.get("channel").and_then(Value::as_u64) != Some(u64::from(channel)) {
                        return false;
                    }
                }
            }
            true
        })
        .cloned()
        .collect())
}

pub(crate) fn has_music_filters(
    measure_range: Option<MeasureRange>,
    voice: Option<&str>,
    track: Option<usize>,
    channel: Option<u8>,
) -> bool {
    measure_range.is_some() || voice.is_some() || track.is_some() || channel.is_some()
}
/// Embedded workflow shared by the CLI and MCP.
pub const GUIDE: &str = include_str!("guide.md");
