use crate::{storage, Result, UiPreferences};
use std::{
    io::Write,
    net::Ipv4Addr,
    path::{Path, PathBuf},
};

const FILE: &str = "preferences.json";

fn path(home: &Path) -> PathBuf {
    home.join(FILE)
}

/// Validate a custom remote OMR URL, optionally with a path prefix.
pub fn validate_omr_url(raw: &str) -> Result<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(crate::invalid("Remote OMR URL is required"));
    }
    if raw.ends_with('/') {
        return Err(crate::invalid("Remote OMR URL must not end with a slash"));
    }
    let url = reqwest::Url::parse(raw)
        .map_err(|_| crate::invalid("Remote OMR URL is not a valid URL"))?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(crate::invalid(
            "Remote OMR URL must not contain credentials",
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(crate::invalid(
            "Remote OMR URL must not contain a query or fragment",
        ));
    }
    if url.path() != "/"
        && url.path_segments().is_none_or(|mut segments| {
            segments.any(|segment| segment.is_empty() || segment == "." || segment == "..")
        })
    {
        return Err(crate::invalid(
            "Remote OMR URL must use a clean path prefix",
        ));
    }
    let https = url.scheme() == "https";
    let loopback_http = url.scheme() == "http"
        && url.host_str().is_some_and(|host| {
            host == "localhost"
                || host == "::1"
                || host
                    .parse::<Ipv4Addr>()
                    .is_ok_and(|address| address.is_loopback())
        });
    if !https && !loopback_http {
        return Err(crate::invalid(
            "Remote OMR URL must use HTTPS unless it targets loopback",
        ));
    }
    Ok(raw.to_string())
}

/// Normalize a remote OMR bearer token without accepting line breaks.
pub fn normalize_omr_token(raw: &str) -> Result<String> {
    normalize_secret(raw, "Remote OMR token")
}

/// Normalize an optional GitHub token used by the desktop updater.
pub fn normalize_github_token(raw: &str) -> Result<String> {
    normalize_secret(raw, "GitHub token")
}

pub fn masked_token(token: Option<&str>) -> Option<String> {
    token.map(|token| {
        let suffix = token
            .chars()
            .rev()
            .take(4)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        format!("••••••••{suffix}")
    })
}

fn normalize_secret(raw: &str, name: &str) -> Result<String> {
    let token = raw.trim();
    if token.is_empty() {
        return Err(crate::invalid(format!("{name} is required")));
    }
    if token.len() > 4096
        || token
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
    {
        return Err(crate::invalid(format!(
            "{name} contains invalid characters"
        )));
    }
    Ok(token.to_string())
}

/// Load the data-directory-scoped UI preferences, or defaults when they do not exist.
pub fn load(home: &Path) -> Result<UiPreferences> {
    let path = path(home);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(UiPreferences::default());
        }
        Err(error) => return Err(storage(error)),
    };
    serde_json::from_slice(&bytes).map_err(|error| {
        storage(format!(
            "cannot parse UI preferences at {}: {error}",
            path.display()
        ))
    })
}

/// Serialize preference read-modify-write transactions across desktop and backend processes.
pub fn lock(home: &Path) -> Result<std::fs::File> {
    use fs2::FileExt;
    std::fs::create_dir_all(home).map_err(storage)?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(home.join("preferences.lock"))
        .map_err(storage)?;
    file.lock_exclusive().map_err(storage)?;
    Ok(file)
}

/// Atomically replace the data-directory-scoped UI preferences.
pub fn save(home: &Path, preferences: &UiPreferences) -> Result<()> {
    std::fs::create_dir_all(home).map_err(storage)?;
    let mut temporary = tempfile::NamedTempFile::new_in(home).map_err(storage)?;
    serde_json::to_writer_pretty(&mut temporary, preferences).map_err(storage)?;
    temporary.write_all(b"\n").map_err(storage)?;
    temporary.as_file().sync_all().map_err(storage)?;
    temporary
        .persist(path(home))
        .map_err(|error| storage(error.error))?;
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/preferences.rs"]
mod tests;
