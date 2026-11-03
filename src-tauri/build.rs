fn main() {
    // Declaring the app's commands makes each one require an explicit
    // permission in capabilities/, instead of being callable by default.
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&["get_settings", "save_settings", "app_ready"]),
    ))
    .expect("failed to run tauri-build");
}
