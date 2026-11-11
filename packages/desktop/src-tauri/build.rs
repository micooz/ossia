fn main() {
    #[cfg(feature = "gui")]
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "management_status",
            "management_start",
            "management_stop",
            "management_doctor",
            "management_agents_state",
            "management_agents_change",
        ]),
    ))
    .expect("build Tauri management permissions");
}
