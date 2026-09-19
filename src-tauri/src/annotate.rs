//! Persist annotations into a PDF as standard annotation objects (Highlight,
//! Ink, Text) using pure-Rust `lopdf`. Each markup annotation also gets an
//! appearance stream (/AP) so it renders consistently across viewers.
//!
//! Coordinates arrive already converted to PDF user space (origin bottom-left)
//! by the frontend via pdf.js `viewport.convertToPdfPoint`, so this module does
//! no coordinate math beyond normalizing rectangles.

use lopdf::{Dictionary, Document, Object, ObjectId, Stream, StringFormat};
use serde::Deserialize;
use std::collections::BTreeMap;

/// One output page: which source (index into `sources`), which page in it,
/// plus a rotation delta.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanEntry {
    pub source: usize,
    pub src_page: u32,
    pub rotation: i32,
}

#[derive(Deserialize)]
pub struct RectPdf {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SaveAnnotation {
    Highlight {
        out_index: u32,
        color: String,
        rect: RectPdf,
    },
    Draw {
        out_index: u32,
        color: String,
        width: f32,
        /// One entry per stroke; each is a flat [x, y, x, y, ...] list.
        paths: Vec<Vec<f32>>,
    },
    Note {
        out_index: u32,
        color: String,
        x: f32,
        y: f32,
        text: String,
    },
}

impl SaveAnnotation {
    /// 0-based index of the output page this annotation belongs to.
    fn out_index(&self) -> usize {
        (match self {
            SaveAnnotation::Highlight { out_index, .. }
            | SaveAnnotation::Draw { out_index, .. }
            | SaveAnnotation::Note { out_index, .. } => *out_index,
        }) as usize
    }
}

/// Merge pages from one or more `sources` into a single PDF, in the order given
/// by `plan` (each entry names a source + page + rotation), write `annos` onto
/// the output pages (referenced by output index), and save to `dest`.
///
/// With a single source this is just reorder/rotate/delete; with several it
/// merges/inserts pages across documents.
#[allow(clippy::too_many_arguments)]
pub fn save(
    sources: Vec<String>,
    dest: &str,
    plan: Vec<PlanEntry>,
    annos: Vec<SaveAnnotation>,
    content_edits: Vec<crate::content::ContentEdit>,
    form_values: Vec<crate::form::FormValue>,
    form_mode: String,
    password: Option<String>,
    source_password: Option<String>,
) -> Result<(), String> {
    // 0. Apply PDFium content edits per source, swapping in edited temp files.
    let sources = crate::content::apply(&sources, &content_edits, source_password.as_deref())?;
    let editable_form = form_mode == "editable";

    // 1. Load every source and merge their objects into one document, giving
    //    each source a disjoint object-id range (lopdf's merge recipe).
    let mut merged = Document::with_version("1.7");
    let mut page_map: BTreeMap<(usize, u32), ObjectId> = BTreeMap::new();
    let mut max_id: u32 = 1;
    // AcroForm from the primary source, captured (renumbered) for re-attachment.
    let mut acroform_id: Option<ObjectId> = None;

    for (si, path) in sources.iter().enumerate() {
        let mut doc = Document::load(path).map_err(|e| format!("Couldn't open \"{path}\": {e}"))?;
        // Decrypt an encrypted source so its content is usable downstream.
        if doc.is_encrypted() {
            let pw = source_password.as_deref().unwrap_or("");
            doc.decrypt(pw)
                .map_err(|e| format!("Couldn't decrypt source PDF: {e}"))?;
        }
        // Make each page self-contained before it leaves its tree.
        for pid in doc.get_pages().values().copied().collect::<Vec<_>>() {
            resolve_inherited(&mut doc, pid);
        }
        doc.renumber_objects_with(max_id);
        max_id = doc.max_id + 1;
        if si == 0 {
            acroform_id = crate::form::acroform_of(&doc);
        }
        for (pnum, pid) in doc.get_pages() {
            page_map.insert((si, pnum), pid);
        }
        merged.objects.extend(doc.objects);
        if merged.max_id < doc.max_id {
            merged.max_id = doc.max_id;
        }
    }

    // 2. Build a fresh, flat page tree in the plan order.
    let pages_root = merged.add_object(Object::Dictionary(Dictionary::new()));
    let mut kids: Vec<Object> = Vec::new();
    let mut out_pages: Vec<ObjectId> = Vec::new();
    for entry in &plan {
        let Some(pid) = page_map.get(&(entry.source, entry.src_page)).copied() else {
            continue;
        };
        if let Ok(d) = merged.get_object_mut(pid).and_then(|o| o.as_dict_mut()) {
            d.set("Parent", Object::Reference(pages_root));
            if entry.rotation != 0 {
                let eff = d
                    .get(b"Rotate")
                    .ok()
                    .and_then(|o| o.as_i64().ok())
                    .unwrap_or(0) as i32;
                let rot = (((eff + entry.rotation) % 360) + 360) % 360;
                d.set("Rotate", Object::Integer(rot as i64));
            }
        }
        kids.push(Object::Reference(pid));
        out_pages.push(pid);
    }

    let count = kids.len() as i64;
    if let Ok(root) = merged
        .get_object_mut(pages_root)
        .and_then(|o| o.as_dict_mut())
    {
        root.set("Type", Object::Name(b"Pages".to_vec()));
        root.set("Kids", Object::Array(kids));
        root.set("Count", Object::Integer(count));
    }

    // 3. New catalog + trailer Root. Re-attach the AcroForm when keeping fields
    //    editable, so the merged document is still a valid interactive form.
    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_root));
    if editable_form {
        if let Some(af) = acroform_id {
            catalog.set("AcroForm", Object::Reference(af));
        }
    }
    let catalog_id = merged.add_object(Object::Dictionary(catalog));
    merged.trailer.set("Root", Object::Reference(catalog_id));

    // 4. Annotations, addressed by output-page index.
    let mut per_page: BTreeMap<ObjectId, Vec<ObjectId>> = BTreeMap::new();
    for a in &annos {
        let Some(pid) = out_pages.get(a.out_index()).copied() else {
            continue;
        };
        let id = build_annotation(&mut merged, a)?;
        per_page.entry(pid).or_default().push(id);
    }
    for (pid, ids) in per_page {
        attach_annots(&mut merged, pid, &ids)?;
    }

    // 4b. Form handling. Editable: write values into the live AcroForm. Flatten:
    //     values were already stamped via the content engine, so drop the now-
    //     redundant widget annotations.
    if editable_form {
        if let Some(af) = acroform_id {
            crate::form::apply_values(&mut merged, af, &form_values);
        }
    } else {
        crate::form::strip_widgets(&mut merged, &out_pages);
    }

    // 5. Drop unreferenced (unused source) objects and compact ids.
    merged.prune_objects();
    merged.renumber_objects();

    // 6. Optional AES-256 encryption. Must run last: per-object keys depend on
    //    the final object ids, and the /Encrypt dict must not itself be encrypted.
    if let Some(pw) = password.as_deref() {
        if !pw.is_empty() {
            encrypt_document(&mut merged, pw)?;
        }
    }

    merged
        .save(dest)
        .map_err(|e| format!("Couldn't save PDF: {e}"))?;
    Ok(())
}

/// Encrypt the assembled document with AES-128 (PDF 1.6 standard security
/// handler, V4/R4), using `password` for both opening and ownership. Permissions
/// are left fully permissive — the password just gates opening the file.
///
/// AES-128 (rather than AES-256 / V5-R6) is deliberate: the R4 handler is
/// supported by essentially every PDF reader, whereas AES-256 is routinely
/// rejected by older/strict viewers. R4 also derives the key from the password
/// and the file /ID internally, so it must run after /ID is set.
fn encrypt_document(doc: &mut Document, password: &str) -> Result<(), String> {
    use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
    use lopdf::encryption::{EncryptionState, EncryptionVersion, Permissions};
    use std::sync::Arc;

    // The R4 key derivation reads the file /ID, so it must exist first.
    ensure_file_id(doc);

    let mut crypt_filters: BTreeMap<Vec<u8>, Arc<dyn CryptFilter>> = BTreeMap::new();
    crypt_filters.insert(b"StdCF".to_vec(), Arc::new(Aes128CryptFilter));

    // The version borrows the document immutably to compute keys; that borrow
    // ends when the owned state is produced, before we mutate via encrypt().
    let state = {
        let version = EncryptionVersion::V4 {
            document: doc,
            encrypt_metadata: true,
            crypt_filters,
            stream_filter: b"StdCF".to_vec(),
            string_filter: b"StdCF".to_vec(),
            owner_password: password,
            user_password: password,
            permissions: Permissions::all(),
        };
        EncryptionState::try_from(version).map_err(|e| format!("Encryption setup failed: {e}"))?
    };
    doc.encrypt(&state)
        .map_err(|e| format!("Encryption failed: {e}"))?;
    Ok(())
}

/// Ensure the trailer has a file /ID (two 16-byte strings), required by readers
/// when an /Encrypt dictionary is present.
fn ensure_file_id(doc: &mut Document) {
    if doc.trailer.get(b"ID").is_ok() {
        return;
    }
    let mut a = [0u8; 16];
    let mut b = [0u8; 16];
    let _ = getrandom::fill(&mut a);
    let _ = getrandom::fill(&mut b);
    doc.trailer.set(
        "ID",
        Object::Array(vec![
            Object::String(a.to_vec(), StringFormat::Hexadecimal),
            Object::String(b.to_vec(), StringFormat::Hexadecimal),
        ]),
    );
}

/// Copy inherited page attributes (MediaBox, CropBox, Resources, Rotate) from
/// ancestor Pages nodes onto the page itself, so it survives tree flattening.
fn resolve_inherited(doc: &mut Document, page_id: ObjectId) {
    const KEYS: [&[u8]; 4] = [b"MediaBox", b"CropBox", b"Resources", b"Rotate"];
    for key in KEYS {
        let present = doc
            .get_dictionary(page_id)
            .map(|d| d.get(key).is_ok())
            .unwrap_or(false);
        if present {
            continue;
        }
        // Walk up the /Parent chain looking for the attribute.
        let mut cursor = doc
            .get_dictionary(page_id)
            .ok()
            .and_then(|d| d.get(b"Parent").ok().cloned())
            .and_then(|o| o.as_reference().ok());
        let mut found: Option<Object> = None;
        while let Some(pid) = cursor {
            match doc.get_dictionary(pid) {
                Ok(pd) => {
                    if let Ok(v) = pd.get(key) {
                        found = Some(v.clone());
                        break;
                    }
                    cursor = pd
                        .get(b"Parent")
                        .ok()
                        .cloned()
                        .and_then(|o| o.as_reference().ok());
                }
                Err(_) => break,
            }
        }
        if let Some(v) = found {
            if let Ok(d) = doc.get_object_mut(page_id).and_then(|o| o.as_dict_mut()) {
                d.set(key.to_vec(), v);
            }
        }
    }
}

fn build_annotation(doc: &mut Document, a: &SaveAnnotation) -> Result<ObjectId, String> {
    match a {
        SaveAnnotation::Highlight { color, rect, .. } => {
            let (r, g, b) = parse_color(color);
            let (x0, y0, x1, y1) = normalize(rect.x0, rect.y0, rect.x1, rect.y1);
            let (w, h) = (x1 - x0, y1 - y0);

            // Appearance: translucent, multiply-blended filled rect.
            let content =
                format!("/GS gs\n{r:.4} {g:.4} {b:.4} rg\n{x0:.2} {y0:.2} {w:.2} {h:.2} re f\n");
            let mut gs = Dictionary::new();
            gs.set("ca", Object::Real(0.4));
            gs.set("BM", Object::Name(b"Multiply".to_vec()));
            let mut egs = Dictionary::new();
            egs.set("GS", Object::Dictionary(gs));
            let mut res = Dictionary::new();
            res.set("ExtGState", Object::Dictionary(egs));

            let mut form = Dictionary::new();
            form.set("Type", Object::Name(b"XObject".to_vec()));
            form.set("Subtype", Object::Name(b"Form".to_vec()));
            form.set("BBox", arr4(x0, y0, x1, y1));
            form.set("Resources", Object::Dictionary(res));
            let form_id = doc.add_object(Object::Stream(Stream::new(form, content.into_bytes())));

            let mut d = annot_base("Highlight", x0, y0, x1, y1, r, g, b);
            d.set(
                "QuadPoints",
                Object::Array(vec![
                    x0.into(),
                    y1.into(),
                    x1.into(),
                    y1.into(),
                    x0.into(),
                    y0.into(),
                    x1.into(),
                    y0.into(),
                ]),
            );
            d.set("AP", ap_dict(form_id));
            Ok(doc.add_object(Object::Dictionary(d)))
        }

        SaveAnnotation::Draw {
            color,
            width,
            paths,
            ..
        } => {
            let (r, g, b) = parse_color(color);
            let (mut minx, mut miny, mut maxx, mut maxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for p in paths {
                let mut i = 0;
                while i + 1 < p.len() {
                    minx = minx.min(p[i]);
                    maxx = maxx.max(p[i]);
                    miny = miny.min(p[i + 1]);
                    maxy = maxy.max(p[i + 1]);
                    i += 2;
                }
            }
            if !minx.is_finite() {
                return Err("Empty ink annotation".into());
            }
            let pad = width + 2.0;
            minx -= pad;
            miny -= pad;
            maxx += pad;
            maxy += pad;

            let mut s = format!("{width:.2} w\n{r:.4} {g:.4} {b:.4} RG\n1 J 1 j\n");
            let mut inklist: Vec<Object> = Vec::new();
            for p in paths {
                let mut nums: Vec<Object> = Vec::new();
                let mut i = 0;
                let mut first = true;
                while i + 1 < p.len() {
                    let (x, y) = (p[i], p[i + 1]);
                    s.push_str(&format!(
                        "{x:.2} {y:.2} {}\n",
                        if first { "m" } else { "l" }
                    ));
                    first = false;
                    nums.push(x.into());
                    nums.push(y.into());
                    i += 2;
                }
                inklist.push(Object::Array(nums));
            }
            s.push_str("S\n");

            let mut form = Dictionary::new();
            form.set("Type", Object::Name(b"XObject".to_vec()));
            form.set("Subtype", Object::Name(b"Form".to_vec()));
            form.set("BBox", arr4(minx, miny, maxx, maxy));
            let form_id = doc.add_object(Object::Stream(Stream::new(form, s.into_bytes())));

            let mut d = annot_base("Ink", minx, miny, maxx, maxy, r, g, b);
            d.set("InkList", Object::Array(inklist));
            let mut bs = Dictionary::new();
            bs.set("W", Object::Real(*width));
            d.set("BS", Object::Dictionary(bs));
            d.set("AP", ap_dict(form_id));
            Ok(doc.add_object(Object::Dictionary(d)))
        }

        SaveAnnotation::Note {
            color, x, y, text, ..
        } => {
            let (r, g, b) = parse_color(color);
            let size = 18.0_f32;
            let mut d = annot_base("Text", *x, *y - size, *x + size, *y, r, g, b);
            d.set("Contents", pdf_string(text));
            d.set("Name", Object::Name(b"Comment".to_vec()));
            d.set("Open", Object::Boolean(false));
            Ok(doc.add_object(Object::Dictionary(d)))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn annot_base(
    subtype: &str,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    r: f32,
    g: f32,
    b: f32,
) -> Dictionary {
    let mut d = Dictionary::new();
    d.set("Type", Object::Name(b"Annot".to_vec()));
    d.set("Subtype", Object::Name(subtype.as_bytes().to_vec()));
    d.set("Rect", arr4(x0, y0, x1, y1));
    d.set("C", Object::Array(vec![r.into(), g.into(), b.into()]));
    d.set("F", Object::Integer(4)); // Print flag
    d
}

fn ap_dict(form_id: ObjectId) -> Object {
    let mut ap = Dictionary::new();
    ap.set("N", Object::Reference(form_id));
    Object::Dictionary(ap)
}

fn attach_annots(doc: &mut Document, page_id: ObjectId, ids: &[ObjectId]) -> Result<(), String> {
    // If /Annots is an indirect reference to an array, mutate that array;
    // otherwise update (or create) the inline array on the page dict.
    let annots_ref = {
        let pd = doc.get_dictionary(page_id).map_err(|e| e.to_string())?;
        match pd.get(b"Annots") {
            Ok(Object::Reference(r)) => Some(*r),
            _ => None,
        }
    };

    if let Some(r) = annots_ref {
        let arr = doc
            .get_object_mut(r)
            .map_err(|e| e.to_string())?
            .as_array_mut()
            .map_err(|e| e.to_string())?;
        for id in ids {
            arr.push(Object::Reference(*id));
        }
    } else {
        let pd = doc
            .get_object_mut(page_id)
            .map_err(|e| e.to_string())?
            .as_dict_mut()
            .map_err(|e| e.to_string())?;
        let mut arr = match pd.get(b"Annots") {
            Ok(Object::Array(a)) => a.clone(),
            _ => Vec::new(),
        };
        for id in ids {
            arr.push(Object::Reference(*id));
        }
        pd.set("Annots", Object::Array(arr));
    }
    Ok(())
}

fn arr4(a: f32, b: f32, c: f32, d: f32) -> Object {
    Object::Array(vec![a.into(), b.into(), c.into(), d.into()])
}

fn normalize(x0: f32, y0: f32, x1: f32, y1: f32) -> (f32, f32, f32, f32) {
    (x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1))
}

fn parse_color(hex: &str) -> (f32, f32, f32) {
    let h = hex.trim_start_matches('#');
    let parse = |i: usize| {
        u8::from_str_radix(h.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0) as f32 / 255.0
    };
    if h.len() >= 6 {
        (parse(0), parse(2), parse(4))
    } else {
        (1.0, 0.85, 0.2)
    }
}

/// Encode a string for a PDF text field. ASCII becomes a literal string;
/// anything else is UTF-16BE with a byte-order mark, as the spec requires.
fn pdf_string(s: &str) -> Object {
    if s.is_ascii() {
        Object::String(s.as_bytes().to_vec(), StringFormat::Literal)
    } else {
        let mut bytes = vec![0xFE, 0xFF];
        for unit in s.encode_utf16() {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
        Object::String(bytes, StringFormat::Literal)
    }
}
