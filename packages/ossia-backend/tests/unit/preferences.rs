use super::*;
use crate::{
    OmrService, StartupAction, StorageCacheLimit, TrayClickAction, UiLanguage, UiTheme,
    UpdateChannelPreference, UpdateSource,
};

#[test]
fn missing_file_uses_defaults() {
    let root = tempfile::tempdir().unwrap();
    let preferences = load(root.path()).unwrap();
    assert_eq!(preferences, UiPreferences::default());
    assert_eq!(preferences.omr_service, OmrService::Custom);
    assert!(!preferences.omr_enabled());
}

#[test]
fn legacy_preferences_use_the_default_storage_limit() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        path(root.path()),
        br#"{"theme":"dark","language":null,"trayClickAction":"menu"}"#,
    )
    .unwrap();
    assert_eq!(
        load(root.path()).unwrap(),
        UiPreferences {
            theme: UiTheme::Dark,
            language: None,
            tray_click_action: TrayClickAction::Menu,
            startup_action: StartupAction::Window,
            storage_cache_limit: StorageCacheLimit::Gib1,
            storage_auto_cleanup: true,
            lan_access: false,
            auto_download: true,
            update_source: UpdateSource::Ossia,
            update_channel: UpdateChannelPreference::Auto,
            github_token: None,
            omr_service: OmrService::Ossia,
            omr_enabled: None,
            omr_url: None,
            omr_token: None,
            omr_custom_token: None,
        }
    );
}

#[test]
fn removed_two_gib_limit_migrates_to_three_gib() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        path(root.path()),
        br#"{"theme":"dark","language":null,"trayClickAction":"menu","storageCacheLimit":"gib2"}"#,
    )
    .unwrap();
    assert_eq!(
        load(root.path()).unwrap().storage_cache_limit,
        StorageCacheLimit::Gib3
    );
}

#[test]
fn round_trips_all_preferences() {
    let root = tempfile::tempdir().unwrap();
    let expected = UiPreferences {
        theme: UiTheme::Dark,
        language: Some(UiLanguage::ZhCn),
        tray_click_action: TrayClickAction::Menu,
        startup_action: StartupAction::Tray,
        storage_cache_limit: StorageCacheLimit::Gib10,
        storage_auto_cleanup: false,
        lan_access: true,
        auto_download: false,
        update_source: UpdateSource::Github,
        update_channel: UpdateChannelPreference::Beta,
        github_token: Some("github_pat_secret".into()),
        omr_service: OmrService::Custom,
        omr_enabled: None,
        omr_url: Some("https://omr.example.com".into()),
        omr_token: Some("secret".into()),
        omr_custom_token: Some("custom-secret".into()),
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

#[test]
fn omr_url_accepts_a_path_prefix_with_loopback_http_exception() {
    assert_eq!(
        validate_omr_url(" https://omr.example.com ").unwrap(),
        "https://omr.example.com"
    );
    assert!(validate_omr_url("http://127.0.0.1:3212").is_ok());
    assert!(validate_omr_url("http://localhost:3212").is_ok());
    assert!(validate_omr_url("http://omr.example.com").is_err());
    assert!(validate_omr_url("https://omr.example.com/").is_err());
    assert_eq!(
        validate_omr_url("https://omr.example.com/api").unwrap(),
        "https://omr.example.com/api"
    );
    assert!(validate_omr_url("https://omr.example.com/api/").is_err());
    assert!(validate_omr_url("https://omr.example.com//api").is_err());
    assert!(validate_omr_url("https://omr.example.com?x=1").is_err());
    assert!(validate_omr_url("http://127.0.0.1:3212/omr").is_ok());
}

#[test]
fn omr_token_rejects_whitespace_and_empty_values() {
    assert_eq!(normalize_omr_token(" secret ").unwrap(), "secret");
    assert!(normalize_omr_token("two words").is_err());
    assert!(normalize_omr_token("").is_err());
}

#[test]
fn github_token_is_normalized_and_masked() {
    assert_eq!(
        normalize_github_token(" github_pat_12345678 ").unwrap(),
        "github_pat_12345678"
    );
    assert!(normalize_github_token("bad token").is_err());
    assert_eq!(
        masked_token(Some("github_pat_12345678")).as_deref(),
        Some("••••••••5678")
    );
}

#[test]
fn concurrent_updates_preserve_both_fields() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().to_owned();
    let first = home.clone();
    let second = home.clone();
    let language = std::thread::spawn(move || {
        let _guard = lock(&first).unwrap();
        let mut current = load(&first).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(40));
        current.language = Some(UiLanguage::ZhCn);
        save(&first, &current).unwrap();
    });
    let theme = std::thread::spawn(move || {
        let _guard = lock(&second).unwrap();
        let mut current = load(&second).unwrap();
        current.theme = UiTheme::Dark;
        save(&second, &current).unwrap();
    });
    language.join().unwrap();
    theme.join().unwrap();
    let saved = load(&home).unwrap();
    assert_eq!(saved.language, Some(UiLanguage::ZhCn));
    assert_eq!(saved.theme, UiTheme::Dark);
}
