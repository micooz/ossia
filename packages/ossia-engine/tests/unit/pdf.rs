use super::*;
use lopdf::{dictionary, Document, LoadOptions, Object};

fn minimal_pdf() -> Vec<u8> {
    let mut document = Document::with_version("1.5");
    let pages_id = document.new_object_id();
    let page_id = document.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
    });
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    document.trailer.set(
        "ID",
        vec![
            Object::string_literal("0123456789abcdef"),
            Object::string_literal("0123456789abcdef"),
        ],
    );
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    bytes
}

#[test]
fn encrypted_pdf_requires_the_configured_password() {
    let encrypted = protect_pdf(&minimal_pdf(), "ossia-secret").unwrap();
    assert!(encrypted.windows(8).any(|window| window == b"/Encrypt"));
    assert!(Document::load_mem_with_options(
        &encrypted,
        LoadOptions::with_password("ossia-secret")
    )
    .is_ok());
    assert!(
        Document::load_mem_with_options(&encrypted, LoadOptions::with_password("wrong")).is_err()
    );
}

#[test]
fn malformed_pdf_is_rejected() {
    assert_eq!(
        protect_pdf(b"not a pdf", "secret").unwrap_err().code,
        ErrorCode::CompileFailed
    );
}

#[test]
fn drawing_is_added_as_vector_content_before_password_protection() {
    use ossia_types::{Drawing, DrawingColor, DrawingPoint, DrawingStroke};
    let drawing = Drawing {
        revision: 1,
        strokes: vec![DrawingStroke {
            page: 1,
            width: 0.01,
            color: DrawingColor::Black,
            points: vec![
                DrawingPoint { x: 0.25, y: 0.5 },
                DrawingPoint { x: 0.75, y: 0.75 },
            ],
        }],
    };
    let annotated = overlay_pdf_drawing(&minimal_pdf(), &drawing).unwrap();
    let doc = Document::load_mem(&annotated).unwrap();
    let page = doc.get_pages()[&1];
    let content = String::from_utf8(doc.get_page_content(page)).unwrap();
    assert!(content.contains("5.95 w 148.75 421 m 446.25 210.5 l S"));
    let encrypted = protect_pdf(&annotated, "secret").unwrap();
    assert!(
        Document::load_mem_with_options(&encrypted, LoadOptions::with_password("secret")).is_ok()
    );
}

#[test]
fn drawing_uses_rotated_page_display_coordinates() {
    use ossia_types::{Drawing, DrawingColor, DrawingPoint, DrawingStroke};
    let mut document = Document::load_mem(&minimal_pdf()).unwrap();
    let page = document.get_pages()[&1];
    document
        .get_object_mut(page)
        .unwrap()
        .as_dict_mut()
        .unwrap()
        .set("Rotate", 90);
    let mut pdf = Vec::new();
    document.save_to(&mut pdf).unwrap();
    let drawing = Drawing {
        revision: 1,
        strokes: vec![DrawingStroke {
            page: 1,
            width: 0.01,
            color: DrawingColor::Red,
            points: vec![
                DrawingPoint { x: 0.25, y: 0.5 },
                DrawingPoint { x: 0.75, y: 0.75 },
            ],
        }],
    };
    let result = overlay_pdf_drawing(&pdf, &drawing).unwrap();
    let result = Document::load_mem(&result).unwrap();
    let content = String::from_utf8(result.get_page_content(result.get_pages()[&1])).unwrap();
    assert!(content.contains("1 0.23137254901960785 0.18823529411764706 RG"));
    assert!(content.contains("8.42 w 297.5 210.5 m 446.25 631.5 l S"));
}

#[test]
fn pdf_drawing_is_applied_only_to_its_own_page() {
    use ossia_types::{Drawing, DrawingColor, DrawingPoint, DrawingStroke};
    let mut document = Document::load_mem(&minimal_pdf()).unwrap();
    let first = document.get_pages()[&1];
    let parent = document
        .get_dictionary(first)
        .unwrap()
        .get(b"Parent")
        .unwrap()
        .as_reference()
        .unwrap();
    let second = document.add_object(dictionary! {
        "Type" => "Page", "Parent" => parent,
        "MediaBox" => vec![0.into(), 0.into(), 300.into(), 600.into()],
    });
    let parent = document
        .get_object_mut(parent)
        .unwrap()
        .as_dict_mut()
        .unwrap();
    parent.set(
        "Kids",
        vec![Object::Reference(first), Object::Reference(second)],
    );
    parent.set("Count", 2);
    let mut pdf = Vec::new();
    document.save_to(&mut pdf).unwrap();
    let drawing = Drawing {
        revision: 1,
        strokes: vec![DrawingStroke {
            page: 2,
            width: 0.02,
            color: DrawingColor::Black,
            points: vec![
                DrawingPoint { x: 0.25, y: 0.5 },
                DrawingPoint { x: 0.75, y: 0.75 },
            ],
        }],
    };
    let result = Document::load_mem(&overlay_pdf_drawing(&pdf, &drawing).unwrap()).unwrap();
    assert!(
        !String::from_utf8_lossy(&result.get_page_content(result.get_pages()[&1])).contains(" l S")
    );
    assert!(
        String::from_utf8_lossy(&result.get_page_content(result.get_pages()[&2]))
            .contains("6 w 75 300 m 225 150 l S")
    );
}

#[test]
fn selected_pdf_pages_keep_original_drawing_and_password() {
    use ossia_types::{Drawing, DrawingColor, DrawingPoint, DrawingStroke};
    let mut doc = Document::load_mem(&minimal_pdf()).unwrap();
    let first = doc.get_pages()[&1];
    let parent = doc
        .get_dictionary(first)
        .unwrap()
        .get(b"Parent")
        .unwrap()
        .as_reference()
        .unwrap();
    let second = doc.add_object(dictionary! { "Type" => "Page", "Parent" => parent, "MediaBox" => vec![0.into(), 0.into(), 300.into(), 600.into()] });
    let tree = doc.get_object_mut(parent).unwrap().as_dict_mut().unwrap();
    tree.set(
        "Kids",
        vec![Object::Reference(first), Object::Reference(second)],
    );
    tree.set("Count", 2);
    let mut input = Vec::new();
    doc.save_to(&mut input).unwrap();
    let drawing = Drawing {
        revision: 1,
        strokes: vec![DrawingStroke {
            page: 2,
            width: 0.02,
            color: DrawingColor::Black,
            points: vec![
                DrawingPoint { x: 0.25, y: 0.5 },
                DrawingPoint { x: 0.75, y: 0.75 },
            ],
        }],
    };
    let annotated = overlay_pdf_drawing(&input, &drawing).unwrap();
    let selected = select_pdf_pages(&annotated, &[2]).unwrap();
    let encrypted = protect_pdf(&selected, "secret").unwrap();
    let result =
        Document::load_mem_with_options(&encrypted, LoadOptions::with_password("secret")).unwrap();
    assert_eq!(result.get_pages().len(), 1);
    assert!(
        String::from_utf8_lossy(&result.get_page_content(result.get_pages()[&1]))
            .contains("6 w 75 300 m 225 150 l S")
    );
    assert!(select_pdf_pages(&input, &[3]).is_err());
    assert!(select_pdf_pages(&input, &[]).is_err());
    assert!(select_pdf_pages(&input, &[1, 1]).is_err());
}
