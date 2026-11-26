use lopdf::encryption::crypt_filters::{Aes128CryptFilter, CryptFilter};
use lopdf::{
    Dictionary, Document, EncryptionState, EncryptionVersion, Object, ObjectId, Permissions, Stream,
};
use ossia_types::Drawing;
use std::{collections::BTreeMap, sync::Arc};

use crate::*;

fn inherited<'a>(document: &'a Document, mut id: ObjectId, key: &[u8]) -> Option<&'a Object> {
    for _ in 0..100 {
        let dict = document.get_dictionary(id).ok()?;
        if let Ok(value) = dict.get(key) {
            return document.dereference(value).ok().map(|(_, value)| value);
        }
        id = dict.get(b"Parent").ok()?.as_reference().ok()?;
    }
    None
}

fn pdf_number(value: &Object) -> Option<f64> {
    match value {
        Object::Integer(n) => Some(*n as f64),
        Object::Real(n) => Some(*n as f64),
        _ => None,
    }
}

/// Append freehand vectors to the generated PDF, preserving its text and score graphics.
pub fn overlay_pdf_drawing(bytes: &[u8], drawing: &Drawing) -> Result<Vec<u8>, AppError> {
    let error = |message: String| failure("pdf-drawing", &message);
    let mut document = Document::load_mem(bytes).map_err(|e| error(e.to_string()))?;
    for (page, id) in document.get_pages() {
        let strokes: Vec<_> = drawing
            .strokes
            .iter()
            .filter(|stroke| stroke.page == page)
            .collect();
        if strokes.is_empty() {
            continue;
        }
        let bounds = inherited(&document, id, b"CropBox")
            .or_else(|| inherited(&document, id, b"MediaBox"))
            .and_then(|value| value.as_array().ok())
            .ok_or_else(|| error("PDF page bounds are unavailable".into()))?;
        let bounds: Vec<f64> = bounds.iter().filter_map(pdf_number).collect();
        if bounds.len() != 4
            || bounds.iter().any(|v| !v.is_finite())
            || bounds[2] <= bounds[0]
            || bounds[3] <= bounds[1]
        {
            return Err(error("invalid PDF page bounds".into()));
        }
        let (x, y, width, height) = (
            bounds[0],
            bounds[1],
            bounds[2] - bounds[0],
            bounds[3] - bounds[1],
        );
        let rotation = inherited(&document, id, b"Rotate")
            .and_then(pdf_number)
            .unwrap_or(0.0) as i32;
        let rotation = rotation.rem_euclid(360);
        if ![0, 90, 180, 270].contains(&rotation) {
            return Err(error("unsupported PDF page rotation".into()));
        }
        let view_width = if rotation == 90 || rotation == 270 {
            height
        } else {
            width
        };
        let to_pdf = |point: &ossia_types::DrawingPoint| match rotation {
            90 => (x + point.y * width, y + point.x * height),
            180 => (x + (1.0 - point.x) * width, y + point.y * height),
            270 => (x + (1.0 - point.y) * width, y + (1.0 - point.x) * height),
            _ => (x + point.x * width, y + (1.0 - point.y) * height),
        };
        let mut content = String::from("Q\nq 0 G 1 J 1 j\n");
        for stroke in strokes {
            if stroke.points.is_empty() {
                continue;
            }
            let (px, py) = to_pdf(&stroke.points[0]);
            let [red, green, blue] = stroke.color.rgb();
            content.push_str(&format!(
                "{} {} {} RG ",
                f64::from(red) / 255.0,
                f64::from(green) / 255.0,
                f64::from(blue) / 255.0
            ));
            content.push_str(&format!("{} w {px} {py} m ", stroke.width * view_width));
            if stroke.points.len() == 1 {
                content.push_str(&format!("{px} {py} l "));
            }
            for point in stroke.points.iter().skip(1) {
                let (px, py) = to_pdf(point);
                content.push_str(&format!("{px} {py} l "));
            }
            content.push_str("S\n");
        }
        content.push_str("Q\n");
        let existing = document
            .get_dictionary(id)
            .map_err(|e| error(e.to_string()))?
            .get(b"Contents")
            .ok()
            .cloned();
        let original = match existing {
            Some(Object::Array(items)) => items,
            Some(Object::Reference(reference)) => match document.get_object(reference) {
                Ok(Object::Array(items)) => items.clone(),
                _ => vec![Object::Reference(reference)],
            },
            Some(other) => vec![other],
            None => vec![],
        };
        let prefix = document.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
        let suffix = document.add_object(Stream::new(Dictionary::new(), content.into_bytes()));
        let mut contents = vec![Object::Reference(prefix)];
        contents.extend(original);
        contents.push(Object::Reference(suffix));
        document
            .get_object_mut(id)
            .and_then(Object::as_dict_mut)
            .map_err(|e| error(e.to_string()))?
            .set("Contents", contents);
    }
    let mut output = Vec::new();
    document
        .save_to(&mut output)
        .map_err(|e| error(e.to_string()))?;
    Ok(output)
}

/// Keep the requested one-based pages in their original order.
pub fn select_pdf_pages(bytes: &[u8], pages: &[u32]) -> Result<Vec<u8>, AppError> {
    let mut document =
        Document::load_mem(bytes).map_err(|error| failure("pdf-pages", &error.to_string()))?;
    let existing = document.get_pages();
    let selected: std::collections::BTreeSet<_> = pages.iter().copied().collect();
    if selected.is_empty()
        || selected.len() != pages.len()
        || selected.iter().any(|page| !existing.contains_key(page))
    {
        return Err(AppError::invalid(
            "PDF pages must be unique and within the document",
        ));
    }
    let removed: Vec<_> = existing
        .keys()
        .filter(|page| !selected.contains(page))
        .copied()
        .collect();
    document.delete_pages(&removed);
    let mut output = Vec::new();
    document
        .save_to(&mut output)
        .map_err(|error| failure("pdf-pages", &error.to_string()))?;
    Ok(output)
}

/// Encrypt a PDF with an open password while preserving normal viewing and printing.
pub fn protect_pdf(bytes: &[u8], password: &str) -> Result<Vec<u8>, AppError> {
    let mut document = Document::load_mem(bytes).map_err(|error| {
        failure(
            "pdf-encrypt",
            &format!("could not read generated PDF: {error}"),
        )
    })?;
    let crypt_filter: Arc<dyn CryptFilter> = Arc::new(Aes128CryptFilter);
    let version = EncryptionVersion::V4 {
        document: &document,
        encrypt_metadata: true,
        crypt_filters: BTreeMap::from([(b"StdCF".to_vec(), crypt_filter)]),
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: password,
        user_password: password,
        permissions: Permissions::default(),
    };
    let state = EncryptionState::try_from(version).map_err(|error| {
        failure(
            "pdf-encrypt",
            &format!("could not prepare PDF encryption: {error}"),
        )
    })?;
    document.encrypt(&state).map_err(|error| {
        failure(
            "pdf-encrypt",
            &format!("could not encrypt generated PDF: {error}"),
        )
    })?;
    let mut output = Vec::new();
    document.save_to(&mut output).map_err(|error| {
        failure(
            "pdf-encrypt",
            &format!("could not write encrypted PDF: {error}"),
        )
    })?;
    Ok(output)
}

#[cfg(test)]
#[path = "../tests/unit/pdf.rs"]
mod tests;
