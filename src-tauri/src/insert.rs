//! Create single-page PDFs for inserting blank pages or image pages into a
//! document.  The generated temp files slot into the existing multi-source
//! architecture — the frontend loads them like any other PDF via `readPdf`.

use lopdf::{Dictionary, Document, Object, Stream};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Generate a unique temp file path with a descriptive prefix.
fn temp_pdf_path(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut p = std::env::temp_dir();
    p.push(format!("limitlesspdf-{prefix}-{nanos}.pdf"));
    p
}

/// Build a minimal one-page PDF document with a single page of the given
/// dimensions (in PDF points, 72 pt = 1 inch).  The page is blank — no
/// content stream.
pub fn create_blank_pdf(width: f64, height: f64) -> Result<String, String> {
    if width <= 0.0 || height <= 0.0 {
        return Err("Page dimensions must be positive".to_string());
    }

    let mut doc = Document::with_version("1.7");

    // Empty content stream so viewers don't complain about a missing /Contents.
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
            Object::Real(width as f32),
            Object::Real(height as f32),
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

    let path = temp_pdf_path("blank");
    doc.save(&path)
        .map_err(|e| format!("Failed to create blank PDF: {e}"))?;
    Ok(path.to_string_lossy().into_owned())
}

/// Read an image file, wrap it in a single-page PDF sized to the image, and
/// return the path to the temp PDF.  Supports JPEG, PNG, GIF, BMP, WebP, and
/// TIFF via the `image` crate.
pub fn create_image_pdf(image_path: &str) -> Result<String, String> {
    let img_bytes = std::fs::read(image_path)
        .map_err(|e| format!("Could not read image \"{image_path}\": {e}"))?;

    let img =
        image::load_from_memory(&img_bytes).map_err(|e| format!("Could not decode image: {e}"))?;

    let (img_w, img_h) = (img.width(), img.height());
    if img_w == 0 || img_h == 0 {
        return Err("Image has zero dimensions".to_string());
    }

    // Convert to RGB8 for PDF embedding.
    let rgb = img.to_rgb8();
    let raw_pixels: Vec<u8> = rgb.into_raw();

    // ── Build the PDF ────────────────────────────────────────────────────

    let mut doc = Document::with_version("1.7");

    // Page dimensions: 1 image pixel = 1 PDF point, capped at a maximum of
    // 14400 × 14400 pt (200 in × 200 in) to stay within PDF spec limits.
    let max_dim: f64 = 14400.0;
    let scale = (max_dim / img_w as f64)
        .min(max_dim / img_h as f64)
        .min(1.0);
    let page_w = (img_w as f64 * scale).max(1.0);
    let page_h = (img_h as f64 * scale).max(1.0);

    // Image XObject — raw RGB pixel data (compressed later by doc.compress()).
    let mut img_dict = Dictionary::new();
    img_dict.set("Type", Object::Name(b"XObject".to_vec()));
    img_dict.set("Subtype", Object::Name(b"Image".to_vec()));
    img_dict.set("Width", Object::Integer(img_w as i64));
    img_dict.set("Height", Object::Integer(img_h as i64));
    img_dict.set("ColorSpace", Object::Name(b"DeviceRGB".to_vec()));
    img_dict.set("BitsPerComponent", Object::Integer(8));

    let img_stream = Stream::new(img_dict, raw_pixels);
    let img_id = doc.add_object(img_stream);

    // Content stream: draw image scaled to fill the page.
    let content = format!("q {page_w:.4} 0 0 {page_h:.4} 0 0 cm /Img Do Q");
    let content_stream = Stream::new(Dictionary::new(), content.into_bytes());
    let content_id = doc.add_object(content_stream);

    // Resources dictionary referencing the image XObject.
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

    // Compress all streams (especially the large pixel data) with FlateDecode.
    doc.compress();

    let path = temp_pdf_path("image");
    doc.save(&path)
        .map_err(|e| format!("Failed to create image PDF: {e}"))?;
    Ok(path.to_string_lossy().into_owned())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Blank PDF ────────────────────────────────────────────────────────

    #[test]
    fn blank_pdf_letter_size() {
        let path = create_blank_pdf(612.0, 792.0).expect("should create blank PDF");
        assert!(std::path::Path::new(&path).exists());
        let doc = Document::load(&path).expect("should be a valid PDF");
        assert_eq!(doc.get_pages().len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn blank_pdf_a4_size() {
        let path = create_blank_pdf(595.0, 842.0).expect("should create A4 PDF");
        let doc = Document::load(&path).expect("valid PDF");
        assert_eq!(doc.get_pages().len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn blank_pdf_custom_dimensions() {
        let path = create_blank_pdf(300.0, 400.0).expect("custom size");
        let doc = Document::load(&path).expect("valid PDF");
        assert_eq!(doc.get_pages().len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn blank_pdf_rejects_zero_width() {
        let err = create_blank_pdf(0.0, 792.0).unwrap_err();
        assert!(err.contains("positive"), "Error: {err}");
    }

    #[test]
    fn blank_pdf_rejects_negative_height() {
        let err = create_blank_pdf(612.0, -1.0).unwrap_err();
        assert!(err.contains("positive"), "Error: {err}");
    }

    #[test]
    fn blank_pdf_unique_paths() {
        let p1 = create_blank_pdf(100.0, 100.0).unwrap();
        let p2 = create_blank_pdf(100.0, 100.0).unwrap();
        assert_ne!(p1, p2, "each call should produce a unique temp path");
        let _ = std::fs::remove_file(&p1);
        let _ = std::fs::remove_file(&p2);
    }

    #[test]
    fn blank_pdf_has_correct_media_box() {
        let w = 500.0_f64;
        let h = 700.0_f64;
        let path = create_blank_pdf(w, h).unwrap();
        let doc = Document::load(&path).unwrap();

        let pages = doc.get_pages();
        let (_page_num, &page_id) = pages.iter().next().unwrap();
        let page_dict = doc.get_dictionary(page_id).unwrap();
        let media_box = page_dict.get(b"MediaBox").unwrap().as_array().unwrap();

        // MediaBox = [0, 0, width, height]
        assert_eq!(media_box.len(), 4);
        let got_w = match &media_box[2] {
            Object::Real(v) => *v as f64,
            Object::Integer(v) => *v as f64,
            _ => panic!("unexpected type"),
        };
        let got_h = match &media_box[3] {
            Object::Real(v) => *v as f64,
            Object::Integer(v) => *v as f64,
            _ => panic!("unexpected type"),
        };
        assert!((got_w - w).abs() < 0.01, "width mismatch: {got_w} vs {w}");
        assert!((got_h - h).abs() < 0.01, "height mismatch: {got_h} vs {h}");

        let _ = std::fs::remove_file(&path);
    }

    // ── Image PDF ────────────────────────────────────────────────────────

    #[test]
    fn image_pdf_nonexistent_file() {
        let err = create_image_pdf("/nonexistent/image.png").unwrap_err();
        assert!(err.contains("Could not read image"), "Error: {err}");
    }

    #[test]
    fn image_pdf_invalid_data() {
        let path = temp_pdf_path("garbage").with_extension("png");
        std::fs::write(&path, b"this is not an image").unwrap();
        let err = create_image_pdf(&path.to_string_lossy()).unwrap_err();
        assert!(err.contains("Could not decode"), "Error: {err}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn image_pdf_from_png() {
        // Create a 100×50 red test PNG.
        let img = image::RgbImage::from_fn(100, 50, |_, _| image::Rgb([255, 0, 0]));
        let png_path = temp_pdf_path("red").with_extension("png");
        img.save(&png_path).expect("save test PNG");

        let pdf_path = create_image_pdf(&png_path.to_string_lossy()).expect("should create PDF");
        assert!(std::path::Path::new(&pdf_path).exists());

        let doc = Document::load(&pdf_path).expect("valid PDF");
        assert_eq!(doc.get_pages().len(), 1);

        let _ = std::fs::remove_file(&png_path);
        let _ = std::fs::remove_file(&pdf_path);
    }

    #[test]
    fn image_pdf_from_jpeg() {
        let img = image::RgbImage::from_fn(200, 300, |x, y| {
            image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
        });
        let jpg_path = temp_pdf_path("gradient").with_extension("jpg");
        img.save(&jpg_path).expect("save test JPEG");

        let pdf_path = create_image_pdf(&jpg_path.to_string_lossy()).expect("should create PDF");
        let doc = Document::load(&pdf_path).expect("valid PDF");
        assert_eq!(doc.get_pages().len(), 1);

        let _ = std::fs::remove_file(&jpg_path);
        let _ = std::fs::remove_file(&pdf_path);
    }

    #[test]
    fn image_pdf_page_matches_image_aspect() {
        let img = image::RgbImage::new(400, 200);
        let path = temp_pdf_path("aspect").with_extension("png");
        img.save(&path).unwrap();

        let pdf_path = create_image_pdf(&path.to_string_lossy()).unwrap();
        let doc = Document::load(&pdf_path).unwrap();

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

        // 400×200 image → 2:1 aspect ratio page
        let ratio = w / h;
        assert!(
            (ratio - 2.0).abs() < 0.01,
            "aspect ratio should be ~2:1, got {ratio}"
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&pdf_path);
    }

    #[test]
    fn image_pdf_contains_xobject() {
        let img = image::RgbImage::new(10, 10);
        let path = temp_pdf_path("xobj").with_extension("png");
        img.save(&path).unwrap();

        let pdf_path = create_image_pdf(&path.to_string_lossy()).unwrap();
        let doc = Document::load(&pdf_path).unwrap();

        // The page should have a Resources dict with an XObject entry.
        let pages = doc.get_pages();
        let (_, &page_id) = pages.iter().next().unwrap();
        let page_dict = doc.get_dictionary(page_id).unwrap();
        let resources = page_dict.get(b"Resources").unwrap().as_dict().unwrap();
        assert!(
            resources.has(b"XObject"),
            "page should have an XObject resource"
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&pdf_path);
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    #[test]
    fn temp_path_contains_prefix() {
        let p = temp_pdf_path("myprefix");
        let name = p.file_name().unwrap().to_string_lossy();
        assert!(name.contains("myprefix"), "path should contain prefix");
        assert!(name.ends_with(".pdf"), "path should end with .pdf");
    }
}
