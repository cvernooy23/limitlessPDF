use tauri::{
  plugin::{Builder, TauriPlugin},
  Manager, Runtime,
};

pub use models::*;

#[cfg(desktop)]
mod desktop;
#[cfg(mobile)]
mod mobile;

mod commands;
mod error;
mod models;

pub use error::{Error, Result};

#[cfg(desktop)]
use desktop::Pivsign;
#[cfg(mobile)]
use mobile::Pivsign;

/// Extensions to access the pivsign APIs from the app.
pub trait PivsignExt<R: Runtime> {
  fn pivsign(&self) -> &Pivsign<R>;
}

impl<R: Runtime, T: Manager<R>> crate::PivsignExt<R> for T {
  fn pivsign(&self) -> &Pivsign<R> {
    self.state::<Pivsign<R>>().inner()
  }
}

/// Initializes the plugin.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
  Builder::new("pivsign")
    .invoke_handler(tauri::generate_handler![
      commands::list_identities,
      commands::sign_data
    ])
    .setup(|app, api| {
      #[cfg(mobile)]
      let pivsign = mobile::init(app, api)?;
      #[cfg(desktop)]
      let pivsign = desktop::init(app, api)?;
      app.manage(pivsign);
      Ok(())
    })
    .build()
}
