//! OCR pipeline for scanned PDFs. Renders pages to images via PDFium, runs
//! Tesseract (CLI) to extract word positions, and injects an invisible text
//! layer into the PDF via lopdf so the content becomes searchable and
//! selectable.
//!
//! Tesseract is detected at runtime — no build-time dependency. If it isn't
//! installed the frontend shows install instructions.

use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

// ── Public types ──────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OcrStatus {
    pub available: bool,
    pub version: Option<String>,
    pub languages: Vec<String>,
    pub message: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OcrPageResult {
    pub page: u32,
    pub word_count: usize,
    pub text_preview: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrRequest {
    pub src_path: String,
    pub pages: Vec<u32>,
    pub language: Option<String>,
    pub dpi: Option<u32>,
    pub source_password: Option<String>,
}

/// A single word extracted from Tesseract's HOCR output.
#[derive(Debug, Clone)]
struct OcrWord {
    text: String,
    /// Bounding box in image-pixel coordinates (x0, y0, x1, y1).
    bbox: (f32, f32, f32, f32),
    #[allow(dead_code)]
    confidence: f32,
}

// ── Tesseract detection ───────────────────────────────────────────────────

/// Probe for the `tesseract` CLI and return its status.
pub fn check_available() -> OcrStatus {
    let exe = tesseract_exe();
    let version = match Command::new(&exe).arg("--version").output() {
        Ok(out) => {
            let raw = String::from_utf8_lossy(&out.stdout).to_string()
                + &String::from_utf8_lossy(&out.stderr);
            // First line is typically "tesseract 5.x.x"
            raw.lines().next().unwrap_or("").trim().to_string()
        }
        Err(_) => {
            return OcrStatus {
                available: false,
                version: None,
                languages: vec![],
                message: "Tesseract not found. Install it:\n\
                     \u{2022} Windows: winget install UB-Mannheim.TesseractOCR\n\
                     \u{2022} macOS: brew install tesseract\n\
                     \u{2022} Linux: sudo apt install tesseract-ocr"
                    .to_string(),
            };
        }
    };

    let languages = list_languages(&exe);

    OcrStatus {
        available: true,
        version: Some(version.clone()),
        languages: languages.clone(),
        message: format!(
            "{version} \u{2014} {} language{} available",
            languages.len(),
            if languages.len() == 1 { "" } else { "s" }
        ),
    }
}

fn tesseract_exe() -> String {
    // Check common Windows install path if not on PATH.
    if cfg!(target_os = "windows") {
        let candidates = [
            r"C:\Program Files\Tesseract-OCR\tesseract.exe",
            r"C:\Program Files (x86)\Tesseract-OCR\tesseract.exe",
        ];
        for c in &candidates {
            if Path::new(c).exists() {
                return c.to_string();
            }
        }
    }
    "tesseract".to_string()
}

fn list_languages(exe: &str) -> Vec<String> {
    match Command::new(exe).arg("--list-langs").output() {
        Ok(out) => {
            let raw = String::from_utf8_lossy(&out.stdout).to_string()
                + &String::from_utf8_lossy(&out.stderr);
            raw.lines()
                .skip(1) // first line is header
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect()
        }
        Err(_) => vec![],
    }
}

// ── Scanned-page detection ────────────────────────────────────────────────

/// Heuristic: a page is "scanned" (image-only) if pdf.js finds very little
/// or no text on it. This is checked on the frontend — but for the backend
/// we offer a PDFium-based fallback: count text objects on a page.
pub fn detect_scanned_pages(path: &str, password: Option<&str>) -> Result<Vec<u32>, String> {
    let pdfium = crate::engine::instance()?;
    let doc = pdfium
        .load_pdf_from_file(path, password)
        .map_err(|e| format!("Could not open PDF: {e:?}"))?;

    let mut scanned = Vec::new();
    for (i, page) in doc.pages().iter().enumerate() {
        let text_len = page.text().map(|t| t.all().len()).unwrap_or(0);
        // If the page has fewer than 10 characters of extractable text,
        // it's likely a scanned image.
        if text_len < 10 {
            scanned.push((i + 1) as u32);
        }
    }
    Ok(scanned)
}

// ── Core OCR pipeline ─────────────────────────────────────────────────────

/// Run OCR on the specified pages and inject an invisible text layer into
/// the PDF, writing the result back to `src_path` (in-place).
pub fn run_ocr(req: &OcrRequest) -> Result<Vec<OcrPageResult>, String> {
    let exe = tesseract_exe();
    let dpi = req.dpi.unwrap_or(300);
    let lang = req.language.as_deref().unwrap_or("eng");

    // Verify tesseract is reachable.
    if Command::new(&exe).arg("--version").output().is_err() {
        return Err("Tesseract not found on this system.".to_string());
    }

    // Open the PDF with PDFium for page rendering.
    let pdfium = crate::engine::instance()?;
    let pdfium_doc = pdfium
        .load_pdf_from_file(&req.src_path, req.source_password.as_deref())
        .map_err(|e| format!("Could not open PDF for rendering: {e:?}"))?;

    // Open the same PDF with lopdf for text-layer injection.
    let mut lopdf_doc =
        Document::load(&req.src_path).map_err(|e| format!("lopdf could not open PDF: {e}"))?;

    let page_ids: Vec<ObjectId> = lopdf_doc.page_iter().collect();

    let tmp_dir = std::env::temp_dir().join("limitlesspdf-ocr");
    std::fs::create_dir_all(&tmp_dir).map_err(|e| format!("Temp dir: {e}"))?;

    let mut results = Vec::new();

    for &page_num in &req.pages {
        let idx = (page_num - 1) as usize;
        if idx >= pdfium_doc.pages().len() as usize {
            continue;
        }

        // 1. Render the page to an image via PDFium.
        let page = pdfium_doc
            .pages()
            .get(idx as u16)
            .map_err(|e| format!("Page {page_num}: {e:?}"))?;

        let width_pts = page.width().value;
        let height_pts = page.height().value;

        // Render at the requested DPI.
        let scale = dpi as f32 / 72.0;
        let px_w = (width_pts * scale) as i32;
        let px_h = (height_pts * scale) as i32;

        let bitmap = page
            .render_with_config(
                &pdfium_render::prelude::PdfRenderConfig::new()
                    .set_target_width(px_w)
                    .set_target_height(px_h),
            )
            .map_err(|e| format!("Render page {page_num}: {e:?}"))?;

        let img_path = tmp_dir.join(format!("page_{page_num}.ppm"));
        {
            use std::io::Write as _;
            let rgba = bitmap.as_rgba_bytes();
            let w = bitmap.width();
            let h = bitmap.height();
            let mut f = std::fs::File::create(&img_path)
                .map_err(|e| format!("Create image page {page_num}: {e}"))?;
            write!(f, "P6\n{w} {h}\n255\n")
                .map_err(|e| format!("PPM header page {page_num}: {e}"))?;
            // RGBA → RGB: drop the alpha byte from each pixel.
            let rgb: Vec<u8> = rgba.chunks(4).flat_map(|px| &px[..3]).copied().collect();
            f.write_all(&rgb)
                .map_err(|e| format!("PPM data page {page_num}: {e}"))?;
        }

        // 2. Run Tesseract to produce HOCR output.
        let hocr_base = tmp_dir.join(format!("page_{page_num}_hocr"));
        let tess_result = Command::new(&exe)
            .arg(img_path.to_str().unwrap())
            .arg(hocr_base.to_str().unwrap())
            .args(["-l", lang])
            .arg("--dpi")
            .arg(dpi.to_string())
            .arg("hocr")
            .output()
            .map_err(|e| format!("Tesseract failed for page {page_num}: {e}"))?;

        if !tess_result.status.success() {
            let err = String::from_utf8_lossy(&tess_result.stderr);
            return Err(format!("Tesseract error on page {page_num}: {err}"));
        }

        // Tesseract writes <base>.hocr
        let hocr_path = hocr_base.with_extension("hocr");
        let hocr_content = std::fs::read_to_string(&hocr_path)
            .map_err(|e| format!("Read HOCR page {page_num}: {e}"))?;

        // 3. Parse HOCR to extract words with bounding boxes.
        let words = parse_hocr(&hocr_content, px_w as f32, px_h as f32);

        if words.is_empty() {
            results.push(OcrPageResult {
                page: page_num,
                word_count: 0,
                text_preview: String::new(),
            });
            continue;
        }

        // 4. Inject invisible text layer into the lopdf Document.
        if idx < page_ids.len() {
            inject_text_layer(
                &mut lopdf_doc,
                page_ids[idx],
                &words,
                width_pts,
                height_pts,
                px_w as f32,
                px_h as f32,
            )?;
        }

        let full_text: String = words
            .iter()
            .map(|w| w.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let preview = if full_text.len() > 200 {
            format!("{}\u{2026}", &full_text[..200])
        } else {
            full_text.clone()
        };

        results.push(OcrPageResult {
            page: page_num,
            word_count: words.len(),
            text_preview: preview,
        });
    }

    // 5. Save the modified PDF back.
    lopdf_doc
        .save(&req.src_path)
        .map_err(|e| format!("Save OCR result: {e}"))?;

    // Cleanup temp files (best-effort).
    let _ = std::fs::remove_dir_all(&tmp_dir);

    Ok(results)
}

// ── HOCR parsing ──────────────────────────────────────────────────────────

/// Extract words with bounding boxes from Tesseract HOCR output.
/// HOCR format: `<span class='ocrx_word' title='bbox x0 y0 x1 y1; x_wconf 95'>word</span>`
fn parse_hocr(html: &str, _img_w: f32, _img_h: f32) -> Vec<OcrWord> {
    let mut words = Vec::new();

    let mut pos = 0;
    let bytes = html.as_bytes();

    while pos < bytes.len() {
        // Find next <span
        let span_start = match find_substr(html, pos, "<span") {
            Some(p) => p,
            None => break,
        };

        // Find the closing >
        let tag_end = match find_substr(html, span_start, ">") {
            Some(p) => p,
            None => break,
        };

        let tag = &html[span_start..=tag_end];

        // Check if it's an ocrx_word span.
        if !tag.contains("ocrx_word") {
            pos = tag_end + 1;
            continue;
        }

        // Extract the title attribute for bbox.
        let bbox = extract_bbox(tag);
        let conf = extract_confidence(tag);

        // Find the text content (up to </span>).
        let text_start = tag_end + 1;
        let text_end = match find_substr(html, text_start, "</span>") {
            Some(p) => p,
            None => break,
        };

        let raw_text = &html[text_start..text_end];
        // Strip any nested tags (like <strong>, <em>).
        let text = strip_tags(raw_text).trim().to_string();

        if let Some((x0, y0, x1, y1)) = bbox {
            if !text.is_empty() {
                words.push(OcrWord {
                    text,
                    bbox: (x0, y0, x1, y1),
                    confidence: conf.unwrap_or(0.0),
                });
            }
        }

        pos = text_end + 7; // skip past </span>
    }

    words
}

fn find_substr(haystack: &str, start: usize, needle: &str) -> Option<usize> {
    haystack[start..].find(needle).map(|i| start + i)
}

/// Extract `bbox x0 y0 x1 y1` from a title attribute.
fn extract_bbox(tag: &str) -> Option<(f32, f32, f32, f32)> {
    let title_start = tag.find("title=")?;
    let quote = tag.as_bytes().get(title_start + 6)?;
    let delim = *quote as char;
    let val_start = title_start + 7;
    let val_end = tag[val_start..].find(delim)? + val_start;
    let title = &tag[val_start..val_end];

    // Find "bbox x0 y0 x1 y1"
    let bbox_start = title.find("bbox ")?;
    let bbox_str = &title[bbox_start + 5..];
    // Take until ; or end of string.
    let bbox_part = bbox_str.split(';').next()?;
    let nums: Vec<f32> = bbox_part
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect();
    if nums.len() >= 4 {
        Some((nums[0], nums[1], nums[2], nums[3]))
    } else {
        None
    }
}

/// Extract `x_wconf NN` from a title attribute.
fn extract_confidence(tag: &str) -> Option<f32> {
    let idx = tag.find("x_wconf ")?;
    let rest = &tag[idx + 8..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Strip HTML tags from a string, keeping only text content.
fn strip_tags(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            result.push(ch);
        }
    }
    result
}

// ── Text layer injection via lopdf ────────────────────────────────────────

/// Inject an invisible text layer onto a PDF page. Each word is positioned
/// using a text matrix (Tm) and rendered in mode 3 (invisible) so it doesn't
/// obscure the scanned image but is selectable/searchable.
fn inject_text_layer(
    doc: &mut Document,
    page_id: ObjectId,
    words: &[OcrWord],
    page_w: f32,
    page_h: f32,
    img_w: f32,
    img_h: f32,
) -> Result<(), String> {
    // Scale factors: image pixels -> PDF points.
    let sx = page_w / img_w;
    let sy = page_h / img_h;

    // Build a content stream with invisible text.
    let mut stream_content = String::new();

    // Save graphics state, set invisible text rendering mode.
    stream_content.push_str("q\n");
    stream_content.push_str("BT\n");
    stream_content.push_str("3 Tr\n"); // render mode 3 = invisible

    // Use Helvetica as a standard base font.
    let font_name = "F_OCR";
    stream_content.push_str(&format!("/{font_name} 1 Tf\n"));

    for word in words {
        let (x0, _y0, _x1, y1) = word.bbox;
        let word_w_px = word.bbox.2 - word.bbox.0;
        let word_h_px = word.bbox.3 - word.bbox.1;

        // Convert from image coords (top-left origin) to PDF coords (bottom-left).
        let pdf_x = x0 * sx;
        let pdf_y = page_h - y1 * sy; // flip Y

        // Font size = word height in PDF points.
        let font_size = word_h_px * sy;
        if font_size < 1.0 {
            continue;
        }

        // Calculate horizontal scaling so the text spans the word's width.
        // Standard Helvetica: average char width ~ 500/1000 units = 0.5 * font_size.
        let char_count = word.text.chars().count().max(1) as f32;
        let estimated_text_w = char_count * font_size * 0.5;
        let tz = if estimated_text_w > 0.0 {
            (word_w_px * sx / estimated_text_w) * 100.0
        } else {
            100.0
        };

        // Tm sets the text matrix: [font_size*tz/100  0  0  font_size  tx  ty]
        let scaled_fs_x = font_size * tz / 100.0;
        stream_content.push_str(&format!(
            "{:.2} 0 0 {:.2} {:.2} {:.2} Tm\n",
            scaled_fs_x, font_size, pdf_x, pdf_y
        ));

        // Escape the text for a PDF string literal.
        let escaped = pdf_escape_string(&word.text);
        stream_content.push_str(&format!("({escaped}) Tj\n"));
    }

    stream_content.push_str("ET\n");
    stream_content.push_str("Q\n");

    // Add the font resource to the page.
    let font_id = doc.add_object(Dictionary::from_iter(vec![
        ("Type", Object::Name(b"Font".to_vec())),
        ("Subtype", Object::Name(b"Type1".to_vec())),
        ("BaseFont", Object::Name(b"Helvetica".to_vec())),
        ("Encoding", Object::Name(b"WinAnsiEncoding".to_vec())),
    ]));

    // Ensure the page's Resources/Font dict includes our OCR font.
    ensure_page_font(doc, page_id, font_name, font_id)?;

    // Append the text-layer stream to the page's Contents.
    let stream_bytes = stream_content.into_bytes();
    let stream_obj = Stream::new(Dictionary::new(), stream_bytes);
    let stream_id = doc.add_object(stream_obj);

    append_to_page_contents(doc, page_id, stream_id)?;

    Ok(())
}

/// Escape a string for use inside a PDF literal string `(...)`.
fn pdf_escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            '\\' => out.push_str("\\\\"),
            _ if ch.is_ascii() => out.push(ch),
            // Non-ASCII: use octal escapes for WinAnsiEncoding range, or skip.
            _ => {
                let code = ch as u32;
                if code < 256 {
                    out.push_str(&format!("\\{:03o}", code));
                }
                // Characters outside WinAnsiEncoding are dropped (Tesseract
                // output is typically ASCII/Latin-1 for Western languages).
            }
        }
    }
    out
}

/// Where the Font dictionary lives in the page's resource tree.
enum FontDictLocation {
    /// Font dict is a referenced object — add our font directly.
    Reference(ObjectId),
    /// Font dict is inline inside a referenced Resources dict.
    InlineInRefResources(ObjectId),
    /// Font dict is inline inside an inline Resources dict (both in page).
    InlineInPage,
    /// No Font dict; Resources is a referenced object — create Font in it.
    MissingFontInRef(ObjectId),
    /// No Font dict; Resources is inline in the page.
    MissingFontInInline,
    /// No Resources dict at all on the page.
    MissingAll,
}

/// Ensure the page's Resources/Font dictionary includes our OCR font entry.
///
/// PDF pages store font references in Page -> Resources -> Font -> <name>.
/// Each level can be either inline or a reference (ObjectId) to another
/// object. We read the structure first (immutable borrows), then write
/// (mutable borrows) to avoid borrow-checker conflicts.
fn ensure_page_font(
    doc: &mut Document,
    page_id: ObjectId,
    font_name: &str,
    font_obj_id: ObjectId,
) -> Result<(), String> {
    // ── Read phase: determine where the Font dictionary lives ─────────
    let location = {
        let page_obj = doc
            .get_object(page_id)
            .map_err(|_| "Page not found".to_string())?;
        let page = page_obj
            .as_dict()
            .map_err(|_| "Page is not a dictionary".to_string())?;

        match page.get(b"Resources") {
            Ok(Object::Reference(res_id)) => {
                let res_id = *res_id;
                match doc.get_object(res_id) {
                    Ok(res_obj) => match res_obj.as_dict() {
                        Ok(res) => match res.get(b"Font") {
                            Ok(Object::Reference(font_id)) => FontDictLocation::Reference(*font_id),
                            Ok(_) => FontDictLocation::InlineInRefResources(res_id),
                            Err(_) => FontDictLocation::MissingFontInRef(res_id),
                        },
                        Err(_) => FontDictLocation::MissingFontInRef(res_id),
                    },
                    Err(_) => FontDictLocation::MissingAll,
                }
            }
            Ok(res_val) => match res_val.as_dict() {
                Ok(res) => match res.get(b"Font") {
                    Ok(Object::Reference(font_id)) => FontDictLocation::Reference(*font_id),
                    Ok(_) => FontDictLocation::InlineInPage,
                    Err(_) => FontDictLocation::MissingFontInInline,
                },
                Err(_) => FontDictLocation::MissingAll,
            },
            Err(_) => FontDictLocation::MissingAll,
        }
    };

    // ── Write phase: add our font to the appropriate place ────────────
    match location {
        FontDictLocation::Reference(font_dict_id) => {
            // Font dict is its own object — add our entry directly.
            let fd_obj = doc
                .get_object_mut(font_dict_id)
                .map_err(|_| "Font dict not found".to_string())?;
            let fd = fd_obj
                .as_dict_mut()
                .map_err(|_| "Font dict is not a dictionary".to_string())?;
            fd.set(font_name.as_bytes(), Object::Reference(font_obj_id));
        }

        FontDictLocation::InlineInRefResources(res_id) => {
            // Font dict is inline inside a referenced Resources dict.
            let res_obj = doc
                .get_object_mut(res_id)
                .map_err(|_| "Resources not found".to_string())?;
            let res = res_obj
                .as_dict_mut()
                .map_err(|_| "Resources is not a dictionary".to_string())?;
            if let Ok(font_obj) = res.get_mut(b"Font") {
                if let Ok(fd) = font_obj.as_dict_mut() {
                    fd.set(font_name.as_bytes(), Object::Reference(font_obj_id));
                }
            }
        }

        FontDictLocation::InlineInPage => {
            // Both Resources and Font are inline in the page dict.
            let page_obj = doc
                .get_object_mut(page_id)
                .map_err(|_| "Page not found".to_string())?;
            let page = page_obj
                .as_dict_mut()
                .map_err(|_| "Page is not a dictionary".to_string())?;
            if let Ok(res_obj) = page.get_mut(b"Resources") {
                if let Ok(res) = res_obj.as_dict_mut() {
                    if let Ok(font_obj) = res.get_mut(b"Font") {
                        if let Ok(fd) = font_obj.as_dict_mut() {
                            fd.set(font_name.as_bytes(), Object::Reference(font_obj_id));
                        }
                    }
                }
            }
        }

        FontDictLocation::MissingFontInRef(res_id) => {
            // Resources exists as a reference, but has no Font entry.
            // Create a new Font dict object and add it to Resources.
            let new_font_dict = Dictionary::from_iter(vec![(
                font_name.as_bytes().to_vec(),
                Object::Reference(font_obj_id),
            )]);
            let new_font_id = doc.add_object(new_font_dict);

            let res_obj = doc
                .get_object_mut(res_id)
                .map_err(|_| "Resources not found".to_string())?;
            let res = res_obj
                .as_dict_mut()
                .map_err(|_| "Resources is not a dictionary".to_string())?;
            res.set(b"Font", Object::Reference(new_font_id));
        }

        FontDictLocation::MissingFontInInline => {
            // Resources is inline in page, but has no Font entry.
            let new_font_dict = Dictionary::from_iter(vec![(
                font_name.as_bytes().to_vec(),
                Object::Reference(font_obj_id),
            )]);
            let new_font_id = doc.add_object(new_font_dict);

            let page_obj = doc
                .get_object_mut(page_id)
                .map_err(|_| "Page not found".to_string())?;
            let page = page_obj
                .as_dict_mut()
                .map_err(|_| "Page is not a dictionary".to_string())?;
            if let Ok(res_obj) = page.get_mut(b"Resources") {
                if let Ok(res) = res_obj.as_dict_mut() {
                    res.set(b"Font", Object::Reference(new_font_id));
                }
            }
        }

        FontDictLocation::MissingAll => {
            // No Resources at all — create the whole chain:
            // Font dict object -> Resources dict object -> page reference.
            let new_font_dict = Dictionary::from_iter(vec![(
                font_name.as_bytes().to_vec(),
                Object::Reference(font_obj_id),
            )]);
            let new_font_id = doc.add_object(new_font_dict);

            let new_res_dict =
                Dictionary::from_iter(vec![(b"Font".to_vec(), Object::Reference(new_font_id))]);
            let new_res_id = doc.add_object(new_res_dict);

            let page_obj = doc
                .get_object_mut(page_id)
                .map_err(|_| "Page not found".to_string())?;
            let page = page_obj
                .as_dict_mut()
                .map_err(|_| "Page is not a dictionary".to_string())?;
            page.set(b"Resources", Object::Reference(new_res_id));
        }
    }

    Ok(())
}

/// What the page's Contents entry currently holds.
enum ContentsShape {
    /// A single stream reference.
    SingleRef(ObjectId),
    /// An array of stream references.
    Array(Vec<Object>),
    /// Anything else (missing, inline stream, etc.).
    Other,
}

/// Append a content stream to a page's Contents array.
fn append_to_page_contents(
    doc: &mut Document,
    page_id: ObjectId,
    stream_id: ObjectId,
) -> Result<(), String> {
    // ── Read phase: determine what Contents currently holds ────────────
    let shape = {
        let page_obj = doc
            .get_object(page_id)
            .map_err(|_| "Page not found".to_string())?;
        let page = page_obj
            .as_dict()
            .map_err(|_| "Page is not a dictionary".to_string())?;

        match page.get(b"Contents") {
            Ok(Object::Reference(r)) => ContentsShape::SingleRef(*r),
            Ok(Object::Array(arr)) => ContentsShape::Array(arr.clone()),
            _ => ContentsShape::Other,
        }
    };

    // ── Write phase: update Contents with the new stream ──────────────
    let page_obj = doc
        .get_object_mut(page_id)
        .map_err(|_| "Page not found".to_string())?;
    let page = page_obj
        .as_dict_mut()
        .map_err(|_| "Page is not a dictionary".to_string())?;

    match shape {
        ContentsShape::SingleRef(existing) => {
            // Single stream -> convert to array containing both.
            page.set(
                b"Contents",
                Object::Array(vec![
                    Object::Reference(existing),
                    Object::Reference(stream_id),
                ]),
            );
        }
        ContentsShape::Array(mut arr) => {
            arr.push(Object::Reference(stream_id));
            page.set(b"Contents", Object::Array(arr));
        }
        ContentsShape::Other => {
            // No existing Contents — set directly.
            page.set(b"Contents", Object::Reference(stream_id));
        }
    }

    Ok(())
}
