use serde::Serialize;

#[cfg(desktop)]
use tauri::{AppHandle, Emitter, Runtime};
#[cfg(desktop)]
use tauri_plugin_updater::UpdaterExt;

#[derive(Clone, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub body: Option<String>,
    pub date: Option<String>,
}

#[cfg(desktop)]
fn endpoint(channel: &str) -> String {
    format!(
        "https://cvernooy23.github.io/limitlessPDF/update/{}.json",
        channel
    )
}

// ── Desktop: real in-app updater (tauri-plugin-updater) ──────────────────────

/// Check for an update on the given channel (stable, beta, alpha).
/// Returns Some(info) when a newer version is available, None otherwise.
#[cfg(desktop)]
#[tauri::command]
pub async fn check_for_update<R: Runtime>(
    app: AppHandle<R>,
    channel: String,
) -> Result<Option<UpdateInfo>, String> {
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint(&channel)
            .parse()
            .map_err(|e: url::ParseError| e.to_string())?])
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?;

    match updater.check().await {
        Ok(Some(update)) => Ok(Some(UpdateInfo {
            version: update.version.clone(),
            body: update.body.clone(),
            date: update.date.map(|d| d.to_string()),
        })),
        Ok(None) => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Download and install the latest update for the given channel.
/// Emits `update-progress` events with `{ chunk, total }` payloads while
/// downloading, so the frontend can render a progress bar.
#[cfg(desktop)]
#[tauri::command]
pub async fn download_and_install_update<R: Runtime>(
    app: AppHandle<R>,
    channel: String,
) -> Result<(), String> {
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint(&channel)
            .parse()
            .map_err(|e: url::ParseError| e.to_string())?])
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?;

    let update = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No update available".to_string())?;

    let handle = app.clone();
    update
        .download_and_install(
            move |chunk, total| {
                let _ = handle.emit(
                    "update-progress",
                    serde_json::json!({ "chunk": chunk, "total": total }),
                );
            },
            || {},
        )
        .await
        .map_err(|e| e.to_string())
}

// ── Mobile: updates come from the app store, so these are inert stubs ────────

#[cfg(not(desktop))]
#[tauri::command]
pub async fn check_for_update(_channel: String) -> Result<Option<UpdateInfo>, String> {
    Ok(None)
}

#[cfg(not(desktop))]
#[tauri::command]
pub async fn download_and_install_update(_channel: String) -> Result<(), String> {
    Err(
        "In-app updates aren't available on this platform; install updates from the app store."
            .into(),
    )
}
