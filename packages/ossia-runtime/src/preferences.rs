use crate::{storage, Result, UiPreferences};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

const FILE: &str = "preferences.json";

fn path(home: &Path) -> PathBuf {
    home.join(FILE)
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
mod tests {
    use super::*;
    use crate::{TrayClickAction, UiLanguage, UiTheme};

    #[test]
    fn missing_file_uses_defaults() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(load(root.path()).unwrap(), UiPreferences::default());
    }

    #[test]
    fn round_trips_all_preferences() {
        let root = tempfile::tempdir().unwrap();
        let expected = UiPreferences {
            theme: UiTheme::Dark,
            language: Some(UiLanguage::ZhCn),
            tray_click_action: TrayClickAction::Menu,
        };
        save(root.path(), &expected).unwrap();
        assert_eq!(load(root.path()).unwrap(), expected);
    }

    #[test]
    fn rejects_invalid_or_unknown_preferences() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(path(root.path()), br#"{"theme":"midnight"}"#).unwrap();
        assert!(load(root.path()).is_err());
        std::fs::write(
            path(root.path()),
            br#"{"theme":"dark","language":null,"trayClickAction":"window","extra":true}"#,
        )
        .unwrap();
        assert!(load(root.path()).is_err());
    }
}
