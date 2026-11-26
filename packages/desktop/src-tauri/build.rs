use std::process::Command;

/// Provenance used by the updater's "only release builds replace themselves" rule.
fn build_stamp() -> String {
    if let Ok(stamp) = std::env::var("OSSIA_BUILD_STAMP") {
        let stamp = stamp.trim();
        if !stamp.is_empty() {
            return stamp.to_string();
        }
    }
    if let Some(tag) = git_output(["describe", "--tags", "--exact-match"]) {
        return tag;
    }
    if let Some(stamp) = git_output(["describe", "--tags", "--always", "--dirty"])
        .filter(|stamp| stamp.contains("-g") || stamp.ends_with("-dirty"))
    {
        return stamp;
    }
    format!(
        "{}-untagged",
        std::env::var("CARGO_PKG_VERSION").unwrap_or_default()
    )
}

fn git_output<const N: usize>(args: [&str; N]) -> Option<String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn main() {
    println!("cargo:rerun-if-env-changed=OSSIA_BUILD_STAMP");
    println!("cargo:rerun-if-changed=icons/icon.icns");
    println!("cargo:rerun-if-changed=icons/32x32.png");
    let git = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../.git");
    for file in ["HEAD", "refs/heads", "refs/tags", "packed-refs"] {
        println!("cargo:rerun-if-changed={}", git.join(file).display());
    }
    println!("cargo:rustc-env=OSSIA_BUILD_STAMP={}", build_stamp());
    #[cfg(feature = "gui")]
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "management_status",
            "management_open_data_directory",
            "management_start",
            "management_stop",
            "management_doctor",
            "management_agents_state",
            "management_agents_change",
            "management_preferences",
            "management_preferences_change",
            "management_omr_settings_set",
            "management_autostart_set",
            "management_storage_status",
            "management_storage_cleanup",
            "management_scores",
            "management_archived_scores",
            "management_score_preview",
            "management_archive_score",
            "management_restore_score",
            "management_delete_score",
            "management_open_score",
            "update_state",
            "update_check",
            "update_cancel",
            "release_notes",
            "update_apply",
            "update_token_set",
            "open_external_url",
        ]),
    ))
    .expect("build Tauri management permissions");
}
