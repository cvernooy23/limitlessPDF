use tauri::{AppHandle, command, Runtime};

use crate::models::*;
use crate::Result;
use crate::PivsignExt;

#[command]
pub(crate) async fn ping<R: Runtime>(
    app: AppHandle<R>,
    payload: PingRequest,
) -> Result<PingResponse> {
    app.pivsign().ping(payload)
}
