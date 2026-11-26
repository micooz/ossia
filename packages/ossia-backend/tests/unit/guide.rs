use super::*;

#[test]
fn embedded_documents_are_selected_without_arbitrary_file_access() {
    let home = tempfile::tempdir().unwrap();
    let index = crate::guide::document(home.path(), None).unwrap();
    assert!(index.text.contains("# Ossia guide"));
    for name in [
        "editing",
        "import",
        "evidence",
        "library",
        "output",
        "operations",
    ] {
        let document =
            crate::guide::document(home.path(), Some(&format!("guide/{name}.md"))).unwrap();
        assert!(!document.text.is_empty());
        assert_eq!(document.bytes, document.text.len());
        assert_eq!(document.lines, document.text.lines().count());
    }
    for name in ["../secret", "/etc/passwd", "guide/unknown.md"] {
        assert!(crate::guide::document(home.path(), Some(name)).is_err());
    }
}

#[test]
fn guide_is_materialized_in_the_data_directory() {
    let home = tempfile::tempdir().unwrap();
    let path = crate::guide::path(home.path());
    assert!(!path.exists());

    let outcome = crate::guide::outcome(home.path()).unwrap();
    assert_eq!(outcome.path, path.to_string_lossy());
    assert_eq!(outcome.bytes, GUIDE.len());
    assert_eq!(outcome.lines, GUIDE.lines().count());
    assert_eq!(std::fs::read(&path).unwrap(), GUIDE.as_bytes());

    let docs = [
        (
            "editing.md",
            include_str!("../../resources/guide/editing.md"),
        ),
        ("import.md", include_str!("../../resources/guide/import.md")),
        (
            "evidence.md",
            include_str!("../../resources/guide/evidence.md"),
        ),
        (
            "library.md",
            include_str!("../../resources/guide/library.md"),
        ),
        ("output.md", include_str!("../../resources/guide/output.md")),
        (
            "operations.md",
            include_str!("../../resources/guide/operations.md"),
        ),
    ];
    for (name, content) in docs {
        let relative = format!("guide/{name}");
        assert!(GUIDE.contains(&format!("]({relative})")), "{relative}");
        let child = path.parent().unwrap().join(relative);
        assert_eq!(std::fs::read_to_string(&child).unwrap(), content);
        std::fs::write(child, b"stale").unwrap();
    }
    std::fs::write(&path, b"stale").unwrap();
    crate::guide::outcome(home.path()).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), GUIDE.as_bytes());
    for (name, content) in docs {
        let child = path.parent().unwrap().join("guide").join(name);
        assert_eq!(std::fs::read_to_string(child).unwrap(), content);
    }
    assert!(crate::guide::outcome(home.path()).is_ok());
}

#[test]
fn guide_reports_child_write_failure() {
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join("docs")).unwrap();
    std::fs::write(home.path().join("docs/guide"), b"not a directory").unwrap();
    assert!(crate::guide::outcome(home.path()).is_err());
    assert!(!crate::guide::path(home.path()).exists());
}

fn all_guide_text() -> String {
    let home = tempfile::tempdir().unwrap();
    crate::guide::outcome(home.path()).unwrap();
    let mut text = GUIDE.to_owned();
    for name in [
        "editing.md",
        "import.md",
        "evidence.md",
        "library.md",
        "output.md",
        "operations.md",
    ] {
        text.push_str(&std::fs::read_to_string(home.path().join("docs/guide").join(name)).unwrap());
    }
    text
}

#[test]
fn guide_points_to_schema_lookup_and_skill_scaffolds() {
    assert!(all_guide_text().contains("ossia schema <command>"));
    for command in ["update-metadata", "write-source", "read-source"] {
        assert!(all_guide_text().contains(command), "{command}");
    }
    for scaffold in [
        "references/piano.ly",
        "references/single-staff.ly",
        "references/guitar-tab.ly",
        "references/jianpu.ly",
    ] {
        assert!(all_guide_text().contains(scaffold));
    }
}

#[test]
fn guide_documents_opt_in_convert_source() {
    assert!(all_guide_text().contains("includeSource: true"));
}

#[test]
fn guide_lists_import_inputs_and_limits() {
    for required in [
        "one local file by absolute path",
        "128 MiB",
        "`.musicxml`, `.xml`",
        "`.mxl`",
        "one rootfile",
        "`.mid`, `.midi`",
        "`.jpg`, `.jpeg`",
        "`.tif`, `.tiff`",
        "one MusicXML movement",
        "30 minutes",
        "`waitMs` is limited to 30 seconds",
    ] {
        assert!(all_guide_text().contains(required), "{required}");
    }
}

#[test]
fn guide_documents_remote_omr_configuration() {
    for required in [
        "Settings → General → Score recognition",
        "MCP and",
        "CLI do not expose a settings command",
        "**Ossia OMR**",
        "**Access key** (访问密钥)",
        "`OSSIA_OMR_TOKEN`",
        "`Remote OMR token is not configured`",
        "`Custom remote OMR URL is not configured`",
        "PDF and image imports need an access key",
        "Tell me once it is",
        "saved and I will retry the same import",
        "edit `preferences.json` directly",
    ] {
        assert!(all_guide_text().contains(required), "{required}");
    }
}
