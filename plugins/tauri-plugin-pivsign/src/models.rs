use serde::{Deserialize, Serialize};

/// Which credential source to use: "keychain" (Android system KeyChain,
/// covering imported soft certs and MDM/Purebred derived PIV creds), and later
/// "nfc" / "usb" for hardware PIV tokens.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListIdentitiesArgs {
  pub source: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
  /// Opaque handle used to sign later (KeyChain alias, or token slot id).
  pub id: String,
  pub subject: String,
  pub issuer: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListIdentitiesResponse {
  pub identities: Vec<Identity>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignDataArgs {
  pub source: String,
  /// Identity handle from `list_identities`.
  pub id: String,
  /// Path to a file holding the exact bytes to sign (the PDF ByteRange content).
  pub content_path: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SignDataResponse {
  /// Base64 (standard) of the detached PKCS#7 / CMS SignedData.
  pub pkcs7_b64: String,
}
