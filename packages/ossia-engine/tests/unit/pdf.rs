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
