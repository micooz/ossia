fn main() {
    #[cfg(feature = "gui")]
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "management_status",
            "management_start",
            "management_stop",
            "management_doctor",
            "management_integration_detect",
            "management_integration_plan",
            "management_integration_status",
            "management_integration_apply",
            "management_integration_repair",
            "management_integration_remove",
        ]),
    ))
    .expect("build Tauri management permissions");
}
