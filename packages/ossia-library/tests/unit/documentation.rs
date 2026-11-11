use super::*;
use serde_json::json;

fn pack(root: &Path) {
    fs::create_dir_all(root).unwrap();
    let conn = Connection::open(root.join("index.sqlite")).unwrap();
    conn.execute_batch("CREATE TABLE chapters (id TEXT PRIMARY KEY,manual TEXT,title TEXT,body TEXT,url TEXT,path TEXT,anchor TEXT,commands TEXT); CREATE VIRTUAL TABLE search USING fts5(id UNINDEXED,title,commands,body);").unwrap();
    let body = "Absolute octave entry\n\n```lilypond\n  c'4 d' e' f' |\n```\n中文";
    conn.execute("INSERT INTO chapters VALUES ('notation/pitches#absolute','notation','Absolute octave entry',?1,'https://lilypond.org/doc/v2.26/Documentation/notation/writing-pitches#absolute','section.md','absolute','relative octave')",[body]).unwrap();
    conn.execute("INSERT INTO search VALUES ('notation/pitches#absolute','Absolute octave entry','relative octave',?1)",[body]).unwrap();
    drop(conn);
    fs::write(root.join("section.md"), body).unwrap();
    let mut identity = String::new();
    let mut files = Vec::new();
    for name in ["index.sqlite", "section.md"] {
        let bytes = fs::read(root.join(name)).unwrap();
        let sha = ContentHash::of(&bytes);
        identity.push_str(&format!("{name}\0{sha}\0{}\n", bytes.len()));
        files.push(json!({"path":name,"sha256":sha,"bytes":bytes.len()}));
    }
    fs::write(root.join("manifest.json"),serde_json::to_vec(&json!({"version":"2.26.0","formatVersion":1,"archiveSha256":lock().unwrap().sha256,"contentHash":ContentHash::of(identity.as_bytes()),"files":files})).unwrap()).unwrap();
}
fn args(action: DocsAction) -> DocsArgs {
    DocsArgs {
        action,
        query: Some("八度".into()),
        chapter_id: None,
        limit: None,
        offset: 0,
    }
}
#[test]
fn install_search_read_and_integrity_are_offline_and_bounded() {
    let root = tempfile::tempdir().unwrap();
    let resources = root.path().join("resources");
    let home = root.path().join("home");
    fs::create_dir(&home).unwrap();
    pack(&resources);
    let docs = Documentation::install(&home, &resources).unwrap();
    let hits = docs.query(args(DocsAction::Search)).unwrap();
    assert_eq!(hits.hits.len(), 1);
    assert!(Path::new(&hits.hits[0].path).is_file());
    let mut read = args(DocsAction::Read);
    read.chapter_id = Some(hits.hits[0].chapter_id.clone());
    read.limit = Some(8);
    let first = docs.query(read.clone()).unwrap();
    assert_eq!(first.text.as_deref(), Some("Absolute"));
    read.offset = first.next_offset.unwrap();
    assert!(docs.query(read).unwrap().text.is_some());
    let installed = Documentation::install(&home, &resources).unwrap();
    assert_eq!(installed.root, docs.root);
    assert!(!Documentation::inspect(&root.path().join("missing")).available);
    fs::write(docs.root.join("index.sqlite"), b"corrupt").unwrap();
    assert!(docs.query(args(DocsAction::Search)).is_err());
    assert!(Documentation::install(&home, &resources).is_err());
}
#[test]
fn invalid_version_or_manifest_never_publishes_a_pack() {
    let root = tempfile::tempdir().unwrap();
    pack(root.path());
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(root.path().join("manifest.json")).unwrap()).unwrap();
    manifest["version"] = json!("2.24.0");
    fs::write(
        root.path().join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    assert!(!Documentation::inspect(root.path()).available);
}
