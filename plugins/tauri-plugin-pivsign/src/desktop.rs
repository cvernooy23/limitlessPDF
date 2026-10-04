use serde::de::DeserializeOwned;
use tauri::{plugin::PluginApi, AppHandle, Runtime};

use crate::models::*;

pub fn init<R: Runtime, C: DeserializeOwned>(
  app: &AppHandle<R>,
  _api: PluginApi<R, C>,
) -> crate::Result<Pivsign<R>> {
  Ok(Pivsign(app.clone()))
}

/// Access to the pivsign APIs (desktop stub: signing uses the OS cert store
/// directly in the app, so these are never called on desktop).
pub struct Pivsign<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Pivsign<R> {
  pub fn list_identities(&self, _args: ListIdentitiesArgs) -> crate::Result<ListIdentitiesResponse> {
    Err(crate::Error::Unsupported)
  }

  pub fn sign_data(&self, _args: SignDataArgs) -> crate::Result<SignDataResponse> {
    Err(crate::Error::Unsupported)
  }
}
