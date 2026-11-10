use ossia_types::*;
use serde_json::{json, Value};

#[test]
fn snapshots_validate_encoding_hash_and_contract_wire_shape() {
    let examples: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/local/contract-examples.json"
    ))
    .unwrap();
    let snapshot: SourceSnapshot =
        serde_json::from_value(examples["sourceSnapshot"].clone()).unwrap();
    assert_eq!(
        snapshot.bytes(),
        include_bytes!("../../../tests/fixtures/local/inputs/compressed-rest.ly")
    );
    assert_eq!(
        serde_json::to_value(&snapshot).unwrap(),
        examples["sourceSnapshot"]
    );
    let mut bad = examples["sourceSnapshot"].clone();
    bad["bytes"] = json!([255]);
    assert!(serde_json::from_value::<SourceSnapshot>(bad)
        .unwrap_err()
        .to_string()
        .contains("UTF-8"));
    let mut bad = examples["sourceSnapshot"].clone();
    bad["sourceHash"] = json!("0".repeat(64));
    assert!(serde_json::from_value::<SourceSnapshot>(bad).is_err());
    assert!(serde_json::from_value::<ContentHash>(json!("F".repeat(64))).is_err());
    assert!(serde_json::from_value::<DocumentId>(json!("not-a-uuid")).is_err());
    assert_eq!(
        serde_json::to_value(AppError::new(ErrorCode::StaleSelection, "reread selection")).unwrap(),
        json!({"code":"stale-selection","message":"reread selection"})
    );
}

#[test]
fn library_contract_examples_keep_ids_metadata_and_error_codes() {
    let examples: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/local/contract-examples.json"
    ))
    .unwrap();
    let version: Version = serde_json::from_value(examples["version"].clone()).unwrap();
    let edit: EditSession = serde_json::from_value(examples["editSession"].clone()).unwrap();
    assert_eq!(version.document_id, edit.document_id);
    assert_eq!(serde_json::to_value(version).unwrap(), examples["version"]);
    assert_eq!(serde_json::to_value(edit).unwrap(), examples["editSession"]);
    for (code, wire) in [
        (ErrorCode::StorageError, "storage-error"),
        (ErrorCode::LibraryBusy, "library-busy"),
    ] {
        assert_eq!(serde_json::to_value(code).unwrap(), json!(wire));
    }
}
