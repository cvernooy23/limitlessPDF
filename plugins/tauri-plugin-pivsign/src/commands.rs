use tauri::{command, AppHandle, Runtime};

use crate::models::*;
use crate::PivsignExt;
use crate::Result;

#[command]
pub(crate) async fn list_identities<R: Runtime>(
  app: AppHandle<R>,
  args: ListIdentitiesArgs,
) -> Result<ListIdentitiesResponse> {
  app.pivsign().list_identities(args)
}

#[command]
pub(crate) async fn sign_data<R: Runtime>(
  app: AppHandle<R>,
  args: SignDataArgs,
) -> Result<SignDataResponse> {
  app.pivsign().sign_data(args)
}
