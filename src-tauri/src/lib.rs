mod annotate;
mod commands;
mod content;
mod engine;
mod form;
mod ocr;
mod signature;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if let Some(window) = app.get_webview_window("main") {
                apply_window_effects(&window);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::engine_status,
            commands::read_pdf,
            commands::snapshot_pdf,
            commands::save_pdf,
            commands::export_file,
            commands::check_ocr_available,
            commands::detect_scanned_pages,
            commands::run_ocr,
            commands::check_signatures,
            commands::split_pdf,
        ])
        .run(tauri::generate_context!())
        .expect("error while running limitlessPDF");
}

/// Apply native blur for the glassy look. Best-effort: if the platform/compositor
/// doesn't support it, we silently fall back to the in-app gradient.
#[allow(unused_variables)]
fn apply_window_effects(window: &tauri::WebviewWindow) {
    #[cfg(target_os = "windows")]
    {
        use window_vibrancy::{apply_acrylic, apply_mica};
        // Prefer Mica (Win11); fall back to Acrylic (Win10).
        if apply_mica(window, Some(true)).is_err() {
            let _ = apply_acrylic(window, Some((18, 18, 26, 125)));
        }
    }

    #[cfg(target_os = "macos")]
    {
        use window_vibrancy::{apply_vibrancy, NSVisualEffectMaterial, NSVisualEffectState};
        let _ = apply_vibrancy(
            window,
            NSVisualEffectMaterial::HudWindow,
            Some(NSVisualEffectState::Active),
            Some(14.0),
        );
    }
}
