mod annotate;
mod commands;
mod content;
mod digsig;
mod engine;
mod form;
mod insert;
mod ocr;
mod signature;
mod updater;

#[cfg(target_os = "linux")]
mod linux_gl;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
#[cfg(mobile)]
static PIV_APP: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

/// The app handle captured at setup, for the mobile certificate-signing path.
#[cfg(mobile)]
pub(crate) fn piv_app() -> Option<tauri::AppHandle> {
    PIV_APP.get().cloned()
}

pub fn run() {
    // On Linux, fall back to software rendering when EGL is unusable so we
    // degrade gracefully instead of aborting on GPU-less machines.
    #[cfg(target_os = "linux")]
    linux_gl::ensure_webkit_compat();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init());

    // The in-app updater is desktop-only; on mobile, updates come from the store.
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    // Smart-card / certificate signing plugin is mobile-only.
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_pivsign::init());

    builder
        .setup(|app| {
            // Capture the resource dir so the PDFium loader can find the
            // bundled library on Linux/macOS, where it is not next to the exe.
            if let Ok(resource_dir) = app.path().resource_dir() {
                engine::set_resource_dir(resource_dir);
            }
            // Stash the app handle so the mobile signing path can reach the
            // pivsign plugin from the (handle-less) digsig helpers.
            #[cfg(mobile)]
            {
                let _ = PIV_APP.set(app.handle().clone());
            }
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
            commands::zip_files,
            commands::create_blank_pdf,
            commands::create_image_pdf,
            commands::list_certificates,
            commands::sign_pdf,
            commands::create_stamp_pdf,
            updater::check_for_update,
            updater::download_and_install_update,
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
