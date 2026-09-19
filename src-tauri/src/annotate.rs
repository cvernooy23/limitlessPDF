//! Persist annotations into a PDF as standard annotation objects (Highlight,
//! Underline, StrikeOut, Ink, Text) using pure-Rust `lopdf`. Each markup annotation also gets an
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
pub struct PointPdf {
    pub x: f32,
    pub y: f32,
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
    Underline {
        out_index: u32,
        color: String,
        rect: RectPdf,
    },
    Strikethrough {
        out_index: u32,
        color: String,
        rect: RectPdf,
    },
    Rect {
        out_index: u32,
        color: String,
        rect: RectPdf,
        #[serde(rename = "borderWidth")]
        border_width: f32,
    },
    Circle {
        out_index: u32,
        color: String,
        rect: RectPdf,
        #[serde(rename = "borderWidth")]
        border_width: f32,
    },
    Arrow {
        out_index: u32,
        color: String,
        start: PointPdf,
        end: PointPdf,
        width: f32,
    },
}

impl SaveAnnotation {
    /// 0-based index of the output page this annotation belongs to.
    fn out_index(&self) -> usize {
        (match self {
            SaveAnnotation::Highlight { out_index, .. }
            | SaveAnnotation::Draw { out_index, .. }
            | SaveAnnotation::Note { out_index, .. }
            | SaveAnnotation::Underline { out_index, .. }
            | SaveAnnotation::Strikethrough { out_index, .. }
            | SaveAnnotation::Rect { out_index, .. }
            | SaveAnnotation::Circle { out_index, .. }
            | SaveAnnotation::Arrow { out_index, .. } => *out_index,
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

        SaveAnnotation::Underline { color, rect, .. } => {
            let (r, g, b) = parse_color(color);
            let (x0, y0, x1, y1) = normalize(rect.x0, rect.y0, rect.x1, rect.y1);

            // Appearance: a line along the bottom edge of the rect.
            let content =
                format!("{r:.4} {g:.4} {b:.4} RG\n1 w\n{x0:.2} {y0:.2} m {x1:.2} {y0:.2} l S\n");
            let mut form = Dictionary::new();
            form.set("Type", Object::Name(b"XObject".to_vec()));
            form.set("Subtype", Object::Name(b"Form".to_vec()));
            form.set("BBox", arr4(x0, y0 - 1.0, x1, y0 + 1.0));
            let form_id = doc.add_object(Object::Stream(Stream::new(form, content.into_bytes())));

            let mut d = annot_base("Underline", x0, y0, x1, y1, r, g, b);
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

        SaveAnnotation::Strikethrough { color, rect, .. } => {
            let (r, g, b) = parse_color(color);
            let (x0, y0, x1, y1) = normalize(rect.x0, rect.y0, rect.x1, rect.y1);
            let mid_y = (y0 + y1) / 2.0;

            // Appearance: a line through the vertical center of the rect.
            let content = format!(
                "{r:.4} {g:.4} {b:.4} RG\n1 w\n{x0:.2} {mid_y:.2} m {x1:.2} {mid_y:.2} l S\n"
            );
            let mut form = Dictionary::new();
            form.set("Type", Object::Name(b"XObject".to_vec()));
            form.set("Subtype", Object::Name(b"Form".to_vec()));
            form.set("BBox", arr4(x0, mid_y - 1.0, x1, mid_y + 1.0));
            let form_id = doc.add_object(Object::Stream(Stream::new(form, content.into_bytes())));

            let mut d = annot_base("StrikeOut", x0, y0, x1, y1, r, g, b);
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

        SaveAnnotation::Rect {
            color,
            rect,
            border_width,
            ..
        } => {
            let (r, g, b) = parse_color(color);
            let (x0, y0, x1, y1) = normalize(rect.x0, rect.y0, rect.x1, rect.y1);
            let (w, h) = (x1 - x0, y1 - y0);
            let bw = *border_width;

            // Appearance: stroked rectangle (no fill).
            let content =
                format!("{r:.4} {g:.4} {b:.4} RG\n{bw:.2} w\n{x0:.2} {y0:.2} {w:.2} {h:.2} re S\n");
            let mut form = Dictionary::new();
            form.set("Type", Object::Name(b"XObject".to_vec()));
            form.set("Subtype", Object::Name(b"Form".to_vec()));
            form.set("BBox", arr4(x0 - bw, y0 - bw, x1 + bw, y1 + bw));
            let form_id = doc.add_object(Object::Stream(Stream::new(form, content.into_bytes())));

            let mut d = annot_base("Square", x0, y0, x1, y1, r, g, b);
            d.set("IC", Object::Array(vec![])); // no interior color
            let mut bs = Dictionary::new();
            bs.set("W", Object::Real(bw));
            bs.set("S", Object::Name(b"S".to_vec()));
            d.set("BS", Object::Dictionary(bs));
            d.set("AP", ap_dict(form_id));
            Ok(doc.add_object(Object::Dictionary(d)))
        }

        SaveAnnotation::Circle {
            color,
            rect,
            border_width,
            ..
        } => {
            let (r, g, b) = parse_color(color);
            let (x0, y0, x1, y1) = normalize(rect.x0, rect.y0, rect.x1, rect.y1);
            let bw = *border_width;
            let cx = (x0 + x1) / 2.0;
            let cy = (y0 + y1) / 2.0;
            let rx = (x1 - x0) / 2.0;
            let ry = (y1 - y0) / 2.0;

            // Approximate ellipse with 4 cubic Bezier curves (kappa ≈ 0.5523).
            let k = 0.5523;
            let kx = rx * k as f32;
            let ky = ry * k as f32;
            let content = format!(
                "{r:.4} {g:.4} {b:.4} RG\n{bw:.2} w\n\
                 {:.2} {cy:.2} m\n\
                 {:.2} {:.2} {:.2} {:.2} {cx:.2} {:.2} c\n\
                 {:.2} {:.2} {:.2} {:.2} {:.2} {cy:.2} c\n\
                 {:.2} {:.2} {:.2} {:.2} {cx:.2} {:.2} c\n\
                 {:.2} {:.2} {:.2} {:.2} {:.2} {cy:.2} c\n\
                 S\n",
                cx + rx,
                cx + rx,
                cy + ky,
                cx + kx,
                cy + ry,
                cy + ry,
                cx - kx,
                cy + ry,
                cx - rx,
                cy + ky,
                cx - rx,
                cx - rx,
                cy - ky,
                cx - kx,
                cy - ry,
                cy - ry,
                cx + kx,
                cy - ry,
                cx + rx,
                cy - ky,
                cx + rx,
            );
            let mut form = Dictionary::new();
            form.set("Type", Object::Name(b"XObject".to_vec()));
            form.set("Subtype", Object::Name(b"Form".to_vec()));
            form.set("BBox", arr4(x0 - bw, y0 - bw, x1 + bw, y1 + bw));
            let form_id = doc.add_object(Object::Stream(Stream::new(form, content.into_bytes())));

            let mut d = annot_base("Circle", x0, y0, x1, y1, r, g, b);
            d.set("IC", Object::Array(vec![]));
            let mut bs = Dictionary::new();
            bs.set("W", Object::Real(bw));
            bs.set("S", Object::Name(b"S".to_vec()));
            d.set("BS", Object::Dictionary(bs));
            d.set("AP", ap_dict(form_id));
            Ok(doc.add_object(Object::Dictionary(d)))
        }

        SaveAnnotation::Arrow {
            color,
            start,
            end,
            width,
            ..
        } => {
            let (r, g, b) = parse_color(color);
            let bw = *width;
            let (sx, sy) = (start.x, start.y);
            let (ex, ey) = (end.x, end.y);
            let (x0, y0, x1, y1) = normalize(sx, sy, ex, ey);
            let pad = bw + 12.0;

            // Appearance: line with arrowhead at the end point.
            let dx = ex - sx;
            let dy = ey - sy;
            let len = (dx * dx + dy * dy).sqrt();
            let mut content = format!(
                "{r:.4} {g:.4} {b:.4} RG\n{r:.4} {g:.4} {b:.4} rg\n{bw:.2} w\n1 J\n\
                 {sx:.2} {sy:.2} m {ex:.2} {ey:.2} l S\n"
            );
            if len > 0.01 {
                let ux = dx / len;
                let uy = dy / len;
                let head_len = (12.0_f32).min(len * 0.4);
                let head_w = head_len * 0.5;
                let bx = ex - ux * head_len;
                let by = ey - uy * head_len;
                content.push_str(&format!(
                    "{:.2} {:.2} m {ex:.2} {ey:.2} l {:.2} {:.2} l f\n",
                    bx - uy * head_w,
                    by + ux * head_w,
                    bx + uy * head_w,
                    by - ux * head_w,
                ));
            }
            let mut form = Dictionary::new();
            form.set("Type", Object::Name(b"XObject".to_vec()));
            form.set("Subtype", Object::Name(b"Form".to_vec()));
            form.set("BBox", arr4(x0 - pad, y0 - pad, x1 + pad, y1 + pad));
            let form_id = doc.add_object(Object::Stream(Stream::new(form, content.into_bytes())));

            let mut d = annot_base("Line", x0 - pad, y0 - pad, x1 + pad, y1 + pad, r, g, b);
            d.set(
                "L",
                Object::Array(vec![sx.into(), sy.into(), ex.into(), ey.into()]),
            );
            d.set(
                "LE",
                Object::Array(vec![
                    Object::Name(b"None".to_vec()),
                    Object::Name(b"OpenArrow".to_vec()),
                ]),
            );
            let mut bs = Dictionary::new();
            bs.set("W", Object::Real(bw));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_color_hex6() {
        let (r, g, b) = parse_color("#ff8000");
        assert!((r - 1.0).abs() < 0.01);
        assert!((g - 0.502).abs() < 0.01);
        assert!((b - 0.0).abs() < 0.01);
    }

    #[test]
    fn parse_color_no_hash() {
        let (r, g, b) = parse_color("ff0000");
        assert!((r - 1.0).abs() < 0.01);
        assert!((g - 0.0).abs() < 0.01);
        assert!((b - 0.0).abs() < 0.01);
    }

    #[test]
    fn parse_color_short_fallback() {
        let (r, g, b) = parse_color("#abc");
        assert!((r - 1.0).abs() < 0.01);
        assert!((g - 0.85).abs() < 0.01);
        assert!((b - 0.2).abs() < 0.01);
    }

    #[test]
    fn parse_color_black() {
        let (r, g, b) = parse_color("#000000");
        assert!(r.abs() < 0.001);
        assert!(g.abs() < 0.001);
        assert!(b.abs() < 0.001);
    }

    #[test]
    fn parse_color_white() {
        let (r, g, b) = parse_color("#ffffff");
        assert!((r - 1.0).abs() < 0.001);
        assert!((g - 1.0).abs() < 0.001);
        assert!((b - 1.0).abs() < 0.001);
    }

    #[test]
    fn normalize_already_ordered() {
        assert_eq!(normalize(10.0, 20.0, 30.0, 40.0), (10.0, 20.0, 30.0, 40.0));
    }

    #[test]
    fn normalize_swapped() {
        assert_eq!(normalize(30.0, 40.0, 10.0, 20.0), (10.0, 20.0, 30.0, 40.0));
    }

    #[test]
    fn normalize_mixed() {
        let (x0, y0, x1, y1) = normalize(50.0, 10.0, 20.0, 80.0);
        assert_eq!((x0, y0, x1, y1), (20.0, 10.0, 50.0, 80.0));
    }

    #[test]
    fn arr4_produces_array_of_reals() {
        let obj = arr4(1.0, 2.0, 3.0, 4.0);
        match obj {
            Object::Array(a) => assert_eq!(a.len(), 4),
            _ => panic!("Expected Array"),
        }
    }

    #[test]
    fn pdf_string_ascii() {
        match pdf_string("Hello") {
            Object::String(bytes, StringFormat::Literal) => {
                assert_eq!(bytes, b"Hello");
            }
            _ => panic!("Expected literal string"),
        }
    }

    #[test]
    fn pdf_string_unicode() {
        match pdf_string("\u{00e9}") {
            Object::String(bytes, StringFormat::Literal) => {
                assert_eq!(bytes[0], 0xFE);
                assert_eq!(bytes[1], 0xFF);
                assert!(bytes.len() > 2);
            }
            _ => panic!("Expected literal string"),
        }
    }

    #[test]
    fn annot_base_sets_type_and_subtype() {
        let d = annot_base("Highlight", 10.0, 20.0, 110.0, 40.0, 1.0, 0.85, 0.2);
        assert_eq!(d.get(b"Type").unwrap().as_name().unwrap(), b"Annot");
        assert_eq!(d.get(b"Subtype").unwrap().as_name().unwrap(), b"Highlight");
    }

    #[test]
    fn annot_base_sets_rect() {
        let d = annot_base("Text", 5.0, 10.0, 25.0, 30.0, 0.0, 0.0, 0.0);
        let rect = d.get(b"Rect").unwrap().as_array().unwrap();
        assert_eq!(rect.len(), 4);
    }

    #[test]
    fn annot_base_sets_print_flag() {
        let d = annot_base("Ink", 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0);
        assert_eq!(d.get(b"F").unwrap().as_i64().unwrap(), 4);
    }

    fn make_doc() -> Document {
        Document::with_version("1.7")
    }

    #[test]
    fn build_highlight_annotation() {
        let mut doc = make_doc();
        let anno = SaveAnnotation::Highlight {
            out_index: 0,
            color: "#ffff00".to_string(),
            rect: RectPdf {
                x0: 72.0,
                y0: 700.0,
                x1: 200.0,
                y1: 720.0,
            },
        };
        let id = build_annotation(&mut doc, &anno).unwrap();
        let d = doc.get_dictionary(id).unwrap();
        assert_eq!(d.get(b"Subtype").unwrap().as_name().unwrap(), b"Highlight");
        assert!(d.get(b"QuadPoints").is_ok());
        assert!(d.get(b"AP").is_ok());
    }

    #[test]
    fn build_underline_annotation() {
        let mut doc = make_doc();
        let anno = SaveAnnotation::Underline {
            out_index: 0,
            color: "#ff0000".to_string(),
            rect: RectPdf {
                x0: 72.0,
                y0: 700.0,
                x1: 200.0,
                y1: 720.0,
            },
        };
        let id = build_annotation(&mut doc, &anno).unwrap();
        let d = doc.get_dictionary(id).unwrap();
        assert_eq!(d.get(b"Subtype").unwrap().as_name().unwrap(), b"Underline");
        assert!(d.get(b"QuadPoints").is_ok());
    }

    #[test]
    fn build_strikethrough_annotation() {
        let mut doc = make_doc();
        let anno = SaveAnnotation::Strikethrough {
            out_index: 0,
            color: "#0000ff".to_string(),
            rect: RectPdf {
                x0: 72.0,
                y0: 700.0,
                x1: 200.0,
                y1: 720.0,
            },
        };
        let id = build_annotation(&mut doc, &anno).unwrap();
        let d = doc.get_dictionary(id).unwrap();
        assert_eq!(d.get(b"Subtype").unwrap().as_name().unwrap(), b"StrikeOut");
        assert!(d.get(b"QuadPoints").is_ok());
    }

    #[test]
    fn build_draw_annotation() {
        let mut doc = make_doc();
        let anno = SaveAnnotation::Draw {
            out_index: 0,
            color: "#000000".to_string(),
            width: 2.0,
            paths: vec![vec![10.0, 20.0, 30.0, 40.0]],
        };
        let id = build_annotation(&mut doc, &anno).unwrap();
        let d = doc.get_dictionary(id).unwrap();
        assert_eq!(d.get(b"Subtype").unwrap().as_name().unwrap(), b"Ink");
        assert!(d.get(b"InkList").is_ok());
        assert!(d.get(b"BS").is_ok());
    }

    #[test]
    fn build_note_annotation() {
        let mut doc = make_doc();
        let anno = SaveAnnotation::Note {
            out_index: 0,
            color: "#ffff00".to_string(),
            x: 100.0,
            y: 500.0,
            text: "A note".to_string(),
        };
        let id = build_annotation(&mut doc, &anno).unwrap();
        let d = doc.get_dictionary(id).unwrap();
        assert_eq!(d.get(b"Subtype").unwrap().as_name().unwrap(), b"Text");
        assert!(d.get(b"Contents").is_ok());
    }

    #[test]
    fn attach_annots_creates_annots_array() {
        let mut doc = make_doc();
        let mut page_dict = Dictionary::new();
        page_dict.set("Type", Object::Name(b"Page".to_vec()));
        let page_id = doc.add_object(Object::Dictionary(page_dict));

        let mut annot_dict = Dictionary::new();
        annot_dict.set("Type", Object::Name(b"Annot".to_vec()));
        let annot_id = doc.add_object(Object::Dictionary(annot_dict));

        attach_annots(&mut doc, page_id, &[annot_id]).unwrap();

        let pd = doc.get_dictionary(page_id).unwrap();
        let annots = pd.get(b"Annots").unwrap().as_array().unwrap();
        assert_eq!(annots.len(), 1);
    }

    #[test]
    fn attach_annots_appends_to_existing() {
        let mut doc = make_doc();
        let mut annot1 = Dictionary::new();
        annot1.set("Type", Object::Name(b"Annot".to_vec()));
        let a1_id = doc.add_object(Object::Dictionary(annot1));

        let mut page_dict = Dictionary::new();
        page_dict.set("Type", Object::Name(b"Page".to_vec()));
        page_dict.set("Annots", Object::Array(vec![Object::Reference(a1_id)]));
        let page_id = doc.add_object(Object::Dictionary(page_dict));

        let mut annot2 = Dictionary::new();
        annot2.set("Type", Object::Name(b"Annot".to_vec()));
        let a2_id = doc.add_object(Object::Dictionary(annot2));

        attach_annots(&mut doc, page_id, &[a2_id]).unwrap();

        let pd = doc.get_dictionary(page_id).unwrap();
        let annots = pd.get(b"Annots").unwrap().as_array().unwrap();
        assert_eq!(annots.len(), 2);
    }

    #[test]
    fn build_rect_annotation() {
        let mut doc = make_doc();
        let anno = SaveAnnotation::Rect {
            out_index: 0,
            color: "#ff0000".to_string(),
            rect: RectPdf {
                x0: 50.0,
                y0: 400.0,
                x1: 200.0,
                y1: 500.0,
            },
            border_width: 2.0,
        };
        let id = build_annotation(&mut doc, &anno).unwrap();
        let d = doc.get_dictionary(id).unwrap();
        assert_eq!(d.get(b"Subtype").unwrap().as_name().unwrap(), b"Square");
        assert!(d.get(b"BS").is_ok());
        assert!(d.get(b"AP").is_ok());
    }

    #[test]
    fn build_circle_annotation() {
        let mut doc = make_doc();
        let anno = SaveAnnotation::Circle {
            out_index: 0,
            color: "#00ff00".to_string(),
            rect: RectPdf {
                x0: 100.0,
                y0: 300.0,
                x1: 250.0,
                y1: 450.0,
            },
            border_width: 1.5,
        };
        let id = build_annotation(&mut doc, &anno).unwrap();
        let d = doc.get_dictionary(id).unwrap();
        assert_eq!(d.get(b"Subtype").unwrap().as_name().unwrap(), b"Circle");
        assert!(d.get(b"BS").is_ok());
        assert!(d.get(b"AP").is_ok());
    }

    #[test]
    fn build_arrow_annotation() {
        let mut doc = make_doc();
        let anno = SaveAnnotation::Arrow {
            out_index: 0,
            color: "#0000ff".to_string(),
            start: PointPdf { x: 100.0, y: 500.0 },
            end: PointPdf { x: 300.0, y: 400.0 },
            width: 2.0,
        };
        let id = build_annotation(&mut doc, &anno).unwrap();
        let d = doc.get_dictionary(id).unwrap();
        assert_eq!(d.get(b"Subtype").unwrap().as_name().unwrap(), b"Line");
        assert!(d.get(b"L").is_ok());
        assert!(d.get(b"LE").is_ok());
        assert!(d.get(b"AP").is_ok());
    }

    #[test]
    fn out_index_returns_correct_value() {
        let anno = SaveAnnotation::Highlight {
            out_index: 5,
            color: "#ff0000".to_string(),
            rect: RectPdf {
                x0: 0.0,
                y0: 0.0,
                x1: 1.0,
                y1: 1.0,
            },
        };
        assert_eq!(anno.out_index(), 5);

        let anno2 = SaveAnnotation::Note {
            out_index: 3,
            color: "#ff0000".to_string(),
            x: 0.0,
            y: 0.0,
            text: "test".to_string(),
        };
        assert_eq!(anno2.out_index(), 3);

        let anno3 = SaveAnnotation::Rect {
            out_index: 7,
            color: "#ff0000".to_string(),
            rect: RectPdf {
                x0: 0.0,
                y0: 0.0,
                x1: 1.0,
                y1: 1.0,
            },
            border_width: 1.0,
        };
        assert_eq!(anno3.out_index(), 7);

        let anno4 = SaveAnnotation::Circle {
            out_index: 2,
            color: "#ff0000".to_string(),
            rect: RectPdf {
                x0: 0.0,
                y0: 0.0,
                x1: 1.0,
                y1: 1.0,
            },
            border_width: 1.0,
        };
        assert_eq!(anno4.out_index(), 2);

        let anno5 = SaveAnnotation::Arrow {
            out_index: 9,
            color: "#ff0000".to_string(),
            start: PointPdf { x: 0.0, y: 0.0 },
            end: PointPdf { x: 1.0, y: 1.0 },
            width: 1.0,
        };
        assert_eq!(anno5.out_index(), 9);
    }
}
