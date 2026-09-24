//! Content-level edits applied with PDFium (the real page-object engine). These
//! mutate actual page content (not annotations), so they require the PDFium
//! binary to be present. Edits are applied to each affected source PDF, written
//! to a temp file, and that temp becomes the effective source for the lopdf
//! assemble step in `annotate::save`.

use crate::engine;
use pdfium_render::prelude::*;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// A single content edit, tagged by `kind`. Coordinates are already in PDF user
/// space (origin bottom-left), converted on the frontend.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ContentEdit {
    /// Add a new text object to a page.
    #[serde(rename_all = "camelCase")]
    Text {
        source: usize,
        src_page: u32,
        x: f32,
        y: f32,
        size: f32,
        color: String,
        text: String,
    },
    /// Replace the string of an existing text object near (x, y).
    #[serde(rename_all = "camelCase")]
    EditText {
        source: usize,
        src_page: u32,
        x: f32,
        y: f32,
        orig_text: String,
        text: String,
    },
}

impl ContentEdit {
    fn source(&self) -> usize {
        match self {
            ContentEdit::Text { source, .. } | ContentEdit::EditText { source, .. } => *source,
        }
    }
}

/// Apply content edits to the given sources. Returns the effective source paths
/// (temp files for edited sources, originals otherwise). A no-op when there are
/// no edits, so the PDFium binary is only needed when actually content-editing.
pub fn apply(
    sources: &[String],
    edits: &[ContentEdit],
    source_password: Option<&str>,
) -> Result<Vec<String>, String> {
    if edits.is_empty() {
        return Ok(sources.to_vec());
    }

    let pdfium = engine::instance()?;
    let mut by_source: BTreeMap<usize, Vec<&ContentEdit>> = BTreeMap::new();
    for e in edits {
        by_source.entry(e.source()).or_default().push(e);
    }

    let mut effective = sources.to_vec();
    let tmp_dir = std::env::temp_dir();

    for (si, group) in by_source {
        let src = sources.get(si).ok_or("Invalid edit source index")?;
        let mut doc = pdfium
            .load_pdf_from_file(src, source_password)
            .map_err(|e| format!("PDFium open failed: {e:?}"))?;
        let font = doc.fonts_mut().helvetica();

        for edit in group {
            match edit {
                ContentEdit::Text {
                    src_page,
                    x,
                    y,
                    size,
                    color,
                    text,
                    ..
                } => {
                    let mut page = doc
                        .pages()
                        .get((*src_page - 1) as u16)
                        .map_err(|e| format!("PDFium page {src_page}: {e:?}"))?;
                    let mut object =
                        PdfPageTextObject::new(&doc, text.clone(), font, PdfPoints::new(*size))
                            .map_err(|e| format!("PDFium text object: {e:?}"))?;
                    let (r, g, b) = parse_color(color);
                    object
                        .set_fill_color(PdfColor::new(r, g, b, 255))
                        .map_err(|e| format!("PDFium color: {e:?}"))?;
                    object
                        .translate(PdfPoints::new(*x), PdfPoints::new(*y))
                        .map_err(|e| format!("PDFium translate: {e:?}"))?;
                    page.objects_mut()
                        .add_text_object(object)
                        .map_err(|e| format!("PDFium add text: {e:?}"))?;
                }

                ContentEdit::EditText {
                    src_page,
                    x,
                    y,
                    orig_text,
                    text,
                    ..
                } => {
                    let mut page = doc
                        .pages()
                        .get((*src_page - 1) as u16)
                        .map_err(|e| format!("PDFium page {src_page}: {e:?}"))?;
                    let (px, py) = (*x, *y);

                    // Find the text object near the click, preferring an exact text match.
                    let count = page.objects().len();
                    let mut target: Option<usize> = None;
                    let mut fallback: Option<usize> = None;
                    for i in 0..count {
                        if let Ok(obj) = page.objects().get(i) {
                            if let Some(t) = obj.as_text_object() {
                                let contains = t
                                    .bounds()
                                    .ok()
                                    .map(|r| {
                                        px >= r.left().value
                                            && px <= r.right().value
                                            && py >= r.bottom().value
                                            && py <= r.top().value
                                    })
                                    .unwrap_or(false);
                                if contains {
                                    if fallback.is_none() {
                                        fallback = Some(i);
                                    }
                                    if t.text().trim() == orig_text.trim() {
                                        target = Some(i);
                                        break;
                                    }
                                }
                            }
                        }
                    }

                    if let Some(i) = target.or(fallback) {
                        if let Ok(PdfPageObject::Text(ref mut t)) = page.objects().get(i) {
                            t.set_text(text)
                                .map_err(|e| format!("PDFium set_text: {e:?}"))?;
                        }
                        // set_text doesn't auto-commit; regenerate the page content.
                        page.regenerate_content()
                            .map_err(|e| format!("PDFium regenerate: {e:?}"))?;
                    }
                }
            }
        }

        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let out = tmp_dir.join(format!("limitlesspdf-edit-{si}-{nanos}.pdf"));
        doc.save_to_file(&out)
            .map_err(|e| format!("PDFium save failed: {e:?}"))?;
        effective[si] = out.to_string_lossy().into_owned();
    }

    Ok(effective)
}

fn parse_color(hex: &str) -> (u8, u8, u8) {
    let h = hex.trim_start_matches('#');
    let p = |i: usize| u8::from_str_radix(h.get(i..i + 2).unwrap_or("00"), 16).unwrap_or(0);
    if h.len() >= 6 {
        (p(0), p(2), p(4))
    } else {
        (20, 20, 24)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_color_with_hash() {
        assert_eq!(parse_color("#ff8000"), (255, 128, 0));
        assert_eq!(parse_color("#000000"), (0, 0, 0));
        assert_eq!(parse_color("#ffffff"), (255, 255, 255));
    }

    #[test]
    fn parse_color_without_hash() {
        assert_eq!(parse_color("00ff00"), (0, 255, 0));
    }

    #[test]
    fn parse_color_short_or_invalid_fallback() {
        assert_eq!(parse_color("#abc"), (20, 20, 24));
        assert_eq!(parse_color(""), (20, 20, 24));
    }

    #[test]
    fn content_edit_text_serde_and_source() {
        let json = r##"{
            "kind": "text",
            "source": 3,
            "srcPage": 2,
            "x": 100.5,
            "y": 200.5,
            "size": 14.0,
            "color": "#112233",
            "text": "Sample text"
        }"##;
        let edit: ContentEdit = serde_json::from_str(json).unwrap();
        assert_eq!(edit.source(), 3);
        match edit {
            ContentEdit::Text {
                source,
                src_page,
                x,
                y,
                size,
                color,
                text,
            } => {
                assert_eq!(source, 3);
                assert_eq!(src_page, 2);
                assert_eq!(x, 100.5);
                assert_eq!(y, 200.5);
                assert_eq!(size, 14.0);
                assert_eq!(color, "#112233");
                assert_eq!(text, "Sample text");
            }
            _ => panic!("Expected ContentEdit::Text"),
        }
    }

    #[test]
    fn content_edit_edit_text_serde_and_source() {
        let json = r##"{
            "kind": "editText",
            "source": 1,
            "srcPage": 5,
            "x": 50.0,
            "y": 75.0,
            "origText": "Old String",
            "text": "New String"
        }"##;
        let edit: ContentEdit = serde_json::from_str(json).unwrap();
        assert_eq!(edit.source(), 1);
        match edit {
            ContentEdit::EditText {
                source,
                src_page,
                x,
                y,
                orig_text,
                text,
            } => {
                assert_eq!(source, 1);
                assert_eq!(src_page, 5);
                assert_eq!(x, 50.0);
                assert_eq!(y, 75.0);
                assert_eq!(orig_text, "Old String");
                assert_eq!(text, "New String");
            }
            _ => panic!("Expected ContentEdit::EditText"),
        }
    }

    #[test]
    fn apply_empty_edits_returns_sources_unchanged() {
        let sources = vec!["doc1.pdf".to_string(), "doc2.pdf".to_string()];
        let res = apply(&sources, &[], None).unwrap();
        assert_eq!(res, sources);
    }
}
