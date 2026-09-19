//! Digital signature extraction and basic validation.
//!
//! Reads existing signature fields from a PDF, extracts signer information
//! from the embedded PKCS#7/CMS data, and checks whether the signed byte
//! ranges still cover the document without post-sign modifications.

use lopdf::{Document, Object, ObjectId};
use serde::Serialize;

// ── Public types ─────────────────────────────────────────────────────────

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SignatureInfo {
    /// The PDF field name (/T chain) for this signature.
    pub field_name: String,
    /// Signer's common name, extracted from the certificate subject or the
    /// /Name entry in the signature dictionary.
    pub signer_name: Option<String>,
    /// Signing time as an ISO-8601-ish string, from the CMS signed
    /// attributes or the /M entry.
    pub signing_time: Option<String>,
    /// Reason given for signing (/Reason).
    pub reason: Option<String>,
    /// Location where the document was signed (/Location).
    pub location: Option<String>,
    /// The sub-filter (e.g. "adbe.pkcs7.detached", "ETSI.CAdES.detached").
    pub sub_filter: Option<String>,
    /// Whether the ByteRange covers the full file (no bytes outside the
    /// range and signature placeholder). `false` means the document was
    /// modified after signing.
    pub covers_whole_doc: bool,
    /// Overall status label for the UI.
    pub status: String,
}

// ── Entry point ──────────────────────────────────────────────────────────

/// Extract every digital signature from the PDF at `path`.
pub fn extract_signatures(
    path: &str,
    password: Option<&str>,
) -> Result<Vec<SignatureInfo>, String> {
    let mut doc =
        Document::load(path).map_err(|e| format!("Couldn't open \"{path}\": {e}"))?;
    if doc.is_encrypted() {
        let pw = password.unwrap_or("");
        doc.decrypt(pw)
            .map_err(|e| format!("Couldn't decrypt source PDF: {e}"))?;
    }

    let file_bytes = std::fs::read(path)
        .map_err(|e| format!("Couldn't read \"{path}\": {e}"))?;
    let file_len = file_bytes.len();

    let sig_dicts = collect_sig_dicts(&doc);
    let mut out = Vec::new();

    for (field_name, dict) in sig_dicts {
        let info = parse_sig_dict(&doc, &dict, field_name, &file_bytes, file_len);
        out.push(info);
    }

    Ok(out)
}

// ── Internals ────────────────────────────────────────────────────────────

/// Walk the AcroForm field tree and collect every /Sig value dictionary,
/// together with its field name.
fn collect_sig_dicts(doc: &Document) -> Vec<(String, Vec<(Vec<u8>, Object)>)> {
    let mut results = Vec::new();

    let fields = match acroform_fields(doc) {
        Some(f) => f,
        None => return results,
    };

    for field_id in fields {
        collect_sig_fields_recursive(doc, field_id, String::new(), &mut results);
    }

    results
}

/// Recursively descend into field tree nodes. Signature fields have
/// /FT = /Sig and a /V dictionary containing the PKCS#7 data.
fn collect_sig_fields_recursive(
    doc: &Document,
    obj_id: ObjectId,
    parent_name: String,
    out: &mut Vec<(String, Vec<(Vec<u8>, Object)>)>,
) {
    let dict = match doc.objects.get(&obj_id).and_then(|o| o.as_dict().ok()) {
        Some(d) => d,
        None => return,
    };

    // Build the fully-qualified field name.
    let local_name = dict
        .get(b"T")
        .ok()
        .and_then(|o| pdf_string(o))
        .unwrap_or_default();
    let fq_name = if parent_name.is_empty() {
        local_name.clone()
    } else if local_name.is_empty() {
        parent_name.clone()
    } else {
        format!("{parent_name}.{local_name}")
    };

    // Check if this node has /FT = /Sig.
    let is_sig = dict
        .get(b"FT")
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(|n| n == b"Sig")
        .unwrap_or(false);

    if is_sig {
        // The value dictionary /V holds the signature data.
        if let Some(v_obj) = dict.get(b"V").ok() {
            let v_dict = resolve_dict(doc, v_obj);
            if !v_dict.is_empty() {
                out.push((fq_name.clone(), v_dict));
            }
        }
    }

    // Recurse into /Kids if present.
    if let Ok(kids) = dict.get(b"Kids") {
        if let Ok(arr) = resolve_obj(doc, kids).as_array() {
            for kid in arr {
                if let Ok(id) = kid.as_reference() {
                    collect_sig_fields_recursive(doc, id, fq_name.clone(), out);
                }
            }
        }
    }
}

/// Parse a signature value dictionary into a `SignatureInfo`.
fn parse_sig_dict(
    _doc: &Document,
    dict_entries: &[(Vec<u8>, Object)],
    field_name: String,
    _file_bytes: &[u8],
    file_len: usize,
) -> SignatureInfo {
    let get = |key: &[u8]| -> Option<&Object> {
        dict_entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    };

    // /SubFilter — e.g. adbe.pkcs7.detached
    let sub_filter = get(b"SubFilter")
        .and_then(|o| o.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).to_string());

    // /Name — signer name from the dictionary (fallback if cert parsing fails).
    let dict_name = get(b"Name").and_then(|o| pdf_string(o));

    // /M — signing time from the dictionary.
    let dict_time = get(b"M").and_then(|o| pdf_string(o)).map(|s| parse_pdf_date(&s));

    // /Reason
    let reason = get(b"Reason").and_then(|o| pdf_string(o));

    // /Location
    let location = get(b"Location").and_then(|o| pdf_string(o));

    // /ByteRange — array of 4 integers [off1, len1, off2, len2].
    let byte_range = get(b"ByteRange").and_then(|o| parse_byte_range(o));

    // Check byte coverage.
    let covers_whole_doc = byte_range
        .as_ref()
        .map(|br| check_byte_coverage(br, file_len))
        .unwrap_or(false);

    // /Contents — the PKCS#7 DER blob (hex-encoded in the PDF, but lopdf
    // gives us the raw bytes).
    let pkcs7_bytes = get(b"Contents").and_then(|o| match o {
        Object::String(bytes, _) => Some(bytes.clone()),
        _ => None,
    });

    // Try to extract signer name and time from the PKCS#7 data.
    let (cert_name, cert_time) = pkcs7_bytes
        .as_ref()
        .map(|b| parse_pkcs7_signer(b))
        .unwrap_or((None, None));

    let signer_name = cert_name.or(dict_name);
    let signing_time = cert_time.or(dict_time);

    // Determine status.
    let status = if byte_range.is_none() {
        "unknown".to_string()
    } else if covers_whole_doc {
        "signed".to_string()
    } else {
        "modified".to_string()
    };

    SignatureInfo {
        field_name,
        signer_name,
        signing_time,
        reason,
        location,
        sub_filter,
        covers_whole_doc,
        status,
    }
}

/// Get the AcroForm /Fields array as a list of object IDs.
fn acroform_fields(doc: &Document) -> Option<Vec<ObjectId>> {
    let catalog = doc.catalog().ok()?;
    let acro_ref = catalog.get(b"AcroForm").ok()?;
    let acro_dict = resolve_dict_ref(doc, acro_ref)?;
    let fields_obj = acro_dict.iter().find(|(k, _)| &k[..] == b"Fields")?.1;
    let arr = resolve_obj(doc, fields_obj).as_array().ok()?;
    Some(
        arr.iter()
            .filter_map(|o| o.as_reference().ok())
            .collect(),
    )
}

// ── ByteRange validation ─────────────────────────────────────────────────

/// A byte range is typically [0, off_before_sig, off_after_sig, len_after].
/// The two ranges should together cover the entire file except the hex-
/// encoded signature placeholder (the /Contents value).
fn parse_byte_range(obj: &Object) -> Option<Vec<i64>> {
    let arr = obj.as_array().ok()?;
    if arr.len() != 4 {
        return None;
    }
    let nums: Option<Vec<i64>> = arr.iter().map(|o| o.as_i64().ok()).collect();
    nums
}

fn check_byte_coverage(br: &[i64], file_len: usize) -> bool {
    if br.len() != 4 {
        return false;
    }
    let (off1, len1, off2, len2) = (br[0], br[1], br[2], br[3]);
    // Range 1 must start at the beginning.
    if off1 != 0 {
        return false;
    }
    // The gap between the two ranges is the signature placeholder.
    let gap_start = off1 + len1;
    if gap_start != off2 - (off2 - gap_start) {
        // off2 should be > gap_start; the gap is the /Contents hex string.
    }
    // Range 2 must end exactly at EOF.
    let end = off2 + len2;
    end as usize == file_len
}

// ── PKCS#7 / CMS parsing (minimal, no external crate) ───────────────────

/// Minimal ASN.1 DER parser — just enough to walk a PKCS#7 SignedData and
/// pull the first signer's certificate subject CN and signing time. This
/// avoids adding heavy crypto crates for a display-only feature.

fn parse_pkcs7_signer(data: &[u8]) -> (Option<String>, Option<String>) {
    // The PKCS#7 ContentInfo is a SEQUENCE { contentType OID, content [0] }.
    // content wraps SignedData { version, digestAlgorithms, encapContentInfo,
    //   certificates [0] IMPLICIT, crls [1] IMPLICIT, signerInfos }.
    // We want certificates[0] for the subject CN, and signerInfos for the
    // signing time attribute.

    let mut name: Option<String> = None;
    let mut time: Option<String> = None;

    // Parse outer ContentInfo SEQUENCE.
    let content_info = match der_sequence(data) {
        Some(inner) => inner,
        None => return (name, time),
    };

    // Skip the contentType OID, find the [0] EXPLICIT wrapper.
    let signed_data_wrapper = match der_find_context(content_info, 0) {
        Some(w) => w,
        None => return (name, time),
    };

    // Inside the [0] wrapper is the SignedData SEQUENCE.
    let signed_data = match der_sequence(signed_data_wrapper) {
        Some(inner) => inner,
        None => return (name, time),
    };

    // Walk SignedData fields:
    // 0: version (INTEGER)
    // 1: digestAlgorithms (SET)
    // 2: encapContentInfo (SEQUENCE)
    // 3: certificates [0] IMPLICIT (optional)
    // 4: crls [1] IMPLICIT (optional)
    // 5: signerInfos (SET)
    let fields = der_collect_tlvs(signed_data);

    // Find certificates — context tag [0], constructed.
    for (tag, _, content) in &fields {
        if *tag == 0xA0 {
            // This is the certificates SET. Each element is a Certificate SEQUENCE.
            let certs = der_collect_tlvs(content);
            if let Some((_, _, cert_bytes)) = certs.first() {
                name = extract_subject_cn(cert_bytes);
            }
            break;
        }
    }

    // Find signerInfos — the last SET OF in SignedData.
    for (tag, _, content) in fields.iter().rev() {
        if *tag == 0x31 {
            // SET OF SignerInfo; take the first.
            let signer_infos = der_collect_tlvs(content);
            if let Some((_, _, si_bytes)) = signer_infos.first() {
                time = extract_signing_time(si_bytes);
            }
            break;
        }
    }

    (name, time)
}

/// Extract the Common Name (CN) from a certificate's Subject field.
fn extract_subject_cn(cert_seq_content: &[u8]) -> Option<String> {
    // Certificate ::= SEQUENCE { tbsCertificate, signatureAlgorithm, signature }
    let tbs = match der_sequence(cert_seq_content) {
        Some(inner) => inner,
        None => return None,
    };

    let tbs_fields = der_collect_tlvs(tbs);

    // tbsCertificate fields (simplified):
    // [0] version (optional), serialNumber, signature, issuer, validity, subject, ...
    // Subject is a SEQUENCE of SETs of SEQUENCE { OID, value }.
    // We need field index ~5 (after optional version).
    let has_explicit_version = tbs_fields
        .first()
        .map(|(tag, _, _)| *tag == 0xA0)
        .unwrap_or(false);
    let subject_idx = if has_explicit_version { 5 } else { 4 };

    let (_, _, subject_bytes) = tbs_fields.get(subject_idx)?;

    // Walk the RDN sets looking for CN OID = 2.5.4.3.
    let cn_oid: &[u8] = &[0x55, 0x04, 0x03]; // 2.5.4.3
    let rdn_sets = der_collect_tlvs(subject_bytes);
    for (_, _, set_content) in &rdn_sets {
        let attrs = der_collect_tlvs(set_content);
        for (_, _, attr_content) in &attrs {
            let attr_fields = der_collect_tlvs(attr_content);
            if attr_fields.len() >= 2 {
                let (oid_tag, _, oid_bytes) = &attr_fields[0];
                if *oid_tag == 0x06 && &oid_bytes[..] == cn_oid {
                    let (val_tag, _, val_bytes) = &attr_fields[1];
                    // Value is usually UTF8String (0x0C), PrintableString (0x13),
                    // or IA5String (0x16).
                    if *val_tag == 0x0C || *val_tag == 0x13 || *val_tag == 0x16 {
                        return Some(String::from_utf8_lossy(val_bytes).to_string());
                    }
                    // BMPString (0x1E) — UTF-16BE.
                    if *val_tag == 0x1E && val_bytes.len() % 2 == 0 {
                        let chars: Vec<u16> = val_bytes
                            .chunks_exact(2)
                            .map(|c| u16::from_be_bytes([c[0], c[1]]))
                            .collect();
                        return Some(String::from_utf16_lossy(&chars));
                    }
                }
            }
        }
    }

    None
}

/// Extract the signing time from a SignerInfo's signed attributes.
fn extract_signing_time(signer_info_content: &[u8]) -> Option<String> {
    // SignerInfo ::= SEQUENCE { version, sid, digestAlgorithm,
    //   signedAttrs [0] IMPLICIT, ..., signature, unsignedAttrs [1] }
    let fields = der_collect_tlvs(signer_info_content);

    // Find signed attributes — context tag [0], constructed.
    for (tag, _, content) in &fields {
        if *tag == 0xA0 {
            // SET OF Attribute { attrType OID, attrValues SET OF }
            let attrs = der_collect_tlvs(content);
            for (_, _, attr_content) in &attrs {
                let attr_fields = der_collect_tlvs(attr_content);
                if attr_fields.len() >= 2 {
                    let (oid_tag, _, oid_bytes) = &attr_fields[0];
                    // signingTime OID = 1.2.840.113549.1.9.5
                    let signing_time_oid: &[u8] =
                        &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x09, 0x05];
                    if *oid_tag == 0x06 && &oid_bytes[..] == signing_time_oid {
                        // attrValues is a SET containing one UTCTime or GeneralizedTime.
                        let (_, _, val_set) = &attr_fields[1];
                        let vals = der_collect_tlvs(val_set);
                        if let Some((time_tag, _, time_bytes)) = vals.first() {
                            if *time_tag == 0x17 || *time_tag == 0x18 {
                                return Some(parse_asn1_time(*time_tag, time_bytes));
                            }
                        }
                    }
                }
            }
            break;
        }
    }

    None
}

// ── Minimal DER helpers ──────────────────────────────────────────────────

/// Read a DER tag-length-value at the start of `data`. Returns
/// (tag, header_len, content_slice, total_consumed).
fn der_read_tlv(data: &[u8]) -> Option<(u8, usize, &[u8], usize)> {
    if data.is_empty() {
        return None;
    }
    let tag = data[0];
    let (length, hdr_rest) = der_read_length(&data[1..])?;
    let hdr_len = 1 + (data.len() - data[1..].len() - hdr_rest.len());
    // Ensure we don't exceed the remaining data.
    let total = hdr_len + length;
    if total > data.len() {
        return None;
    }
    Some((tag, hdr_len, &data[hdr_len..hdr_len + length], total))
}

/// Read a DER length. Returns (length, remaining_data).
fn der_read_length(data: &[u8]) -> Option<(usize, &[u8])> {
    if data.is_empty() {
        return None;
    }
    let first = data[0];
    if first < 0x80 {
        Some((first as usize, &data[1..]))
    } else if first == 0x80 {
        // Indefinite length — not valid DER but tolerate.
        None
    } else {
        let num_bytes = (first & 0x7F) as usize;
        if num_bytes > 4 || num_bytes > data.len() - 1 {
            return None;
        }
        let mut length: usize = 0;
        for i in 0..num_bytes {
            length = (length << 8) | (data[1 + i] as usize);
        }
        Some((length, &data[1 + num_bytes..]))
    }
}

/// If `data` starts with a SEQUENCE, return its content bytes.
fn der_sequence(data: &[u8]) -> Option<&[u8]> {
    let (tag, _, content, _) = der_read_tlv(data)?;
    if tag == 0x30 {
        Some(content)
    } else {
        None
    }
}

/// Find the first context-tagged [n] EXPLICIT element in `data`.
fn der_find_context(data: &[u8], ctx: u8) -> Option<&[u8]> {
    let target_tag = 0xA0 | ctx;
    let mut pos = data;
    while !pos.is_empty() {
        let (tag, _, content, consumed) = der_read_tlv(pos)?;
        if tag == target_tag {
            return Some(content);
        }
        pos = &pos[consumed..];
    }
    None
}

/// Collect all top-level TLVs in `data` as (tag, header_len, content).
fn der_collect_tlvs(data: &[u8]) -> Vec<(u8, usize, &[u8])> {
    let mut out = Vec::new();
    let mut pos = data;
    while !pos.is_empty() {
        match der_read_tlv(pos) {
            Some((tag, hdr_len, content, consumed)) => {
                out.push((tag, hdr_len, content));
                pos = &pos[consumed..];
            }
            None => break,
        }
    }
    out
}

// ── PDF helpers ──────────────────────────────────────────────────────────

/// Dereference an indirect reference, returning the underlying object.
fn resolve_obj<'a>(doc: &'a Document, obj: &'a Object) -> &'a Object {
    match obj {
        Object::Reference(id) => doc.objects.get(id).unwrap_or(obj),
        _ => obj,
    }
}

/// Get a dictionary from an object, resolving indirect refs.
fn resolve_dict(doc: &Document, obj: &Object) -> Vec<(Vec<u8>, Object)> {
    let resolved = resolve_obj(doc, obj);
    match resolved.as_dict() {
        Ok(d) => d.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        Err(_) => Vec::new(),
    }
}

/// Resolve an object ref to a dictionary.
fn resolve_dict_ref<'a>(doc: &'a Document, obj: &'a Object) -> Option<&'a lopdf::Dictionary> {
    resolve_obj(doc, obj).as_dict().ok()
}

/// Extract a string from a PDF String or Name object.
fn pdf_string(obj: &Object) -> Option<String> {
    match obj {
        Object::String(bytes, _) => {
            // Check for UTF-16BE BOM.
            if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
                let chars: Vec<u16> = bytes[2..]
                    .chunks_exact(2)
                    .map(|c| u16::from_be_bytes([c[0], c[1]]))
                    .collect();
                Some(String::from_utf16_lossy(&chars))
            } else {
                Some(String::from_utf8_lossy(bytes).to_string())
            }
        }
        Object::Name(bytes) => Some(String::from_utf8_lossy(bytes).to_string()),
        _ => None,
    }
}

/// Parse a PDF date string (D:YYYYMMDDHHmmSS+HH'mm') into a more
/// human-readable form.
fn parse_pdf_date(raw: &str) -> String {
    let s = raw.strip_prefix("D:").unwrap_or(raw);
    if s.len() >= 14 {
        let yyyy = &s[0..4];
        let mm = &s[4..6];
        let dd = &s[6..8];
        let hh = &s[8..10];
        let mi = &s[10..12];
        let ss = &s[12..14];
        let tz = if s.len() > 14 { &s[14..] } else { "" };
        let tz_clean = tz.replace('\'', ":");
        format!("{yyyy}-{mm}-{dd} {hh}:{mi}:{ss}{tz_clean}")
    } else {
        s.to_string()
    }
}

/// Parse an ASN.1 UTCTime (tag 0x17) or GeneralizedTime (tag 0x18) into a
/// readable date string.
fn parse_asn1_time(tag: u8, bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes);
    if tag == 0x17 {
        // UTCTime: YYMMDDHHmmSSZ
        if s.len() >= 12 {
            let yy: u32 = s[0..2].parse().unwrap_or(0);
            let year = if yy >= 50 { 1900 + yy } else { 2000 + yy };
            format!(
                "{}-{}-{} {}:{}:{}Z",
                year,
                &s[2..4],
                &s[4..6],
                &s[6..8],
                &s[8..10],
                &s[10..12]
            )
        } else {
            s.to_string()
        }
    } else {
        // GeneralizedTime: YYYYMMDDHHmmSSZ
        if s.len() >= 14 {
            format!(
                "{}-{}-{} {}:{}:{}Z",
                &s[0..4],
                &s[4..6],
                &s[6..8],
                &s[8..10],
                &s[10..12],
                &s[12..14]
            )
        } else {
            s.to_string()
        }
    }
}
