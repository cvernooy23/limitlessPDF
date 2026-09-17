//! PDF engine layer — binds PDFium at runtime for rendering, content editing, and export.

use pdfium_render::prelude::*;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Serialize)]
pub struct EngineStatus {
    /// True when the PDFium dynamic library was located and bound.
    pub available: bool,
    /// Reserved for the PDFium version string.
    pub version: Option<String>,
    /// Human-readable detail for the UI status badge.
    pub message: String,
}

/// Try to bind PDFium by searching the given directories (in order) for the
/// platform library, then falling back to a system-installed copy.
///
/// `dirs` typically contains the executable's own folder and the bundled
/// resource directory, so an installed app finds the `pdfium.dll` /
/// `libpdfium.*` shipped alongside it.
pub fn status_in(dirs: &[PathBuf]) -> EngineStatus {
    for dir in dirs {
        let lib = Pdfium::pdfium_platform_library_name_at_path(dir);
        if Pdfium::bind_to_library(&lib).is_ok() {
            return ok(format!("PDFium engine bound from {}.", dir.display()));
        }
    }

    if Pdfium::bind_to_system_library().is_ok() {
        return ok("PDFium engine bound from the system library path.".to_string());
    }

    EngineStatus {
        available: false,
        version: None,
        message: "PDFium library not found. Run `npm run get-pdfium` (dev) or rebuild \
                  so the binary is bundled next to the app. The UI shell runs without it."
            .to_string(),
    }
}

/// Bind PDFium and return a usable instance for content editing. Tries the
/// executable's folder and the working directory, then the system library.
pub fn instance() -> Result<Pdfium, String> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(p) = exe.parent() {
            dirs.push(p.to_path_buf());
            dirs.push(p.join("resources")); // bundled resources keep this prefix
        }
    }
    dirs.push(PathBuf::from("."));
    dirs.push(PathBuf::from("./resources"));

    for dir in &dirs {
        let lib = Pdfium::pdfium_platform_library_name_at_path(dir);
        if let Ok(bindings) = Pdfium::bind_to_library(&lib) {
            return Ok(Pdfium::new(bindings));
        }
    }
    match Pdfium::bind_to_system_library() {
        Ok(bindings) => Ok(Pdfium::new(bindings)),
        Err(e) => Err(format!(
            "PDFium is required for content editing but wasn't found ({e:?}). \
             Run `npm run get-pdfium`."
        )),
    }
}

fn ok(message: String) -> EngineStatus {
    EngineStatus {
        available: true,
        version: None,
        message,
    }
}
