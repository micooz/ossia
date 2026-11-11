fn main() {
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
            "management_autostart_set",
            "management_storage_status",
            "management_storage_cleanup",
        ]),
    ))
    .expect("build Tauri management permissions");
}
