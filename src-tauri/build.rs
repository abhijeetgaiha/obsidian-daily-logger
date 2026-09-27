fn main() {
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "load_draft",
            "save_draft",
            "load_settings",
            "set_fallback",
            "set_entry_format",
            "read_settings_form",
            "list_headings",
            "pick_vault_folder",
            "save_settings",
            "submit_entry",
            "request_exit",
        ]),
    ))
    .expect("failed to build application permissions");
}
