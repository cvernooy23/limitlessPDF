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
    if let Ok(af) = doc.get_object_mut(acroform_id).and_then(|o| o.as_dict_mut()) {
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
        let Ok(d) = doc.get_dictionary(id) else { continue };
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
            .filter(|k| doc.get_dictionary(*k).map(|kd| kd.get(b"T").is_ok()).unwrap_or(false))
            .collect();
        if !child_fields.is_empty() {
            for c in child_fields {
                stack.push((c, name.clone()));
            }
        } else {
            let widgets = if kids.is_empty() { vec![id] } else { kids };
            targets.push(Target { field_id: id, name, widgets });
        }
    }

    // Phase 2 (mutable): set values on the collected targets.
    for t in targets {
        let Some(v) = map.get(t.name.as_str()) else { continue };
        match v.kind.as_str() {
            "checkbox" | "radio" => {
                let on = if v.value.is_empty() { "Off".to_string() } else { v.value.clone() };
                set_name(doc, t.field_id, "V", &on);
                for w in &t.widgets {
                    let won = widget_on_state(doc, *w);
                    let as_val =
                        if !v.value.is_empty() && won.as_deref() == Some(v.value.as_str()) {
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
            let Ok(pd) = doc.get_dictionary(pid) else { continue };
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
