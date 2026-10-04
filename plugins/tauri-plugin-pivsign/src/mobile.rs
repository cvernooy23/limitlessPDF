use serde::de::DeserializeOwned;
use tauri::{
  plugin::{PluginApi, PluginHandle},
  AppHandle, Runtime,
};

use crate::models::*;

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_pivsign);

pub fn init<R: Runtime, C: DeserializeOwned>(
  _app: &AppHandle<R>,
  api: PluginApi<R, C>,
) -> crate::Result<Pivsign<R>> {
  #[cfg(target_os = "android")]
  let handle = api.register_android_plugin("com.plugin.pivsign", "PivsignPlugin")?;
  #[cfg(target_os = "ios")]
  let handle = api.register_ios_plugin(init_plugin_pivsign)?;
  Ok(Pivsign(handle))
}

/// Access to the pivsign APIs.
pub struct Pivsign<R: Runtime>(PluginHandle<R>);

impl<R: Runtime> Pivsign<R> {
  pub fn list_identities(&self, args: ListIdentitiesArgs) -> crate::Result<ListIdentitiesResponse> {
    self
      .0
      .run_mobile_plugin("listIdentities", args)
      .map_err(Into::into)
  }

  pub fn sign_data(&self, args: SignDataArgs) -> crate::Result<SignDataResponse> {
    self.0.run_mobile_plugin("signData", args).map_err(Into::into)
  }
}
