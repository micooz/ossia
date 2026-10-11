// Ossia desktop shell (Tauri 2).
//
// The shell spawns nothing. Every piece of server logic lives in packages/backend (Next.js), and
// this crate only has to tell the embedded SPA which deployment to talk to:
//   * The UI is one static SPA (packages/ui) embedded in the app bundle by tauri-codegen. It is
//     never fetched from a server: remote *data* is fine, a remote *UI* is what gets an iOS build
//     rejected (App Store 2.5.2), so the same bundle works on every platform.
//   * The API host is configuration, not discovery. Precedence: `OSSIA_API_BASE` (dev, scripts),
//     then `<app_data>/config.json` (a user-editable file, the only knob that survives a double-click
//     from Finder), then loopback dev as the default.
//
// The `.ly` sources, the LilyPond engine and the version chain all stay server-side (G6/D9): the
// shell never touches them, so it needs no hardened-runtime code-execution exceptions.
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The only deployment that exists today is the local server (`pnpm dev:web`), so that is the
/// default. A packaged build points elsewhere through config.
const DEFAULT_API_BASE: &str = "http://127.0.0.1:3210";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Endpoint {
    base_url: String,
    token: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Config {
    /// Which API to talk to. Absent means loopback dev.
    api_base: Option<String>,
    /// The shared secret the server expects in `x-ossia-token`. Empty means unauthenticated, which
    /// the server only ever accepts from a loopback Host.
    api_token: Option<String>,
}

/// Strip trailing slashes so the SPA never builds `//api/v1/...`.
fn normalize_base(raw: &str) -> String {
    raw.trim_end_matches('/').to_string()
}

/// Reject anything that is not an http(s) origin before it reaches the webview's fetch().
fn is_http_origin(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

/// Parse the user-editable config, reporting a malformed file instead of silently ignoring it.
fn parse_config(text: &str) -> Result<Config, String> {
    serde_json::from_str(text).map_err(|e| format!("config.json is not valid JSON: {e}"))
}

impl Config {
    /// Merge: env beats file, file beats default; the first non-empty token wins.
    fn resolve(self, app_data: &Path) -> Result<Endpoint, String> {
        let base = std::env::var("OSSIA_API_BASE")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .or(self.api_base)
            .unwrap_or_else(|| DEFAULT_API_BASE.into());
        let base_url = normalize_base(&base);
        if !is_http_origin(&base_url) {
            // Deliberately does not echo the value: a base URL can carry credentials
            // (`https://user:***@host`), and this string reaches the webview as an error.
            let scheme = base_url.split("://").next().unwrap_or("").to_string();
            return Err(format!("apiBase must be an http(s) URL, got scheme {scheme:?}"));
        }
        let token = std::env::var("OSSIA_API_TOKEN")
            .ok()
            .filter(|s| !s.is_empty())
            .or(self.api_token)
            .unwrap_or_default();
        // `token` is a secret: it never appears in an error path.
        let _ = app_data;
        Ok(Endpoint { base_url, token })
    }
}

/// `<app_data>/config.json`, if the user wrote one.
fn config_path(app_data: &Path) -> PathBuf {
    app_data.join("config.json")
}

/// The UI's only bootstrap call: which API to talk to, plus the secret the server expects.
#[tauri::command]
async fn get_endpoint(app: tauri::AppHandle) -> Result<Endpoint, String> {
    use tauri::Manager;
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("no app data dir: {e}"))?;
    std::fs::create_dir_all(&app_data).map_err(|e| format!("cannot create app data dir: {e}"))?;
    let file = config_path(&app_data);
    let cfg = match std::fs::read_to_string(&file) {
        Ok(text) if text.trim().is_empty() => Config::default(),
        Ok(text) => parse_config(&text)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Config::default(),
        Err(e) => return Err(format!("cannot read config.json: {e}")),
    };
    cfg.resolve(&app_data)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_endpoint])
        .run(tauri::generate_context!())
        .expect("error while running Ossia");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_base_removes_trailing_slashes() {
        assert_eq!(normalize_base("http://127.0.0.1:3210/"), "http://127.0.0.1:3210");
        assert_eq!(normalize_base("https://api.ossia.dev"), "https://api.ossia.dev");
    }

    #[test]
    fn empty_config_falls_back_to_loopback_dev() {
        let e = Config::default().resolve(Path::new("/tmp")).unwrap();
        assert_eq!(e.base_url, "http://127.0.0.1:3210");
        assert_eq!(e.token, "");
    }

    #[test]
    fn a_configured_base_and_token_are_used() {
        // Safety: env is process-global and other tests read it, so this exercises the file path
        // only. Set OSSIA_API_BASE explicitly to see the env branch win.
        let cfg = parse_config(r#"{"apiBase":"https://api.ossia.dev/","apiToken":"s3cret"}"#).unwrap();
        let e = cfg.resolve(Path::new("/tmp")).unwrap();
        assert_eq!(e.base_url, "https://api.ossia.dev");
        assert_eq!(e.token, "s3cret");
    }

    #[test]
    fn a_non_http_base_is_rejected() {
        let cfg = parse_config(r#"{"apiBase":"file:///etc/passwd"}"#).unwrap();
        let err = cfg.resolve(Path::new("/tmp")).unwrap_err();
        assert!(err.contains("must be an http(s) URL"), "{err}");
        assert!(!err.contains("passwd"), "the rejected value must not be echoed verbatim: {err}");
        assert!(!err.contains("file:"), "even the scheme label must not leak the path: {err}");
    }

    #[test]
    fn a_malformed_config_reports_the_reason() {
        let err = parse_config("{ apiBase: }").unwrap_err();
        assert!(err.contains("not valid JSON"), "{err}");
    }

    #[test]
    fn an_empty_config_file_is_not_an_error() {
        let dir = std::env::temp_dir().join(format!("ossia-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(config_path(&dir), "\n").unwrap();
        let cfg = match std::fs::read_to_string(config_path(&dir)) {
            Ok(text) if text.trim().is_empty() => Config::default(),
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(cfg.resolve(&dir).unwrap().base_url, "http://127.0.0.1:3210");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn endpoint_serializes_as_the_ui_expects() {
        let e = Endpoint { base_url: "http://127.0.0.1:3210".into(), token: String::new() };
        assert_eq!(
            serde_json::to_string(&e).unwrap(),
            r#"{"baseUrl":"http://127.0.0.1:3210","token":""}"#
        );
    }
}
