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
/// Embedded workflow shared by the CLI and MCP.
pub const GUIDE: &str = include_str!("guide.md");
