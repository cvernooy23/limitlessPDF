//! AcroForm form-field handling for the two save modes:
//!
//! * **editable** — keep the interactive fields, write each one's value into the
//!   live form (`/V`, plus `/AS` for checkboxes/radios) and set
//!   `/NeedAppearances` so any viewer regenerates the field appearances. The
//!   `/AcroForm` dictionary itself is re-attached to the rebuilt catalog by
//!   `annotate::save`.
//! * **flatten** — the values are stamped onto the page as text by the content
//!   engine; here we just remove the now-redundant widget annotations so nothing
//!   interactive remains.

use lopdf::{Dictionary, Document, Object, ObjectId, StringFormat};
use serde::Deserialize;
use std::collections::HashMap;

/// One field value from the frontend (editable save).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormValue {
    pub field_name: String,
    pub kind: String,
    pub value: String,
    #[allow(dead_code)]
    pub on_state: String,
}

/// Find the `/AcroForm` object referenced by a document's catalog, if any.
pub fn acroform_of(doc: &Document) -> Option<ObjectId> {
    let root = doc.trailer.get(b"Root").ok()?.as_reference().ok()?;
    let cat = doc.get_dictionary(root).ok()?;
    cat.get(b"AcroForm").ok()?.as_reference().ok()
}

/// Resolve an object (possibly an indirect reference) to a dictionary.
fn as_dict<'a>(doc: &'a Document, o: &'a Object) -> Option<&'a Dictionary> {
    match o {
        Object::Dictionary(d) => Some(d),
        Object::Reference(r) => doc.get_dictionary(*r).ok(),
        _ => None,
    }
}

/// A terminal (fillable) field plus the widget annotations that display it.
struct Target {
    field_id: ObjectId,
    name: String,
    widgets: Vec<ObjectId>,
}

/// Write field values into the live AcroForm and request appearance regen.
pub fn apply_values(doc: &mut Document, acroform_id: ObjectId, values: &[FormValue]) {
    if values.is_empty() {
        return;
    }
    let map: HashMap<&str, &FormValue> =
        values.iter().map(|v| (v.field_name.as_str(), v)).collect();

    // Viewers regenerate /AP from /V + /DA + /DR when this is set.
    if let Ok(af) = doc
        .get_object_mut(acroform_id)
        .and_then(|o| o.as_dict_mut())
    {
        af.set("NeedAppearances", Object::Boolean(true));
    }

    // Phase 1 (immutable): walk the field tree, collecting terminal fields and
    // their fully-qualified names.
    let roots: Vec<ObjectId> = doc
        .get_dictionary(acroform_id)
        .ok()
        .and_then(|d| d.get(b"Fields").ok())
        .and_then(|o| o.as_array().ok())
        .map(|a| a.iter().filter_map(|o| o.as_reference().ok()).collect())
        .unwrap_or_default();

    let mut targets: Vec<Target> = Vec::new();
    let mut stack: Vec<(ObjectId, String)> =
        roots.into_iter().map(|r| (r, String::new())).collect();
    while let Some((id, prefix)) = stack.pop() {
        let Ok(d) = doc.get_dictionary(id) else {
            continue;
        };
        let part = d
            .get(b"T")
            .ok()
            .and_then(|o| o.as_str().ok())
            .map(|s| String::from_utf8_lossy(s).into_owned());
        let name = match part {
            Some(p) if prefix.is_empty() => p,
            Some(p) => format!("{prefix}.{p}"),
            None => prefix.clone(),
        };
        let kids: Vec<ObjectId> = d
            .get(b"Kids")
            .ok()
            .and_then(|o| o.as_array().ok())
            .map(|a| a.iter().filter_map(|o| o.as_reference().ok()).collect())
            .unwrap_or_default();
        // Kids that are themselves fields (have /T) keep descending; otherwise
        // the kids are widget annotations and this node is terminal.
        let child_fields: Vec<ObjectId> = kids
            .iter()
            .copied()
            .filter(|k| {
                doc.get_dictionary(*k)
                    .map(|kd| kd.get(b"T").is_ok())
                    .unwrap_or(false)
            })
            .collect();
        if !child_fields.is_empty() {
            for c in child_fields {
                stack.push((c, name.clone()));
            }
        } else {
            let widgets = if kids.is_empty() { vec![id] } else { kids };
            targets.push(Target {
                field_id: id,
                name,
                widgets,
            });
        }
    }

    // Phase 2 (mutable): set values on the collected targets.
    for t in targets {
        let Some(v) = map.get(t.name.as_str()) else {
            continue;
        };
        match v.kind.as_str() {
            "checkbox" | "radio" => {
                let on = if v.value.is_empty() {
                    "Off".to_string()
                } else {
                    v.value.clone()
                };
                set_name(doc, t.field_id, "V", &on);
                for w in &t.widgets {
                    let won = widget_on_state(doc, *w);
                    let as_val = if !v.value.is_empty() && won.as_deref() == Some(v.value.as_str())
                    {
                        v.value.clone()
                    } else {
                        "Off".to_string()
                    };
                    set_name(doc, *w, "AS", &as_val);
                }
            }
            _ => {
                // Text and choice fields carry a string value.
                set_string(doc, t.field_id, "V", &v.value);
                // Drop stale appearances so NeedAppearances regenerates them.
                remove_key(doc, t.field_id, "AP");
                for w in &t.widgets {
                    remove_key(doc, *w, "AP");
                }
            }
        }
    }
}

/// Remove widget annotations from the given output pages (flatten mode).
pub fn strip_widgets(doc: &mut Document, pages: &[ObjectId]) {
    for &pid in pages {
        let refs: Vec<ObjectId> = {
            let Ok(pd) = doc.get_dictionary(pid) else {
                continue;
            };
            match pd.get(b"Annots") {
                Ok(Object::Array(a)) => a.iter().filter_map(|o| o.as_reference().ok()).collect(),
                Ok(Object::Reference(r)) => doc
                    .get_object(*r)
                    .ok()
                    .and_then(|o| o.as_array().ok())
                    .map(|a| a.iter().filter_map(|x| x.as_reference().ok()).collect())
                    .unwrap_or_default(),
                _ => Vec::new(),
            }
        };
        if refs.is_empty() {
            continue;
        }
        let keep: Vec<Object> = refs
            .into_iter()
            .filter(|r| {
                doc.get_dictionary(*r)
                    .map(|d| {
                        d.get(b"Subtype")
                            .ok()
                            .and_then(|o| o.as_name().ok())
                            .map(|n| String::from_utf8_lossy(n) != "Widget")
                            .unwrap_or(true)
                    })
                    .unwrap_or(true)
            })
            .map(Object::Reference)
            .collect();
        if let Ok(pd) = doc.get_object_mut(pid).and_then(|o| o.as_dict_mut()) {
            if keep.is_empty() {
                pd.remove(b"Annots");
            } else {
                pd.set("Annots", Object::Array(keep));
            }
        }
    }
}

/// The non-`Off` appearance state of a button widget (its "on" name), read from
/// the widget's `/AP /N` sub-dictionary.
fn widget_on_state(doc: &Document, wid: ObjectId) -> Option<String> {
    let wd = doc.get_dictionary(wid).ok()?;
    let ap = wd.get(b"AP").ok()?;
    let apd = as_dict(doc, ap)?;
    let n = apd.get(b"N").ok()?;
    let nd = as_dict(doc, n)?;
    for (k, _) in nd.iter() {
        if String::from_utf8_lossy(k) != "Off" {
            return Some(String::from_utf8_lossy(k).into_owned());
        }
    }
    None
}

fn set_name(doc: &mut Document, id: ObjectId, key: &str, val: &str) {
    if let Ok(d) = doc.get_object_mut(id).and_then(|o| o.as_dict_mut()) {
        d.set(key, Object::Name(val.as_bytes().to_vec()));
    }
}

fn set_string(doc: &mut Document, id: ObjectId, key: &str, val: &str) {
    let obj = pdf_string(val);
    if let Ok(d) = doc.get_object_mut(id).and_then(|o| o.as_dict_mut()) {
        d.set(key, obj);
    }
}

fn remove_key(doc: &mut Document, id: ObjectId, key: &str) {
    if let Ok(d) = doc.get_object_mut(id).and_then(|o| o.as_dict_mut()) {
        d.remove(key.as_bytes());
    }
}

/// Encode a string value for a field: ASCII literal, otherwise UTF-16BE w/ BOM.
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

    fn make_doc_with_form() -> (Document, ObjectId, ObjectId) {
        let mut doc = Document::with_version("1.7");

        // Create a text field widget
        let mut field = Dictionary::new();
        field.set("Type", Object::Name(b"Annot".to_vec()));
        field.set("Subtype", Object::Name(b"Widget".to_vec()));
        field.set("FT", Object::Name(b"Tx".to_vec()));
        field.set("T", Object::String(b"name".to_vec(), StringFormat::Literal));
        let field_id = doc.add_object(Object::Dictionary(field));

        // Create AcroForm
        let mut acroform = Dictionary::new();
        acroform.set("Fields", Object::Array(vec![Object::Reference(field_id)]));
        let acroform_id = doc.add_object(Object::Dictionary(acroform));

        // Create page with Annots
        let mut page = Dictionary::new();
        page.set("Type", Object::Name(b"Page".to_vec()));
        page.set("Annots", Object::Array(vec![Object::Reference(field_id)]));
        let page_id = doc.add_object(Object::Dictionary(page));

        // Create catalog
        let mut catalog = Dictionary::new();
        catalog.set("Type", Object::Name(b"Catalog".to_vec()));
        catalog.set("AcroForm", Object::Reference(acroform_id));
        let catalog_id = doc.add_object(Object::Dictionary(catalog));

        doc.trailer.set("Root", Object::Reference(catalog_id));

        (doc, acroform_id, page_id)
    }

    // ── acroform_of ──────────────────────────────────────────────────────

    #[test]
    fn acroform_of_finds_form() {
        let (doc, acroform_id, _) = make_doc_with_form();
        assert_eq!(acroform_of(&doc), Some(acroform_id));
    }

    #[test]
    fn acroform_of_returns_none_without_form() {
        let mut doc = Document::with_version("1.7");
        let mut catalog = Dictionary::new();
        catalog.set("Type", Object::Name(b"Catalog".to_vec()));
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));
        assert_eq!(acroform_of(&doc), None);
    }

    // ── apply_values ─────────────────────────────────────────────────────

    #[test]
    fn apply_values_sets_text_field() {
        let (mut doc, acroform_id, _) = make_doc_with_form();
        let values = vec![FormValue {
            field_name: "name".to_string(),
            kind: "text".to_string(),
            value: "John".to_string(),
            on_state: String::new(),
        }];
        apply_values(&mut doc, acroform_id, &values);

        // Check NeedAppearances was set
        let af = doc.get_dictionary(acroform_id).unwrap();
        assert!(af.get(b"NeedAppearances").unwrap().as_bool().unwrap());
    }

    #[test]
    fn apply_values_does_nothing_for_empty() {
        let (mut doc, acroform_id, _) = make_doc_with_form();
        apply_values(&mut doc, acroform_id, &[]);
        // NeedAppearances should NOT be set since values is empty
        let af = doc.get_dictionary(acroform_id).unwrap();
        assert!(af.get(b"NeedAppearances").is_err());
    }

    // ── strip_widgets ────────────────────────────────────────────────────

    #[test]
    fn strip_widgets_removes_widget_annots() {
        let (mut doc, _, page_id) = make_doc_with_form();
        strip_widgets(&mut doc, &[page_id]);
        let pd = doc.get_dictionary(page_id).unwrap();
        // Annots should be removed since all were widgets
        assert!(pd.get(b"Annots").is_err());
    }

    #[test]
    fn strip_widgets_keeps_non_widget_annots() {
        let mut doc = Document::with_version("1.7");

        // Non-widget annotation
        let mut annot = Dictionary::new();
        annot.set("Type", Object::Name(b"Annot".to_vec()));
        annot.set("Subtype", Object::Name(b"Highlight".to_vec()));
        let annot_id = doc.add_object(Object::Dictionary(annot));

        // Widget annotation
        let mut widget = Dictionary::new();
        widget.set("Type", Object::Name(b"Annot".to_vec()));
        widget.set("Subtype", Object::Name(b"Widget".to_vec()));
        let widget_id = doc.add_object(Object::Dictionary(widget));

        let mut page = Dictionary::new();
        page.set("Type", Object::Name(b"Page".to_vec()));
        page.set(
            "Annots",
            Object::Array(vec![
                Object::Reference(annot_id),
                Object::Reference(widget_id),
            ]),
        );
        let page_id = doc.add_object(Object::Dictionary(page));

        strip_widgets(&mut doc, &[page_id]);

        let pd = doc.get_dictionary(page_id).unwrap();
        let annots = pd.get(b"Annots").unwrap().as_array().unwrap();
        assert_eq!(annots.len(), 1);
    }

    // ── pdf_string ───────────────────────────────────────────────────────

    #[test]
    fn pdf_string_ascii_literal() {
        match pdf_string("hello") {
            Object::String(bytes, StringFormat::Literal) => {
                assert_eq!(bytes, b"hello");
            }
            _ => panic!("Expected literal string"),
        }
    }

    #[test]
    fn pdf_string_unicode_bom() {
        match pdf_string("caf\u{00e9}") {
            Object::String(bytes, StringFormat::Literal) => {
                assert_eq!(bytes[0], 0xFE);
                assert_eq!(bytes[1], 0xFF);
            }
            _ => panic!("Expected literal string"),
        }
    }

    #[test]
    fn apply_values_checkbox_checked_and_unchecked() {
        let mut doc = Document::with_version("1.7");

        // Create a checkbox widget with an AP dict specifying "Yes"
        let mut n_dict = Dictionary::new();
        n_dict.set("Off", Object::Dictionary(Dictionary::new()));
        n_dict.set("Yes", Object::Dictionary(Dictionary::new()));
        let mut ap_dict = Dictionary::new();
        ap_dict.set("N", Object::Dictionary(n_dict));

        let mut field = Dictionary::new();
        field.set("Type", Object::Name(b"Annot".to_vec()));
        field.set("Subtype", Object::Name(b"Widget".to_vec()));
        field.set("FT", Object::Name(b"Btn".to_vec()));
        field.set(
            "T",
            Object::String(b"subscribe".to_vec(), StringFormat::Literal),
        );
        field.set("AP", Object::Dictionary(ap_dict));
        let field_id = doc.add_object(Object::Dictionary(field));

        let mut acroform = Dictionary::new();
        acroform.set("Fields", Object::Array(vec![Object::Reference(field_id)]));
        let acroform_id = doc.add_object(Object::Dictionary(acroform));

        // Test checking the checkbox
        let check_val = vec![FormValue {
            field_name: "subscribe".to_string(),
            kind: "checkbox".to_string(),
            value: "Yes".to_string(),
            on_state: "Yes".to_string(),
        }];
        apply_values(&mut doc, acroform_id, &check_val);

        let fd = doc.get_dictionary(field_id).unwrap();
        assert_eq!(fd.get(b"V").unwrap().as_name().unwrap(), b"Yes");
        assert_eq!(fd.get(b"AS").unwrap().as_name().unwrap(), b"Yes");

        // Test unchecking the checkbox
        let uncheck_val = vec![FormValue {
            field_name: "subscribe".to_string(),
            kind: "checkbox".to_string(),
            value: "".to_string(),
            on_state: "Yes".to_string(),
        }];
        apply_values(&mut doc, acroform_id, &uncheck_val);

        let fd = doc.get_dictionary(field_id).unwrap();
        assert_eq!(fd.get(b"V").unwrap().as_name().unwrap(), b"Off");
        assert_eq!(fd.get(b"AS").unwrap().as_name().unwrap(), b"Off");
    }

    #[test]
    fn apply_values_radio_group() {
        let mut doc = Document::with_version("1.7");

        // Widget 1: ChoiceA
        let mut n1 = Dictionary::new();
        n1.set("Off", Object::Dictionary(Dictionary::new()));
        n1.set("ChoiceA", Object::Dictionary(Dictionary::new()));
        let mut ap1 = Dictionary::new();
        ap1.set("N", Object::Dictionary(n1));
        let mut w1 = Dictionary::new();
        w1.set("Type", Object::Name(b"Annot".to_vec()));
        w1.set("Subtype", Object::Name(b"Widget".to_vec()));
        w1.set("AP", Object::Dictionary(ap1));
        let w1_id = doc.add_object(Object::Dictionary(w1));

        // Widget 2: ChoiceB
        let mut n2 = Dictionary::new();
        n2.set("Off", Object::Dictionary(Dictionary::new()));
        n2.set("ChoiceB", Object::Dictionary(Dictionary::new()));
        let mut ap2 = Dictionary::new();
        ap2.set("N", Object::Dictionary(n2));
        let mut w2 = Dictionary::new();
        w2.set("Type", Object::Name(b"Annot".to_vec()));
        w2.set("Subtype", Object::Name(b"Widget".to_vec()));
        w2.set("AP", Object::Dictionary(ap2));
        let w2_id = doc.add_object(Object::Dictionary(w2));

        // Parent field
        let mut field = Dictionary::new();
        field.set("FT", Object::Name(b"Btn".to_vec()));
        field.set("T", Object::String(b"plan".to_vec(), StringFormat::Literal));
        field.set(
            "Kids",
            Object::Array(vec![Object::Reference(w1_id), Object::Reference(w2_id)]),
        );
        let field_id = doc.add_object(Object::Dictionary(field));

        let mut acroform = Dictionary::new();
        acroform.set("Fields", Object::Array(vec![Object::Reference(field_id)]));
        let acroform_id = doc.add_object(Object::Dictionary(acroform));

        // Select ChoiceB
        let val = vec![FormValue {
            field_name: "plan".to_string(),
            kind: "radio".to_string(),
            value: "ChoiceB".to_string(),
            on_state: "ChoiceB".to_string(),
        }];
        apply_values(&mut doc, acroform_id, &val);

        let parent = doc.get_dictionary(field_id).unwrap();
        assert_eq!(parent.get(b"V").unwrap().as_name().unwrap(), b"ChoiceB");

        let wd1 = doc.get_dictionary(w1_id).unwrap();
        assert_eq!(wd1.get(b"AS").unwrap().as_name().unwrap(), b"Off");

        let wd2 = doc.get_dictionary(w2_id).unwrap();
        assert_eq!(wd2.get(b"AS").unwrap().as_name().unwrap(), b"ChoiceB");
    }

    #[test]
    fn apply_values_removes_ap_for_text_and_choice() {
        let mut doc = Document::with_version("1.7");

        let mut field = Dictionary::new();
        field.set("Type", Object::Name(b"Annot".to_vec()));
        field.set("Subtype", Object::Name(b"Widget".to_vec()));
        field.set("FT", Object::Name(b"Ch".to_vec()));
        field.set(
            "T",
            Object::String(b"color".to_vec(), StringFormat::Literal),
        );
        field.set("AP", Object::Dictionary(Dictionary::new()));
        let field_id = doc.add_object(Object::Dictionary(field));

        let mut acroform = Dictionary::new();
        acroform.set("Fields", Object::Array(vec![Object::Reference(field_id)]));
        let acroform_id = doc.add_object(Object::Dictionary(acroform));

        let val = vec![FormValue {
            field_name: "color".to_string(),
            kind: "dropdown".to_string(),
            value: "Blue".to_string(),
            on_state: "".to_string(),
        }];
        apply_values(&mut doc, acroform_id, &val);

        let fd = doc.get_dictionary(field_id).unwrap();
        assert!(fd.get(b"AP").is_err()); // AP should have been removed
    }

    #[test]
    fn apply_values_hierarchical_field_names() {
        let mut doc = Document::with_version("1.7");

        // Child field
        let mut child = Dictionary::new();
        child.set("Type", Object::Name(b"Annot".to_vec()));
        child.set("Subtype", Object::Name(b"Widget".to_vec()));
        child.set("FT", Object::Name(b"Tx".to_vec()));
        child.set(
            "T",
            Object::String(b"first".to_vec(), StringFormat::Literal),
        );
        let child_id = doc.add_object(Object::Dictionary(child));

        // Parent field
        let mut parent = Dictionary::new();
        parent.set("T", Object::String(b"user".to_vec(), StringFormat::Literal));
        parent.set("Kids", Object::Array(vec![Object::Reference(child_id)]));
        let parent_id = doc.add_object(Object::Dictionary(parent));

        let mut acroform = Dictionary::new();
        acroform.set("Fields", Object::Array(vec![Object::Reference(parent_id)]));
        let acroform_id = doc.add_object(Object::Dictionary(acroform));

        let val = vec![FormValue {
            field_name: "user.first".to_string(),
            kind: "text".to_string(),
            value: "Bob".to_string(),
            on_state: "".to_string(),
        }];
        apply_values(&mut doc, acroform_id, &val);

        let cd = doc.get_dictionary(child_id).unwrap();
        match cd.get(b"V").unwrap() {
            Object::String(bytes, _) => assert_eq!(bytes, b"Bob"),
            _ => panic!("Expected string value"),
        }
    }

    #[test]
    fn widget_on_state_extraction() {
        let mut doc = Document::with_version("1.7");

        // Widget with on state
        let mut n = Dictionary::new();
        n.set("Off", Object::Dictionary(Dictionary::new()));
        n.set("CustomOn", Object::Dictionary(Dictionary::new()));
        let mut ap = Dictionary::new();
        ap.set("N", Object::Dictionary(n));
        let mut w1 = Dictionary::new();
        w1.set("AP", Object::Dictionary(ap));
        let w1_id = doc.add_object(Object::Dictionary(w1));

        assert_eq!(widget_on_state(&doc, w1_id), Some("CustomOn".to_string()));

        // Widget with only Off
        let mut n_off = Dictionary::new();
        n_off.set("Off", Object::Dictionary(Dictionary::new()));
        let mut ap_off = Dictionary::new();
        ap_off.set("N", Object::Dictionary(n_off));
        let mut w2 = Dictionary::new();
        w2.set("AP", Object::Dictionary(ap_off));
        let w2_id = doc.add_object(Object::Dictionary(w2));

        assert_eq!(widget_on_state(&doc, w2_id), None);

        // Widget without AP
        let w3_id = doc.add_object(Object::Dictionary(Dictionary::new()));
        assert_eq!(widget_on_state(&doc, w3_id), None);
    }

    #[test]
    fn strip_widgets_with_referenced_annots_array() {
        let mut doc = Document::with_version("1.7");

        let mut widget = Dictionary::new();
        widget.set("Type", Object::Name(b"Annot".to_vec()));
        widget.set("Subtype", Object::Name(b"Widget".to_vec()));
        let wid_id = doc.add_object(Object::Dictionary(widget));

        let annots_arr_id = doc.add_object(Object::Array(vec![Object::Reference(wid_id)]));

        let mut page = Dictionary::new();
        page.set("Type", Object::Name(b"Page".to_vec()));
        page.set("Annots", Object::Reference(annots_arr_id));
        let page_id = doc.add_object(Object::Dictionary(page));

        strip_widgets(&mut doc, &[page_id]);

        let pd = doc.get_dictionary(page_id).unwrap();
        assert!(pd.get(b"Annots").is_err());
    }
}
