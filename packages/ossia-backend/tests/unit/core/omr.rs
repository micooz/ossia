use super::{config::OmrConfig, retain_musicxml};
use crate::{OmrService, UiPreferences, OSSIA_OMR_URL};

#[test]
fn retains_omr_musicxml_by_input_hash() {
    let home = tempfile::tempdir().unwrap();
    let path = retain_musicxml(home.path(), b"pdf", b"<score-partwise/>").unwrap();
    assert!(path.starts_with(home.path().join("imports/omr")));
    assert_eq!(std::fs::read(path).unwrap(), b"<score-partwise/>");
}

#[test]
fn ossia_service_uses_the_fixed_endpoint_and_ossia_key() {
    let preferences = UiPreferences {
        omr_service: OmrService::Ossia,
        omr_token: Some("ossia-key".into()),
        omr_enabled: Some(true),
        ..UiPreferences::default()
    };
    let config = OmrConfig::from_preferences(&preferences).unwrap();
    assert_eq!(config.url, OSSIA_OMR_URL);
    assert_eq!(config.token, "ossia-key");
}

#[test]
fn custom_service_uses_its_own_url_and_key() {
    let preferences = UiPreferences {
        omr_service: OmrService::Custom,
        omr_url: Some("https://omr.example.com/omr".into()),
        omr_custom_token: Some("custom-key".into()),
        omr_enabled: Some(true),
        ..UiPreferences::default()
    };
    let config = OmrConfig::from_preferences(&preferences).unwrap();
    assert_eq!(config.url, "https://omr.example.com/omr");
    assert_eq!(config.token, "custom-key");
}

#[test]
fn custom_service_requires_a_url_and_key() {
    let preferences = UiPreferences {
        omr_service: OmrService::Custom,
        ..UiPreferences::default()
    };
    assert!(OmrConfig::from_preferences(&preferences).is_err());
}

#[test]
fn disabled_and_incomplete_omr_have_distinct_errors() {
    use ossia_types::ErrorCode;
    let preferences = UiPreferences::default();
    assert_eq!(
        OmrConfig::from_preferences(&preferences)
            .err()
            .unwrap()
            .code,
        ErrorCode::OmrNotConfigured
    );
    let preferences = UiPreferences {
        omr_enabled: Some(true),
        omr_service: OmrService::Ossia,
        ..preferences
    };
    let error = OmrConfig::from_preferences(&preferences).err().unwrap();
    assert_eq!(error.code, ErrorCode::OmrConfigurationInvalid);
    assert_eq!(error.message, "Remote OMR token is not configured");
    let preferences = UiPreferences {
        omr_service: OmrService::Custom,
        ..preferences
    };
    let error = OmrConfig::from_preferences(&preferences).err().unwrap();
    assert_eq!(error.code, ErrorCode::OmrConfigurationInvalid);
    assert_eq!(error.message, "Custom remote OMR URL is not configured");
}

#[test]
fn legacy_omr_preferences_preserve_existing_service_choices() {
    let default = UiPreferences {
        omr_enabled: None,
        omr_service: OmrService::Ossia,
        ..UiPreferences::default()
    };
    assert!(!default.omr_enabled());
    let built_in = UiPreferences {
        omr_token: Some("key".into()),
        ..default.clone()
    };
    assert!(built_in.omr_enabled());
    let custom = UiPreferences {
        omr_service: OmrService::Custom,
        ..default
    };
    assert!(custom.omr_enabled());
    let disabled = UiPreferences {
        omr_enabled: Some(false),
        ..built_in
    };
    assert!(!disabled.omr_enabled());
    assert_eq!(
        OmrConfig::from_preferences(&disabled).err().unwrap().code,
        ossia_types::ErrorCode::OmrNotConfigured
    );
}

#[test]
fn transcribe_reads_persisted_enabled_state_before_network_access() {
    use ossia_engine::RunContext;
    use ossia_types::{ErrorCode, OmrSourceFormat};
    let home = tempfile::tempdir().unwrap();
    let mut preferences = UiPreferences {
        omr_enabled: Some(false),
        omr_service: OmrService::Ossia,
        omr_token: Some("saved-key".into()),
        ..UiPreferences::default()
    };
    crate::preferences::save(home.path(), &preferences).unwrap();
    let error = super::transcribe(
        home.path(),
        OmrSourceFormat::Pdf,
        b"%PDF",
        &RunContext::default(),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::OmrNotConfigured);
    preferences.omr_enabled = Some(true);
    preferences.omr_token = None;
    crate::preferences::save(home.path(), &preferences).unwrap();
    let error = super::transcribe(
        home.path(),
        OmrSourceFormat::Pdf,
        b"%PDF",
        &RunContext::default(),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::OmrConfigurationInvalid);
    assert_eq!(
        serde_json::to_value(error).unwrap()["code"],
        "omr-configuration-invalid"
    );
}
