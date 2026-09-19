//! The IPC surface exposed to the frontend. Each `#[tauri::command]` here maps
//! to a typed wrapper in `src/lib/api.ts`.

use crate::engine;
use serde::Serialize;
use std::path::PathBuf;
use tauri::Manager;

#[derive(Serialize)]
pub struct AppInfo {
    name: String,
    version: String,
}

/// Basic app metadata.
#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        name: "limitlessPDF".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// Read a PDF (or any file) from disk and return its raw bytes to the webview.
/// Returns a binary IPC response, which arrives in JS as an `ArrayBuffer` —
/// far more efficient than serializing a byte array to JSON.
#[tauri::command]
pub fn read_pdf(path: String) -> Result<tauri::ipc::Response, String> {
    std::fs::read(&path)
        .map(tauri::ipc::Response::new)
        .map_err(|e| format!("Could not read \"{path}\": {e}"))
}

/// Copy the opened PDF to a temp file and return its path. We save annotations
/// from this pristine snapshot, so overwriting the original in place never
/// double-applies annotations on a second save.
#[tauri::command]
pub fn snapshot_pdf(src_path: String) -> Result<String, String> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut tmp = std::env::temp_dir();
    tmp.push(format!("limitlesspdf-src-{nanos}.pdf"));
    std::fs::copy(&src_path, &tmp).map_err(|e| format!("Snapshot failed: {e}"))?;
    Ok(tmp.to_string_lossy().into_owned())
}

/// Write the given annotations into `src_path` and save the result to
/// `dest_path` (may be the same file to overwrite).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn save_pdf(
    sources: Vec<String>,
    dest_path: String,
    plan: Vec<crate::annotate::PlanEntry>,
    annotations: Vec<crate::annotate::SaveAnnotation>,
    content_edits: Vec<crate::content::ContentEdit>,
    form_values: Vec<crate::form::FormValue>,
    form_mode: String,
    password: Option<String>,
    source_password: Option<String>,
) -> Result<(), String> {
    crate::annotate::save(
        sources,
        &dest_path,
        plan,
        annotations,
        content_edits,
        form_values,
        form_mode,
        password,
        source_password,
    )
}

/// Write raw bytes to `path`. Used by the export feature, which builds the
/// output file (txt/md/html/docx/xlsx) on the frontend and hands us the bytes.
#[tauri::command]
pub fn export_file(path: String, bytes: Vec<u8>) -> Result<(), String> {
    std::fs::write(&path, &bytes).map_err(|e| format!("Couldn't write \"{path}\": {e}"))
}

/// Reports whether the PDFium engine is available at runtime. Searches, in
/// order: the executable's own folder, the bundled resource directory, and the
/// current working directory (covers `tauri dev`), before the system path.
#[tauri::command]
pub fn engine_status(app: tauri::AppHandle) -> engine::EngineStatus {
    let mut dirs: Vec<PathBuf> = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push(parent.to_path_buf());
            // Bundled resources keep their `resources/` prefix on disk.
            dirs.push(parent.join("resources"));
        }
    }
    if let Ok(resource_dir) = app.path().resource_dir() {
        dirs.push(resource_dir.join("resources"));
        dirs.push(resource_dir);
    }
    dirs.push(PathBuf::from("."));
    dirs.push(PathBuf::from("./resources"));

    engine::status_in(&dirs)
}

// ── OCR commands ──────────────────────────────────────────────────────────

/// Check whether Tesseract is installed and what languages are available.
#[tauri::command]
pub fn check_ocr_available() -> crate::ocr::OcrStatus {
    crate::ocr::check_available()
}

/// Detect which pages in a PDF are likely scanned images (very little
/// extractable text). Returns a list of 1-based page numbers.
#[tauri::command]
pub fn detect_scanned_pages(
    path: String,
    source_password: Option<String>,
) -> Result<Vec<u32>, String> {
    crate::ocr::detect_scanned_pages(&path, source_password.as_deref())
}

/// Run the full OCR pipeline on the given pages: render via PDFium, run
/// Tesseract, inject invisible text layer, and save the result in-place.
#[tauri::command]
pub fn run_ocr(
    src_path: String,
    pages: Vec<u32>,
    language: Option<String>,
    dpi: Option<u32>,
    source_password: Option<String>,
) -> Result<Vec<crate::ocr::OcrPageResult>, String> {
    let req = crate::ocr::OcrRequest {
        src_path,
        pages,
        language,
        dpi,
        source_password,
    };
    crate::ocr::run_ocr(&req)
}

// ── Signature commands ───────────────────────────────────────────────────

/// Extract digital signature information from a PDF. Returns details about
/// each signature field: signer name, date, reason, and byte-coverage status.
#[tauri::command]
pub fn check_signatures(
    path: String,
    source_password: Option<String>,
) -> Result<Vec<crate::signature::SignatureInfo>, String> {
    crate::signature::extract_signatures(&path, source_password.as_deref())
}
