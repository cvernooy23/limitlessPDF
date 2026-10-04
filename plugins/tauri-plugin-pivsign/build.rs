const COMMANDS: &[&str] = &["list_identities", "sign_data"];

fn main() {
  tauri_plugin::Builder::new(COMMANDS)
    .android_path("android")
    .build();
}
