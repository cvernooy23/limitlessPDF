//! Digital signature creation: certificate-based PKI signing and stamp PDFs
//! for typed/cursive signatures.
//!
//! Certificate operations use platform-native tools:
//! - **Windows**: CryptoAPI via FFI (no extra crate needed).
//! - **macOS**: `security` CLI (Keychain / Security.framework).
//! - **Linux**: `certutil` (NSS) / `pkcs15-tool` for listing,
//!   `openssl cms` for PKCS#7 signing.

use lopdf::{Dictionary, Document, Object, Stream};
use serde::Serialize;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

// ── Public types ─────────────────────────────────────────────────────────

/// Information about a certificate in the system store, returned to the frontend
/// for the cert-picker list.
#[derive(Debug, Clone, Serialize)]
pub struct CertInfo {
    /// SHA-1 thumbprint as a lowercase hex string.
    pub thumbprint: String,
    /// Subject common name (CN), e.g. "Jane Doe".
    pub subject: String,
    /// Issuer common name.
    pub issuer: String,
    /// `true` when the cert has an associated private key (only those can sign).
    pub has_private_key: bool,
}

/// Rectangle describing where the visible signature widget goes, in PDF points
/// (origin = bottom-left of the page).
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SignRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

// ── Constants ────────────────────────────────────────────────────────────

/// Maximum PKCS#7 blob we reserve space for (16 KiB).
const SIG_CONTENT_BYTES: usize = 16_384;
/// Hex-encoded length (two hex chars per byte).
const SIG_HEX_LEN: usize = SIG_CONTENT_BYTES * 2;
/// Sentinel value used inside the ByteRange placeholder so we can find it
/// after serialization.  Ten digits so it's unmistakable.
const BR_SENTINEL: &str = "1234567890";

// ── Helpers ──────────────────────────────────────────────────────────────

/// Unique temp file path, same pattern as `insert.rs`.
fn temp_pdf_path(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut p = std::env::temp_dir();
    p.push(format!("limitlesspdf-{prefix}-{nanos}.pdf"));
    p
}

// ── create_stamp_pdf ─────────────────────────────────────────────────────

/// Build a one-page image PDF from raw PNG bytes (the canvas-rendered cursive
/// name).  Returns the path to the temp file.  This mirrors
/// `insert::create_image_pdf` but takes in-memory bytes instead of a file path.
pub fn create_stamp_pdf(png_bytes: &[u8]) -> Result<String, String> {
    if png_bytes.is_empty() {
        return Err("PNG data is empty".to_string());
    }

    let img = image::load_from_memory(png_bytes)
        .map_err(|e| format!("Could not decode stamp image: {e}"))?;

    let (img_w, img_h) = (img.width(), img.height());
    if img_w == 0 || img_h == 0 {
        return Err("Stamp image has zero dimensions".to_string());
    }

    let rgb = img.to_rgb8();
    let raw_pixels: Vec<u8> = rgb.into_raw();

    let mut doc = Document::with_version("1.7");

    let max_dim: f64 = 14400.0;
    let scale = (max_dim / img_w as f64)
        .min(max_dim / img_h as f64)
        .min(1.0);
    let page_w = (img_w as f64 * scale).max(1.0);
    let page_h = (img_h as f64 * scale).max(1.0);

    // Image XObject
    let mut img_dict = Dictionary::new();
    img_dict.set("Type", Object::Name(b"XObject".to_vec()));
    img_dict.set("Subtype", Object::Name(b"Image".to_vec()));
    img_dict.set("Width", Object::Integer(img_w as i64));
    img_dict.set("Height", Object::Integer(img_h as i64));
    img_dict.set("ColorSpace", Object::Name(b"DeviceRGB".to_vec()));
    img_dict.set("BitsPerComponent", Object::Integer(8));

    let img_stream = Stream::new(img_dict, raw_pixels);
    let img_id = doc.add_object(img_stream);

    // Content stream
    let content = format!("q {page_w:.4} 0 0 {page_h:.4} 0 0 cm /Img Do Q");
    let content_stream = Stream::new(Dictionary::new(), content.into_bytes());
    let content_id = doc.add_object(content_stream);

    // Resources
    let mut xobjects = Dictionary::new();
    xobjects.set("Img", Object::Reference(img_id));
    let mut resources = Dictionary::new();
    resources.set("XObject", Object::Dictionary(xobjects));

    let pages_id = doc.new_object_id();
    let page_id = doc.new_object_id();

    let mut page = Dictionary::new();
    page.set("Type", Object::Name(b"Page".to_vec()));
    page.set("Parent", Object::Reference(pages_id));
    page.set(
        "MediaBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Real(page_w as f32),
            Object::Real(page_h as f32),
        ]),
    );
    page.set("Contents", Object::Reference(content_id));
    page.set("Resources", Object::Dictionary(resources));
    doc.objects.insert(page_id, Object::Dictionary(page));

    let mut pages = Dictionary::new();
    pages.set("Type", Object::Name(b"Pages".to_vec()));
    pages.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
    pages.set("Count", Object::Integer(1));
    doc.objects.insert(pages_id, Object::Dictionary(pages));

    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_id));
    let catalog_id = doc.add_object(catalog);
    doc.trailer.set("Root", catalog_id);

    doc.compress();

    let path = temp_pdf_path("stamp");
    doc.save(&path)
        .map_err(|e| format!("Failed to create stamp PDF: {e}"))?;
    Ok(path.to_string_lossy().into_owned())
}

// ── Certificate listing (Windows) ────────────────────────────────────────

/// List certificates from the platform certificate store.
pub fn list_certificates() -> Result<Vec<CertInfo>, String> {
    #[cfg(target_os = "windows")]
    {
        win_crypt::enum_my_certs()
    }
    #[cfg(target_os = "macos")]
    {
        mac_sec::enum_identities()
    }
    #[cfg(target_os = "linux")]
    {
        linux_ssl::enum_certificates()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        Err("Certificate listing is not available on this platform".to_string())
    }
}

// ── PDF signing ─────────────────────────────────────────────────────────

// Produce a PKCS#7 detached signature over two data ranges, using
// the platform's native certificate/signing infrastructure.
fn platform_sign_data(thumbprint: &str, data1: &[u8], data2: &[u8]) -> Result<Vec<u8>, String> {
    #[cfg(target_os = "windows")]
    {
        win_crypt::sign_data(thumbprint, data1, data2)
    }
    #[cfg(target_os = "macos")]
    {
        mac_sec::sign_data(thumbprint, data1, data2)
    }
    #[cfg(target_os = "linux")]
    {
        linux_ssl::sign_data(thumbprint, data1, data2)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = (thumbprint, data1, data2);
        Err("PDF signing is not available on this platform".to_string())
    }
}

/// Digitally sign a PDF with a certificate identified by its SHA-1 thumbprint.
///
/// 1. Open the source PDF.
/// 2. Add a signature field with /ByteRange and /Contents placeholders.
/// 3. Serialize to bytes.
/// 4. Find the placeholders, patch /ByteRange with the real offsets.
/// 5. Hash the signed ranges, produce the PKCS#7 detached signature.
/// 6. Write the signature into /Contents and save to `dest_path`.
#[allow(clippy::too_many_arguments)]
pub fn sign_pdf(
    src_path: &str,
    dest_path: &str,
    page: u32,
    rect: &SignRect,
    thumbprint: &str,
    field_name: &str,
    reason: Option<&str>,
    location: Option<&str>,
    signer_name: Option<&str>,
    source_password: Option<&str>,
) -> Result<(), String> {
    // Load the source PDF
    let _ = source_password; // reserved for future encrypted PDF support
    let mut doc = Document::load(src_path).map_err(|e| format!("Could not load PDF: {e}"))?;

    // Add signature field with placeholders
    add_signature_field(
        &mut doc,
        page,
        rect,
        field_name,
        reason,
        location,
        signer_name,
    )?;

    // Serialize to a byte buffer
    let mut buf: Vec<u8> = Vec::new();
    doc.save_to(&mut buf)
        .map_err(|e| format!("Failed to serialize PDF: {e}"))?;

    // Find and patch the ByteRange sentinel
    let br_pos = find_byte_range_sentinel(&buf)?;
    let contents_range = find_contents_placeholder(&buf)?;

    // contents_range.0 = position of '<', contents_range.1 = position after '>'
    // The signed data is everything EXCEPT the <hex...> including delimiters.
    let range1_start: usize = 0;
    let range1_len: usize = contents_range.0;
    let range2_start: usize = contents_range.1;
    let range2_len: usize = buf.len() - contents_range.1;

    // Patch ByteRange — must be exactly 36 bytes like the sentinel
    let br_value = format!(
        "[0 {:<10} {:<10} {:<10}]",
        range1_len, range2_start, range2_len
    );
    if br_value.len() != 36 {
        return Err(format!(
            "ByteRange patch length mismatch: {} vs 36",
            br_value.len()
        ));
    }
    buf[br_pos..br_pos + 36].copy_from_slice(br_value.as_bytes());

    // Build the data to sign (two ranges)
    let data1 = &buf[range1_start..range1_start + range1_len];
    let data2 = &buf[range2_start..range2_start + range2_len];

    // Sign with CryptSignMessage
    let pkcs7 = platform_sign_data(thumbprint, data1, data2)?;

    if pkcs7.len() > SIG_CONTENT_BYTES {
        return Err(format!(
            "PKCS#7 signature too large: {} bytes (max {})",
            pkcs7.len(),
            SIG_CONTENT_BYTES
        ));
    }

    // Patch Contents: write hex inside the <...> delimiters
    let hex: String = pkcs7.iter().map(|b| format!("{b:02x}")).collect();
    // Pad with zeros to fill the full SIG_HEX_LEN
    let padded = format!("{hex:0<width$}", width = SIG_HEX_LEN);
    // Write into buf at contents_range.0 + 1 (skip the '<')
    buf[contents_range.0 + 1..contents_range.1 - 1].copy_from_slice(padded.as_bytes());

    // Write final signed PDF
    std::fs::write(dest_path, &buf).map_err(|e| format!("Could not write signed PDF: {e}"))?;

    Ok(())
}

// ── PDF structure helpers ────────────────────────────────────────────────

/// Add a /Sig dictionary, widget annotation, and /AcroForm to the document.
fn add_signature_field(
    doc: &mut Document,
    page: u32,
    rect: &SignRect,
    field_name: &str,
    reason: Option<&str>,
    location: Option<&str>,
    signer_name: Option<&str>,
) -> Result<(), String> {
    // Build the ByteRange placeholder: "[0 1234567890 1234567890 1234567890]"
    let br_placeholder = format!("[0 {BR_SENTINEL} {BR_SENTINEL} {BR_SENTINEL}]");

    // Build the hex Contents placeholder: "<0000...0000>" with SIG_HEX_LEN zeros

    // Signature value dictionary
    let mut sig_dict = Dictionary::new();
    sig_dict.set("Type", Object::Name(b"Sig".to_vec()));
    sig_dict.set("Filter", Object::Name(b"Adobe.PPKLite".to_vec()));
    sig_dict.set("SubFilter", Object::Name(b"adbe.pkcs7.detached".to_vec()));
    sig_dict.set(
        "ByteRange",
        Object::String(br_placeholder.into_bytes(), lopdf::StringFormat::Literal),
    );
    sig_dict.set(
        "Contents",
        Object::String(
            vec![0u8; SIG_CONTENT_BYTES],
            lopdf::StringFormat::Hexadecimal,
        ),
    );

    // Optional fields
    if let Some(r) = reason {
        sig_dict.set(
            "Reason",
            Object::String(r.as_bytes().to_vec(), lopdf::StringFormat::Literal),
        );
    }
    if let Some(l) = location {
        sig_dict.set(
            "Location",
            Object::String(l.as_bytes().to_vec(), lopdf::StringFormat::Literal),
        );
    }
    if let Some(name) = signer_name {
        sig_dict.set(
            "Name",
            Object::String(name.as_bytes().to_vec(), lopdf::StringFormat::Literal),
        );
    }

    // PDF date: D:YYYYMMDDHHmmSS+00'00'
    let now = chrono_pdf_date();
    sig_dict.set(
        "M",
        Object::String(now.into_bytes(), lopdf::StringFormat::Literal),
    );

    let sig_id = doc.add_object(sig_dict);

    // Widget annotation
    let page_ids: Vec<(u32, lopdf::ObjectId)> = doc.get_pages().into_iter().collect();
    let target_page_id = page_ids
        .iter()
        .find(|(num, _)| *num == page)
        .map(|(_, id)| *id)
        .ok_or_else(|| format!("Page {page} not found"))?;

    let mut widget = Dictionary::new();
    widget.set("Type", Object::Name(b"Annot".to_vec()));
    widget.set("Subtype", Object::Name(b"Widget".to_vec()));
    widget.set("FT", Object::Name(b"Sig".to_vec()));
    widget.set(
        "Rect",
        Object::Array(vec![
            Object::Real(rect.x as f32),
            Object::Real(rect.y as f32),
            Object::Real((rect.x + rect.width) as f32),
            Object::Real((rect.y + rect.height) as f32),
        ]),
    );
    widget.set("V", Object::Reference(sig_id));
    widget.set("P", Object::Reference(target_page_id));
    widget.set(
        "T",
        Object::String(field_name.as_bytes().to_vec(), lopdf::StringFormat::Literal),
    );
    widget.set("F", Object::Integer(132)); // Print + Locked

    let widget_id = doc.add_object(widget);

    // Add widget to the page's /Annots array
    if let Ok(page_dict) = doc.get_dictionary_mut(target_page_id) {
        if let Ok(annots) = page_dict.get_mut(b"Annots") {
            if let Object::Array(arr) = annots {
                arr.push(Object::Reference(widget_id));
            }
        } else {
            page_dict.set("Annots", Object::Array(vec![Object::Reference(widget_id)]));
        }
    }

    // Add or update /AcroForm in the catalog
    let catalog_id = doc
        .trailer
        .get(b"Root")
        .and_then(|r| r.as_reference())
        .map_err(|_| "Missing document catalog".to_string())?;

    if let Ok(catalog) = doc.get_dictionary_mut(catalog_id) {
        if let Ok(acro_ref) = catalog.get(b"AcroForm").and_then(|o| o.as_reference()) {
            // AcroForm is a reference — update the referenced dict
            let _ = catalog;
            if let Ok(acro_dict) = doc.get_dictionary_mut(acro_ref) {
                if let Ok(Object::Array(fields)) = acro_dict.get_mut(b"Fields") {
                    fields.push(Object::Reference(widget_id));
                } else {
                    acro_dict.set("Fields", Object::Array(vec![Object::Reference(widget_id)]));
                }
                acro_dict.set("SigFlags", Object::Integer(3));
            }
        } else if let Ok(Object::Dictionary(acro_dict)) = catalog.get_mut(b"AcroForm") {
            // AcroForm is inline
            if let Ok(Object::Array(fields)) = acro_dict.get_mut(b"Fields") {
                fields.push(Object::Reference(widget_id));
            } else {
                acro_dict.set("Fields", Object::Array(vec![Object::Reference(widget_id)]));
            }
            acro_dict.set("SigFlags", Object::Integer(3));
        } else {
            // No AcroForm yet — create one inline
            let mut acro = Dictionary::new();
            acro.set("Fields", Object::Array(vec![Object::Reference(widget_id)]));
            acro.set("SigFlags", Object::Integer(3));
            catalog.set("AcroForm", Object::Dictionary(acro));
        }
    }

    Ok(())
}

/// Generate a PDF date string for the current time: D:YYYYMMDDHHmmSS+00'00'
fn chrono_pdf_date() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = dur.as_secs();

    // Basic UTC breakdown (no chrono crate needed)
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Days since epoch to Y/M/D — simplified civil calendar conversion
    let (y, m, d) = days_to_ymd(days);

    format!("D:{y:04}{m:02}{d:02}{hours:02}{minutes:02}{seconds:02}+00'00'")
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Algorithm from Howard Hinnant's date algorithms
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Locate the ByteRange sentinel in the serialized PDF.
/// Returns the byte offset of the opening `[`.
fn find_byte_range_sentinel(buf: &[u8]) -> Result<usize, String> {
    let sentinel = format!("[0 {BR_SENTINEL} {BR_SENTINEL} {BR_SENTINEL}]");
    let needle = sentinel.as_bytes();
    buf.windows(needle.len())
        .position(|w| w == needle)
        .ok_or_else(|| "ByteRange sentinel not found in serialized PDF".to_string())
}

/// Locate the Contents hex placeholder in the serialized PDF.
/// Returns (start_of_angle_bracket, end_after_closing_angle_bracket).
fn find_contents_placeholder(buf: &[u8]) -> Result<(usize, usize), String> {
    // We look for '<' followed by SIG_HEX_LEN '0' characters followed by '>'
    let needle_len = SIG_HEX_LEN + 2; // <000...000>
    let zero = b'0';

    for i in 0..buf.len().saturating_sub(needle_len) {
        if buf[i] == b'<'
            && buf[i + needle_len - 1] == b'>'
            && buf[i + 1..i + needle_len - 1].iter().all(|&b| b == zero)
        {
            return Ok((i, i + needle_len));
        }
    }
    Err("Contents placeholder not found in serialized PDF".to_string())
}

// ── Windows CryptoAPI FFI ────────────────────────────────────────────────

#[cfg(target_os = "windows")]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
mod win_crypt {
    use super::CertInfo;

    // WinAPI types
    type HCERTSTORE = *mut std::ffi::c_void;
    type PCCERT_CONTEXT = *const CERT_CONTEXT;
    type DWORD = u32;
    type BOOL = i32;
    type LPCWSTR = *const u16;

    const CERT_NAME_SIMPLE_DISPLAY_TYPE: DWORD = 4;
    const CERT_NAME_ISSUER_FLAG: DWORD = 0x1;
    const PKCS_7_ASN_ENCODING: DWORD = 0x00010000;
    const X509_ASN_ENCODING: DWORD = 0x00000001;
    const MY_ENCODING: DWORD = PKCS_7_ASN_ENCODING | X509_ASN_ENCODING;
    const CERT_FIND_SHA1_HASH: DWORD = 0x10000;
    const CERT_KEY_PROV_INFO_PROP_ID: DWORD = 2;

    #[repr(C)]
    struct CERT_CONTEXT {
        encoding_type: DWORD,
        encoded_cert: *const u8,
        encoded_cert_len: DWORD,
        cert_info: *const u8, // PCERT_INFO, opaque
        store: HCERTSTORE,
    }

    #[repr(C)]
    struct CRYPT_DATA_BLOB {
        cb_data: DWORD,
        pb_data: *const u8,
    }

    #[repr(C)]
    struct CRYPT_SIGN_MESSAGE_PARA {
        cb_size: DWORD,
        msg_encoding_type: DWORD,
        signing_cert: PCCERT_CONTEXT,
        hash_algorithm: CRYPT_ALGORITHM_IDENTIFIER,
        pv_hash_aux_info: *const std::ffi::c_void,
        c_msg_cert: DWORD,
        rg_msg_cert: *const PCCERT_CONTEXT,
        c_msg_crl: DWORD,
        rg_msg_crl: *const *const std::ffi::c_void,
        c_auth_attr: DWORD,
        rg_auth_attr: *const std::ffi::c_void,
        c_unauth_attr: DWORD,
        rg_unauth_attr: *const std::ffi::c_void,
        flags: DWORD,
        inner_content_type: DWORD,
    }

    #[repr(C)]
    struct CRYPT_ALGORITHM_IDENTIFIER {
        psz_obj_id: *const u8, // LPSTR
        parameters: CRYPT_DATA_BLOB,
    }

    #[link(name = "crypt32")]
    extern "system" {
        fn CertOpenSystemStoreW(prov: usize, name: LPCWSTR) -> HCERTSTORE;
        fn CertCloseStore(store: HCERTSTORE, flags: DWORD) -> BOOL;
        fn CertEnumCertificatesInStore(store: HCERTSTORE, prev: PCCERT_CONTEXT) -> PCCERT_CONTEXT;
        fn CertGetNameStringW(
            cert: PCCERT_CONTEXT,
            name_type: DWORD,
            flags: DWORD,
            type_para: *const std::ffi::c_void,
            name_string: *mut u16,
            name_size: DWORD,
        ) -> DWORD;
        fn CertGetCertificateContextProperty(
            cert: PCCERT_CONTEXT,
            prop_id: DWORD,
            data: *mut u8,
            data_len: *mut DWORD,
        ) -> BOOL;
        fn CertFindCertificateInStore(
            store: HCERTSTORE,
            encoding: DWORD,
            flags: DWORD,
            find_type: DWORD,
            find_para: *const CRYPT_DATA_BLOB,
            prev: PCCERT_CONTEXT,
        ) -> PCCERT_CONTEXT;
        fn CertFreeCertificateContext(cert: PCCERT_CONTEXT) -> BOOL;
        fn CryptSignMessage(
            para: *const CRYPT_SIGN_MESSAGE_PARA,
            detached: BOOL,
            count: DWORD,
            data_array: *const *const u8,
            size_array: *const DWORD,
            signed_blob: *mut u8,
            signed_blob_len: *mut DWORD,
        ) -> BOOL;
    }

    /// Encode a Rust string as a null-terminated wide string.
    fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Read the display name (subject or issuer) from a cert context.
    fn cert_name(ctx: PCCERT_CONTEXT, issuer: bool) -> String {
        let flags = if issuer { CERT_NAME_ISSUER_FLAG } else { 0 };
        let mut buf = vec![0u16; 256];
        unsafe {
            let len = CertGetNameStringW(
                ctx,
                CERT_NAME_SIMPLE_DISPLAY_TYPE,
                flags,
                std::ptr::null(),
                buf.as_mut_ptr(),
                buf.len() as DWORD,
            );
            if len <= 1 {
                return String::new();
            }
            String::from_utf16_lossy(&buf[..len as usize - 1])
        }
    }

    /// Get the SHA-1 thumbprint of a certificate context.
    fn cert_thumbprint(ctx: PCCERT_CONTEXT) -> String {
        let mut len: DWORD = 20;
        let mut hash = [0u8; 20];
        unsafe {
            // CERT_HASH_PROP_ID = 3
            CertGetCertificateContextProperty(ctx, 3, hash.as_mut_ptr(), &mut len);
        }
        hash.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Check whether a certificate has an associated private key.
    fn has_private_key(ctx: PCCERT_CONTEXT) -> bool {
        let mut len: DWORD = 0;
        unsafe {
            CertGetCertificateContextProperty(
                ctx,
                CERT_KEY_PROV_INFO_PROP_ID,
                std::ptr::null_mut(),
                &mut len,
            ) != 0
        }
    }

    /// Enumerate all certificates in the "MY" store.
    pub fn enum_my_certs() -> Result<Vec<CertInfo>, String> {
        let store_name = to_wide("MY");
        let store = unsafe { CertOpenSystemStoreW(0, store_name.as_ptr()) };
        if store.is_null() {
            return Err("Could not open the MY certificate store".to_string());
        }

        let mut certs = Vec::new();
        let mut ctx: PCCERT_CONTEXT = std::ptr::null();
        loop {
            ctx = unsafe { CertEnumCertificatesInStore(store, ctx) };
            if ctx.is_null() {
                break;
            }
            certs.push(CertInfo {
                thumbprint: cert_thumbprint(ctx),
                subject: cert_name(ctx, false),
                issuer: cert_name(ctx, true),
                has_private_key: has_private_key(ctx),
            });
        }

        unsafe {
            CertCloseStore(store, 0);
        }
        Ok(certs)
    }

    /// Find a certificate by SHA-1 thumbprint and sign data with it.
    /// Returns the PKCS#7 detached signature blob.
    pub fn sign_data(thumbprint: &str, data1: &[u8], data2: &[u8]) -> Result<Vec<u8>, String> {
        let hash_bytes = hex_decode(thumbprint)?;
        if hash_bytes.len() != 20 {
            return Err("Thumbprint must be 40 hex characters (SHA-1)".to_string());
        }

        let store_name = to_wide("MY");
        let store = unsafe { CertOpenSystemStoreW(0, store_name.as_ptr()) };
        if store.is_null() {
            return Err("Could not open the MY certificate store".to_string());
        }

        let blob = CRYPT_DATA_BLOB {
            cb_data: hash_bytes.len() as DWORD,
            pb_data: hash_bytes.as_ptr(),
        };

        let ctx = unsafe {
            CertFindCertificateInStore(
                store,
                MY_ENCODING,
                0,
                CERT_FIND_SHA1_HASH,
                &blob,
                std::ptr::null(),
            )
        };

        if ctx.is_null() {
            unsafe { CertCloseStore(store, 0) };
            return Err(format!(
                "Certificate with thumbprint {thumbprint} not found"
            ));
        }

        // SHA-256 OID: 2.16.840.1.101.3.4.2.1
        let sha256_oid = b"2.16.840.1.101.3.4.2.1\0";

        let hash_alg = CRYPT_ALGORITHM_IDENTIFIER {
            psz_obj_id: sha256_oid.as_ptr(),
            parameters: CRYPT_DATA_BLOB {
                cb_data: 0,
                pb_data: std::ptr::null(),
            },
        };

        let para = CRYPT_SIGN_MESSAGE_PARA {
            cb_size: std::mem::size_of::<CRYPT_SIGN_MESSAGE_PARA>() as DWORD,
            msg_encoding_type: MY_ENCODING,
            signing_cert: ctx,
            hash_algorithm: hash_alg,
            pv_hash_aux_info: std::ptr::null(),
            c_msg_cert: 1,
            rg_msg_cert: &ctx,
            c_msg_crl: 0,
            rg_msg_crl: std::ptr::null(),
            c_auth_attr: 0,
            rg_auth_attr: std::ptr::null(),
            c_unauth_attr: 0,
            rg_unauth_attr: std::ptr::null(),
            flags: 0,
            inner_content_type: 0,
        };

        // Concatenate data1 + data2 for signing (CryptSignMessage wants
        // contiguous ranges but we'll pass them as two entries)
        let data_ptrs = [data1.as_ptr(), data2.as_ptr()];
        let data_sizes = [data1.len() as DWORD, data2.len() as DWORD];

        // First call: get required size
        let mut sig_len: DWORD = 0;
        let ok = unsafe {
            CryptSignMessage(
                &para,
                1, // detached = TRUE
                2, // two data buffers
                data_ptrs.as_ptr(),
                data_sizes.as_ptr(),
                std::ptr::null_mut(),
                &mut sig_len,
            )
        };
        if ok == 0 {
            unsafe {
                CertFreeCertificateContext(ctx);
                CertCloseStore(store, 0);
            }
            return Err("CryptSignMessage failed to compute signature size".to_string());
        }

        // Second call: get the actual signature
        let mut sig_buf = vec![0u8; sig_len as usize];
        let ok = unsafe {
            CryptSignMessage(
                &para,
                1,
                2,
                data_ptrs.as_ptr(),
                data_sizes.as_ptr(),
                sig_buf.as_mut_ptr(),
                &mut sig_len,
            )
        };

        unsafe {
            CertFreeCertificateContext(ctx);
            CertCloseStore(store, 0);
        }

        if ok == 0 {
            return Err("CryptSignMessage failed to produce signature".to_string());
        }

        sig_buf.truncate(sig_len as usize);
        Ok(sig_buf)
    }

    /// Decode a hex string to bytes.
    fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
        let s = s.trim();
        if s.len() % 2 != 0 {
            return Err("Hex string has odd length".to_string());
        }
        (0..s.len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(&s[i..i + 2], 16)
                    .map_err(|e| format!("Invalid hex at offset {i}: {e}"))
            })
            .collect()
    }
}

// ── macOS: Security.framework CLI wrappers ──────────────────────────────

#[cfg(target_os = "macos")]
mod mac_sec {
    use super::CertInfo;
    use std::io::Write;
    use std::process::Command;

    /// Run `security find-identity -v` to list signing identities in the
    /// default keychain.  Each matching line looks like:
    ///
    ///   1) ABCDEF1234... "Common Name (details)"
    ///
    /// We extract the SHA-1 hash and the quoted display name.
    pub fn enum_identities() -> Result<Vec<CertInfo>, String> {
        let output = Command::new("security")
            .args(["find-identity", "-v"])
            .output()
            .map_err(|e| format!("Failed to run `security find-identity`: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut certs = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for line in stdout.lines() {
            let line = line.trim();
            // Skip summary lines like "2 valid identities found"
            if !line.starts_with(|c: char| c.is_ascii_digit()) {
                continue;
            }
            // Format: N) <40-char hex hash> "Display Name"
            let after_paren = match line.find(')') {
                Some(idx) => &line[idx + 1..],
                None => continue,
            };
            let after_paren = after_paren.trim();
            if after_paren.len() < 40 {
                continue;
            }
            let hash_part = &after_paren[..40];
            if !hash_part.chars().all(|c| c.is_ascii_hexdigit()) {
                continue;
            }
            let thumbprint = hash_part.to_lowercase();
            if !seen.insert(thumbprint.clone()) {
                continue;
            }
            // Display name sits inside double quotes
            let display = if let Some(start) = after_paren.find('"') {
                let rest = &after_paren[start + 1..];
                if let Some(end) = rest.find('"') {
                    rest[..end].to_string()
                } else {
                    rest.to_string()
                }
            } else {
                thumbprint.clone()
            };

            certs.push(CertInfo {
                thumbprint,
                subject: display.clone(),
                issuer: display,
                has_private_key: true, // find-identity only shows identities with keys
            });
        }

        Ok(certs)
    }

    /// Sign `data1 || data2` using `security cms -S`.
    ///
    /// We concatenate the byte ranges into a temp file, invoke
    /// `security cms -S -N <hash> -H SHA256 -noattr` and read back the
    /// DER-encoded CMS blob.
    pub fn sign_data(thumbprint: &str, data1: &[u8], data2: &[u8]) -> Result<Vec<u8>, String> {
        let tmp_dir = std::env::temp_dir();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let in_path = tmp_dir.join(format!("lpdf_sign_in_{stamp}.bin"));
        let out_path = tmp_dir.join(format!("lpdf_sign_out_{stamp}.der"));

        // Write concatenated data
        let mut f =
            std::fs::File::create(&in_path).map_err(|e| format!("Create temp file: {e}"))?;
        f.write_all(data1)
            .map_err(|e| format!("Write data1: {e}"))?;
        f.write_all(data2)
            .map_err(|e| format!("Write data2: {e}"))?;
        drop(f);

        let result = Command::new("security")
            .args([
                "cms", "-S", "-H", "SHA256", "-N", thumbprint, "-noattr", "-i",
            ])
            .arg(&in_path)
            .arg("-o")
            .arg(&out_path)
            .output()
            .map_err(|e| format!("Failed to run `security cms -S`: {e}"))?;

        let _ = std::fs::remove_file(&in_path);

        if !result.status.success() {
            let _ = std::fs::remove_file(&out_path);
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(format!("security cms signing failed: {stderr}"));
        }

        let sig = std::fs::read(&out_path).map_err(|e| format!("Read CMS output: {e}"))?;
        let _ = std::fs::remove_file(&out_path);

        if sig.is_empty() {
            return Err("security cms produced empty output".to_string());
        }
        Ok(sig)
    }
}

// ── Linux: OpenSSL / NSS CLI wrappers ───────────────────────────────────

#[cfg(target_os = "linux")]
mod linux_ssl {
    use super::CertInfo;
    use std::io::Write;
    use std::process::Command;

    /// Try to list certificates from the NSS database (`certutil`) first,
    /// falling back to `pkcs15-tool` (smart cards) when NSS is unavailable.
    pub fn enum_certificates() -> Result<Vec<CertInfo>, String> {
        // Try NSS db first (Firefox / Chrome shared db)
        if let Ok(certs) = list_nss_certs() {
            if !certs.is_empty() {
                return Ok(certs);
            }
        }
        // Fall back to pkcs15-tool (smart cards / PIV)
        if let Ok(certs) = list_pkcs15_certs() {
            if !certs.is_empty() {
                return Ok(certs);
            }
        }
        Ok(Vec::new())
    }

    /// List certificates from `~/.pki/nssdb`.
    fn list_nss_certs() -> Result<Vec<CertInfo>, String> {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        let db_dir = format!("sql:{home}/.pki/nssdb");

        let output = Command::new("certutil")
            .args(["-L", "-d", &db_dir])
            .output()
            .map_err(|e| format!("certutil: {e}"))?;

        if !output.status.success() {
            return Err("certutil -L failed".to_string());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut certs = Vec::new();

        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty()
                || line.starts_with("Certificate Nickname")
                || line.contains("Trust Attributes")
                || line.starts_with('-')
            {
                continue;
            }
            // "nickname   trust-flags" — 'u' flag means user cert with key
            let has_key = line.contains(",u,") || line.ends_with(",u");
            let parts: Vec<&str> = line.splitn(2, "  ").collect();
            if parts.is_empty() {
                continue;
            }
            let nickname = parts[0].trim().to_string();
            if nickname.is_empty() {
                continue;
            }

            let thumbprint = nss_cert_hash(&db_dir, &nickname).unwrap_or_default();
            if thumbprint.is_empty() {
                continue;
            }

            certs.push(CertInfo {
                thumbprint,
                subject: nickname.clone(),
                issuer: nickname,
                has_private_key: has_key,
            });
        }

        Ok(certs)
    }

    /// SHA-1 fingerprint of a cert via `certutil -L -a | openssl x509 -fingerprint`.
    fn nss_cert_hash(db_dir: &str, nickname: &str) -> Result<String, String> {
        let pem = Command::new("certutil")
            .args(["-L", "-d", db_dir, "-n", nickname, "-a"])
            .output()
            .map_err(|e| format!("certutil: {e}"))?;
        if !pem.status.success() {
            return Err("certutil export failed".to_string());
        }

        let mut openssl = Command::new("openssl")
            .args(["x509", "-noout", "-fingerprint", "-sha1"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("openssl: {e}"))?;

        if let Some(ref mut stdin) = openssl.stdin {
            let _ = stdin.write_all(&pem.stdout);
        }
        let out = openssl
            .wait_with_output()
            .map_err(|e| format!("openssl wait: {e}"))?;

        // "sha1 Fingerprint=AA:BB:CC:..."
        let text = String::from_utf8_lossy(&out.stdout);
        if let Some(eq) = text.find('=') {
            let hex = text[eq + 1..].trim().replace(':', "").to_lowercase();
            if hex.len() == 40 {
                return Ok(hex);
            }
        }
        Err("Could not parse SHA-1 fingerprint".to_string())
    }

    /// List certificates from a PKCS#15 smart card.
    fn list_pkcs15_certs() -> Result<Vec<CertInfo>, String> {
        let output = Command::new("pkcs15-tool")
            .args(["--list-certificates"])
            .output()
            .map_err(|e| format!("pkcs15-tool: {e}"))?;

        if !output.status.success() {
            return Err("pkcs15-tool failed".to_string());
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut certs = Vec::new();
        let mut label = String::new();
        let mut id = String::new();

        for line in stdout.lines() {
            let line = line.trim();
            if line.starts_with("X.509 Certificate") {
                label.clear();
                id.clear();
            } else if let Some(rest) = line.strip_prefix("Label:") {
                label = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("ID:") {
                id = rest.trim().to_lowercase();
            }
            if !label.is_empty() && !id.is_empty() {
                certs.push(CertInfo {
                    thumbprint: id.clone(),
                    subject: label.clone(),
                    issuer: String::new(),
                    has_private_key: true,
                });
                label.clear();
                id.clear();
            }
        }

        Ok(certs)
    }

    /// Sign `data1 || data2` with `openssl cms -sign`.
    ///
    /// For NSS-stored certs: export via `pk12util` → extract cert/key with
    /// `openssl pkcs12` → `openssl cms -sign -signer cert.pem -inkey key.pem`.
    pub fn sign_data(thumbprint: &str, data1: &[u8], data2: &[u8]) -> Result<Vec<u8>, String> {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
        let db_dir = format!("sql:{home}/.pki/nssdb");
        let tmp = std::env::temp_dir();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        let data_path = tmp.join(format!("lpdf_sign_{ts}.bin"));
        let p12_path = tmp.join(format!("lpdf_sign_{ts}.p12"));
        let cert_path = tmp.join(format!("lpdf_sign_{ts}_cert.pem"));
        let key_path = tmp.join(format!("lpdf_sign_{ts}_key.pem"));
        let out_path = tmp.join(format!("lpdf_sign_{ts}.der"));

        // Write combined data
        let mut f = std::fs::File::create(&data_path).map_err(|e| format!("Create temp: {e}"))?;
        f.write_all(data1)
            .map_err(|e| format!("Write data1: {e}"))?;
        f.write_all(data2)
            .map_err(|e| format!("Write data2: {e}"))?;
        drop(f);

        // Find the NSS nickname for this thumbprint
        let nickname = find_nickname_by_hash(&db_dir, thumbprint)?;

        // Export cert+key to PKCS#12
        let p12 = Command::new("pk12util")
            .args([
                "-o",
                p12_path.to_str().unwrap_or_default(),
                "-d",
                &db_dir,
                "-n",
                &nickname,
                "-W",
                "",
            ])
            .output()
            .map_err(|e| format!("pk12util: {e}"))?;
        if !p12.status.success() {
            cleanup(&[&data_path, &p12_path]);
            let stderr = String::from_utf8_lossy(&p12.stderr);
            return Err(format!("pk12util export failed: {stderr}"));
        }

        // Extract certificate PEM
        let ce = Command::new("openssl")
            .args([
                "pkcs12",
                "-in",
                p12_path.to_str().unwrap_or_default(),
                "-clcerts",
                "-nokeys",
                "-out",
                cert_path.to_str().unwrap_or_default(),
                "-passin",
                "pass:",
                "-passout",
                "pass:",
            ])
            .output()
            .map_err(|e| format!("openssl pkcs12 cert: {e}"))?;
        if !ce.status.success() {
            cleanup(&[&data_path, &p12_path, &cert_path]);
            return Err("Failed to extract cert from PKCS#12".to_string());
        }

        // Extract private key PEM
        let ke = Command::new("openssl")
            .args([
                "pkcs12",
                "-in",
                p12_path.to_str().unwrap_or_default(),
                "-nocerts",
                "-nodes",
                "-out",
                key_path.to_str().unwrap_or_default(),
                "-passin",
                "pass:",
            ])
            .output()
            .map_err(|e| format!("openssl pkcs12 key: {e}"))?;
        if !ke.status.success() {
            cleanup(&[&data_path, &p12_path, &cert_path, &key_path]);
            return Err("Failed to extract key from PKCS#12".to_string());
        }

        // Sign with openssl cms
        let sig_result = Command::new("openssl")
            .args([
                "cms",
                "-sign",
                "-binary",
                "-noattr",
                "-md",
                "sha256",
                "-signer",
                cert_path.to_str().unwrap_or_default(),
                "-inkey",
                key_path.to_str().unwrap_or_default(),
                "-in",
                data_path.to_str().unwrap_or_default(),
                "-outform",
                "DER",
                "-out",
                out_path.to_str().unwrap_or_default(),
            ])
            .output()
            .map_err(|e| format!("openssl cms: {e}"))?;

        // Cleanup sensitive temp files immediately
        cleanup(&[&data_path, &p12_path, &cert_path, &key_path]);

        if !sig_result.status.success() {
            let _ = std::fs::remove_file(&out_path);
            let stderr = String::from_utf8_lossy(&sig_result.stderr);
            return Err(format!("openssl cms sign failed: {stderr}"));
        }

        let sig = std::fs::read(&out_path).map_err(|e| format!("Read CMS output: {e}"))?;
        let _ = std::fs::remove_file(&out_path);

        if sig.is_empty() {
            return Err("openssl cms produced empty output".to_string());
        }
        Ok(sig)
    }

    /// Map a SHA-1 thumbprint back to the NSS nickname.
    fn find_nickname_by_hash(db_dir: &str, thumbprint: &str) -> Result<String, String> {
        let output = Command::new("certutil")
            .args(["-L", "-d", db_dir])
            .output()
            .map_err(|e| format!("certutil: {e}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let line = line.trim();
            if line.is_empty()
                || line.starts_with("Certificate Nickname")
                || line.contains("Trust Attributes")
                || line.starts_with('-')
            {
                continue;
            }
            let parts: Vec<&str> = line.splitn(2, "  ").collect();
            if parts.is_empty() {
                continue;
            }
            let nickname = parts[0].trim();
            if nickname.is_empty() {
                continue;
            }
            if let Ok(hash) = nss_cert_hash(db_dir, nickname) {
                if hash == thumbprint {
                    return Ok(nickname.to_string());
                }
            }
        }
        Err(format!("No certificate found with thumbprint {thumbprint}"))
    }

    fn cleanup(paths: &[&std::path::PathBuf]) {
        for p in paths {
            let _ = std::fs::remove_file(p);
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── create_stamp_pdf ────────────────────────────────────────────────

    #[test]
    fn stamp_pdf_empty_bytes() {
        let err = create_stamp_pdf(&[]).unwrap_err();
        assert!(err.contains("empty"), "Error: {err}");
    }

    #[test]
    fn stamp_pdf_invalid_bytes() {
        let err = create_stamp_pdf(b"not a png").unwrap_err();
        assert!(err.contains("Could not decode"), "Error: {err}");
    }

    #[test]
    fn stamp_pdf_valid_png() {
        // Create a small 80×30 test PNG in memory
        let img = image::RgbImage::from_fn(80, 30, |x, _| image::Rgb([(x % 256) as u8, 0, 0]));
        let mut png_buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png_buf, image::ImageFormat::Png)
            .expect("encode PNG");
        let png_bytes = png_buf.into_inner();

        let path = create_stamp_pdf(&png_bytes).expect("should create stamp PDF");
        assert!(std::path::Path::new(&path).exists());

        let doc = Document::load(&path).expect("valid PDF");
        assert_eq!(doc.get_pages().len(), 1);

        // Verify it has an image XObject
        let pages = doc.get_pages();
        let (_, &page_id) = pages.iter().next().unwrap();
        let page_dict = doc.get_dictionary(page_id).unwrap();
        let resources = page_dict.get(b"Resources").unwrap().as_dict().unwrap();
        assert!(resources.has(b"XObject"));

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn stamp_pdf_page_dimensions_match_image() {
        let img = image::RgbImage::new(200, 50);
        let mut png_buf = std::io::Cursor::new(Vec::new());
        img.write_to(&mut png_buf, image::ImageFormat::Png)
            .expect("encode PNG");

        let path = create_stamp_pdf(&png_buf.into_inner()).unwrap();
        let doc = Document::load(&path).unwrap();

        let pages = doc.get_pages();
        let (_, &page_id) = pages.iter().next().unwrap();
        let page_dict = doc.get_dictionary(page_id).unwrap();
        let mb = page_dict.get(b"MediaBox").unwrap().as_array().unwrap();

        let w = match &mb[2] {
            Object::Real(v) => *v as f64,
            Object::Integer(v) => *v as f64,
            _ => panic!("unexpected"),
        };
        let h = match &mb[3] {
            Object::Real(v) => *v as f64,
            Object::Integer(v) => *v as f64,
            _ => panic!("unexpected"),
        };

        // 200×50 image → 4:1 aspect ratio
        let ratio = w / h;
        assert!(
            (ratio - 4.0).abs() < 0.01,
            "aspect ratio should be ~4:1, got {ratio}"
        );

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn stamp_pdf_unique_paths() {
        let img = image::RgbImage::new(10, 10);
        let mut buf1 = std::io::Cursor::new(Vec::new());
        img.write_to(&mut buf1, image::ImageFormat::Png).unwrap();
        let bytes = buf1.into_inner();

        let p1 = create_stamp_pdf(&bytes).unwrap();
        let p2 = create_stamp_pdf(&bytes).unwrap();
        assert_ne!(p1, p2);
        let _ = std::fs::remove_file(&p1);
        let _ = std::fs::remove_file(&p2);
    }

    // ── PDF structure helpers ───────────────────────────────────────────

    #[test]
    fn chrono_pdf_date_format() {
        let d = chrono_pdf_date();
        assert!(d.starts_with("D:"), "Should start with D: but got {d}");
        assert!(d.ends_with("+00'00'"), "Should end with UTC offset");
        // D:YYYYMMDDHHmmSS+00'00' = 23 chars
        assert_eq!(d.len(), 23, "PDF date should be 23 chars: {d}");
    }

    #[test]
    fn days_to_ymd_epoch() {
        let (y, m, d) = days_to_ymd(0);
        assert_eq!((y, m, d), (1970, 1, 1));
    }

    #[test]
    fn days_to_ymd_known_date() {
        // 2024-02-29 (leap day) is day 19782
        let (y, m, d) = days_to_ymd(19782);
        assert_eq!((y, m, d), (2024, 2, 29));
    }

    #[test]
    fn days_to_ymd_another_date() {
        // 2000-01-01 is day 10957
        let (y, m, d) = days_to_ymd(10957);
        assert_eq!((y, m, d), (2000, 1, 1));
    }

    #[test]
    fn find_byte_range_sentinel_found() {
        let sentinel = format!("[0 {BR_SENTINEL} {BR_SENTINEL} {BR_SENTINEL}]");
        let mut buf = b"prefix data ".to_vec();
        buf.extend_from_slice(sentinel.as_bytes());
        buf.extend_from_slice(b" suffix data");

        let pos = find_byte_range_sentinel(&buf).unwrap();
        assert_eq!(pos, 12); // "prefix data " is 12 bytes
    }

    #[test]
    fn find_byte_range_sentinel_not_found() {
        let buf = b"this PDF has no sentinel";
        assert!(find_byte_range_sentinel(buf).is_err());
    }

    #[test]
    fn find_contents_placeholder_found() {
        let zeros = "0".repeat(SIG_HEX_LEN);
        let placeholder = format!("<{zeros}>");
        let mut buf = b"some data ".to_vec();
        buf.extend_from_slice(placeholder.as_bytes());
        buf.extend_from_slice(b" more data");

        let (start, end) = find_contents_placeholder(&buf).unwrap();
        assert_eq!(start, 10);
        assert_eq!(end, 10 + SIG_HEX_LEN + 2);
    }

    #[test]
    fn find_contents_placeholder_not_found() {
        let buf = b"no placeholder here at all";
        assert!(find_contents_placeholder(buf).is_err());
    }

    #[test]
    fn add_signature_field_creates_widget() {
        let mut doc = Document::with_version("1.7");

        // Build a minimal one-page document
        let content_id = doc.add_object(Stream::new(Dictionary::new(), vec![]));
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();

        let mut page = Dictionary::new();
        page.set("Type", Object::Name(b"Page".to_vec()));
        page.set("Parent", Object::Reference(pages_id));
        page.set(
            "MediaBox",
            Object::Array(vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Real(612.0),
                Object::Real(792.0),
            ]),
        );
        page.set("Contents", Object::Reference(content_id));
        doc.objects.insert(page_id, Object::Dictionary(page));

        let mut pages = Dictionary::new();
        pages.set("Type", Object::Name(b"Pages".to_vec()));
        pages.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
        pages.set("Count", Object::Integer(1));
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let mut catalog = Dictionary::new();
        catalog.set("Type", Object::Name(b"Catalog".to_vec()));
        catalog.set("Pages", Object::Reference(pages_id));
        let catalog_id = doc.add_object(catalog);
        doc.trailer.set("Root", catalog_id);

        let rect = SignRect {
            x: 100.0,
            y: 100.0,
            width: 200.0,
            height: 50.0,
        };

        add_signature_field(
            &mut doc,
            1,
            &rect,
            "Signature1",
            Some("Approval"),
            Some("Office"),
            Some("Test User"),
        )
        .expect("should add signature field");

        // Verify the page now has annotations
        let page_dict = doc.get_dictionary(page_id).unwrap();
        let annots = page_dict.get(b"Annots").unwrap().as_array().unwrap();
        assert_eq!(annots.len(), 1);

        // Verify AcroForm was created in the catalog
        let cat = doc.get_dictionary(catalog_id).unwrap();
        let acro = cat.get(b"AcroForm").unwrap().as_dict().unwrap();
        let sig_flags = acro.get(b"SigFlags").unwrap().as_i64().unwrap();
        assert_eq!(sig_flags, 3);
        let fields = acro.get(b"Fields").unwrap().as_array().unwrap();
        assert_eq!(fields.len(), 1);
    }

    #[test]
    fn add_signature_field_invalid_page() {
        let mut doc = Document::with_version("1.7");

        let pages_id = doc.new_object_id();
        let mut pages = Dictionary::new();
        pages.set("Type", Object::Name(b"Pages".to_vec()));
        pages.set("Kids", Object::Array(vec![]));
        pages.set("Count", Object::Integer(0));
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let mut catalog = Dictionary::new();
        catalog.set("Type", Object::Name(b"Catalog".to_vec()));
        catalog.set("Pages", Object::Reference(pages_id));
        let catalog_id = doc.add_object(catalog);
        doc.trailer.set("Root", catalog_id);

        let rect = SignRect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 50.0,
        };

        let err = add_signature_field(&mut doc, 99, &rect, "Sig1", None, None, None);
        assert!(err.is_err());
        assert!(err.unwrap_err().contains("not found"));
    }

    // ── list_certificates (non-Windows) ─────────────────────────────────

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn list_certificates_returns_error_on_non_windows() {
        let err = list_certificates().unwrap_err();
        assert!(err.contains("Windows"));
    }

    // ── sign_pdf (non-Windows) ──────────────────────────────────────────

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn sign_pdf_returns_error_on_non_windows() {
        let rect = SignRect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 50.0,
        };
        let err = sign_pdf(
            "test.pdf", "out.pdf", 1, &rect, "aabbccdd", "Sig1", None, None, None, None,
        )
        .unwrap_err();
        assert!(err.contains("Windows"));
    }

    // ── Windows-specific tests ──────────────────────────────────────────

    #[cfg(target_os = "windows")]
    mod windows_tests {
        use super::super::*;

        #[test]
        fn list_certificates_runs() {
            // Should at least return Ok (may be empty if no certs installed)
            let result = list_certificates();
            assert!(result.is_ok(), "Failed: {:?}", result.err());
        }

        #[test]
        fn sign_pdf_bad_thumbprint() {
            // Create a minimal valid PDF to try signing
            let src = crate::insert::create_blank_pdf(612.0, 792.0).unwrap();
            let dest = temp_pdf_path("sign-test");

            let rect = SignRect {
                x: 100.0,
                y: 100.0,
                width: 200.0,
                height: 50.0,
            };

            let err = sign_pdf(
                &src,
                &dest.to_string_lossy(),
                1,
                &rect,
                "0000000000000000000000000000000000000000",
                "Sig1",
                None,
                None,
                None,
                None,
            );
            // Should fail because the thumbprint won't match any real cert
            assert!(err.is_err());

            let _ = std::fs::remove_file(&src);
            let _ = std::fs::remove_file(&dest);
        }
    }
}
