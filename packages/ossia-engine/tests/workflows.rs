use ossia_engine::*;
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    fs::{self, Permissions},
    path::{Path, PathBuf},
};

const SOURCE: &str = "\\version \"2.26.0\"\n\\score { { c'1 } \\layout {} }\n";

#[cfg(unix)]
#[test]
fn import_report_distinguishes_unavailable_measure_count_from_zero() {
    let f = fixture();
    for (source, expected) in [
        ("Music = { c'1 | d'1 | } \\score { \\new Staff \\Music }", json!(2)),
        ("Music = { c'1 | d'1 } \\score { \\new Staff \\Music }", Value::Null),
        ("Music = { \\repeat unfold 2 { c'1 | } } \\score { \\new Staff \\Music }", Value::Null),
        ("Unused = { c1 | c1 | c1 | } Music = { c'1 | } \\score { \\new Staff \\Music }", json!(1)),
        ("Upper = { c'1 | } Lower = { c1 } \\score { << \\new Staff \\Upper \\new Staff \\Lower >> }", Value::Null),
    ] {
        let report = f.engine.convert(source.as_bytes(), InputFormat::Ly, None, &RunContext::default()).unwrap();
        let report = serde_json::to_value(report).unwrap();
        assert_eq!(report["measures"], expected, "{source}");
        assert_eq!(report["measureCountReason"].is_string(), expected.is_null(), "{source}");
    }
}

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    scratch: PathBuf,
    engine: Engine,
}

fn lock() -> Value {
    serde_json::from_str(include_str!("../../../vendor/ENGINE.lock")).unwrap()
}

fn target() -> Value {
    lock()["targets"][host_target().unwrap()].clone()
}

fn snapshot(text: &str) -> SourceSnapshot {
    SourceSnapshot::new(
        "10000000-0000-4000-8000-000000000001".parse().unwrap(),
        text.as_bytes().to_vec(),
    )
    .unwrap()
}

#[cfg(unix)]
fn write_inventory(root: &Path, target: &Value) -> Value {
    use std::os::unix::fs::PermissionsExt;

    let mut paths = BTreeSet::new();
    collect_files(root, root, &mut paths);
    let files: Vec<_> = paths
        .into_iter()
        .map(|path| {
            let file = root.join(&path);
            let bytes = fs::read(&file).unwrap();
            json!({
                "path": path,
                "sha256": ContentHash::of(&bytes),
                "bytes": bytes.len(),
                "executable": fs::metadata(file).unwrap().permissions().mode() & 0o111 != 0,
            })
        })
        .collect();
    let inventory = json!({
        "schemaVersion": 1,
        "target": host_target().unwrap(),
        "engineBuildId": target["engineBuildId"],
        "archiveSha256": target["archive"]["sha256"],
        "files": files,
    });
    fs::write(
        root.join("resources.json"),
        serde_json::to_vec(&inventory).unwrap(),
    )
    .unwrap();
    inventory
}

#[cfg(unix)]
fn collect_files(root: &Path, directory: &Path, out: &mut BTreeSet<String>) {
    for entry in fs::read_dir(directory).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if relative == "resources.json" {
            continue;
        }
        if entry.file_type().unwrap().is_dir() {
            collect_files(root, &path, out);
        } else {
            out.insert(relative);
        }
    }
}

#[cfg(unix)]
fn fixture() -> Fixture {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("引擎 with spaces");
    let scratch = dir.path().join("scratch");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&scratch).unwrap();
    let target = target();
    for directory in target["requiredDirectories"].as_array().unwrap() {
        let path = root.join(directory.as_str().unwrap());
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("stub"), b"stub").unwrap();
    }
    fs::create_dir_all(root.join("share/lilypond/2.26.0/ly")).unwrap();
    fs::write(
        root.join("share/lilypond/2.26.0/ly/english.ly"),
        "% standard",
    )
    .unwrap();
    let midi = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/inputs/midi-valid.mid");
    fs::copy(midi, root.join("valid.mid")).unwrap();
    let script = r#"#!/bin/sh
root="@ROOT@"
case "$*" in
  *--version*) echo 'GNU LilyPond 2.26.0'; exit 0 ;;
esac
if test -f input.musicxml; then
  if grep -q negative input.musicxml; then
    i=0; while test "$i" -lt 250; do echo progress; i=$((i+1)); done
    echo 'warning: Negative skip' >&2
  elif grep -q loss input.musicxml; then echo 'warning: Invalid bpm' >&2; fi
  printf '\\version "2.26.0"\n\\score { { c\0471 } \\layout {}\n  %% To create MIDI output, uncomment the following line:\n  %% \\midi { \\tempo 4 = 100 }\n}\n' > converted.ly
  exit 0
fi
printf 'call\n' >> "$root/calls"
if grep -q FAIL score.ly; then echo 'score.ly:2:1: error: invalid' >&2; exit 1; fi
case "$*" in
  *--svg*) printf '<svg viewBox="0 0 100 100"><path d="M 0 0 L 10 10"/></svg>' > score.svg ;;
  *) printf '%%PDF-1.4\n' > score.pdf; cp "$root/valid.mid" score.midi ;;
esac
"#
    .replace("@ROOT@", &root.to_string_lossy());
    for tool in target["tools"].as_object().unwrap().values() {
        let program = root.join(tool["program"].as_str().unwrap());
        fs::create_dir_all(program.parent().unwrap()).unwrap();
        fs::write(&program, &script).unwrap();
        fs::set_permissions(&program, Permissions::from_mode(0o755)).unwrap();
        for arg in tool["args"].as_array().unwrap() {
            let path = root.join(arg.as_str().unwrap());
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "stub").unwrap();
        }
    }
    write_inventory(&root, &target);
    let engine = Engine::verify(&root, &scratch, &RunContext::default()).unwrap();
    let _ = fs::remove_file(root.join("calls"));
    Fixture {
        _dir: dir,
        root,
        scratch,
        engine,
    }
}

#[cfg(unix)]
#[test]
fn single_file_validation_keeps_opaque_handwritten_source_but_rejects_user_dependencies() {
    let f = fixture();
    for allowed in [
        "\\include \"english.ly\"\n{ c'1 }",
        "%{ \\include \"user.ly\" %}\n{ c'1 }",
        "not valid lilypond but repairable",
        "#(display \"hello\")",
    ] {
        assert!(
            f.engine
                .convert(
                    allowed.as_bytes(),
                    InputFormat::Ly,
                    None,
                    &RunContext::default()
                )
                .is_ok(),
            "{allowed}"
        );
    }
    for rejected in [
        "\\include \"user.ly\"",
        "\\include \"/tmp/user.ly\"",
        "\\include #name",
        r#"\include dynamic "english.ly""#,
        "\\epsfile #X #10 \"a.eps\"",
        "#(list #{ \\include \"user.ly\" #})",
        "#(list #{ \\image #X #10 \"a.png\" #})",
    ] {
        assert_eq!(
            f.engine
                .convert(
                    rejected.as_bytes(),
                    InputFormat::Ly,
                    None,
                    &RunContext::default()
                )
                .unwrap_err()
                .code,
            ErrorCode::UnsupportedSource
        );
    }
    assert_eq!(
        f.engine
            .convert(&[0xff], InputFormat::Ly, None, &RunContext::default())
            .unwrap_err()
            .code,
        ErrorCode::InvalidEncoding
    );
}

#[cfg(unix)]
#[test]
fn import_confirmation_is_bound_and_negative_skip_cannot_be_hidden_after_progress() {
    let f = fixture();
    let context = RunContext::default();
    let loss = f
        .engine
        .convert(b"loss", InputFormat::MusicXml, None, &context)
        .unwrap();
    assert_eq!(loss.status, ImportStatus::NeedsConfirmation);
    let token = loss.confirmation.unwrap();
    assert!(
        f.engine
            .convert(b"loss", InputFormat::MusicXml, Some(&token), &context)
            .unwrap()
            .confirmed
    );
    assert!(f
        .engine
        .convert(b"other loss", InputFormat::MusicXml, Some(&token), &context)
        .is_err());
    assert_eq!(
        f.engine
            .convert(b"negative", InputFormat::MusicXml, None, &context)
            .unwrap()
            .status,
        ImportStatus::Refused
    );
    assert_eq!(
        f.engine
            .convert(b"negative", InputFormat::MusicXml, Some(&token), &context)
            .unwrap_err()
            .code,
        ErrorCode::ImportRefused
    );
}

#[cfg(unix)]
#[test]
fn concurrent_compilers_share_cache_and_options_builds_are_isolated() {
    use std::sync::Arc;
    let f = fixture();
    let assets = tempfile::tempdir().unwrap();
    let engine = Arc::new(f.engine.clone());
    let mut threads = Vec::new();
    for _ in 0..4 {
        let engine = engine.clone();
        let assets = assets.path().to_owned();
        threads.push(std::thread::spawn(move || {
            engine
                .compile(
                    &snapshot(SOURCE),
                    &CompileOptions::default(),
                    &assets,
                    &RunContext::default(),
                )
                .unwrap()
        }));
    }
    let mut results = Vec::new();
    for thread in threads {
        results.push(thread.join().unwrap());
    }
    assert!(results.windows(2).all(|p| p[0] == p[1]));
    assert_eq!(
        fs::read_to_string(f.root.join("calls"))
            .unwrap()
            .lines()
            .count(),
        1
    );
    let options = CompileOptions {
        point_and_click: false,
        ..CompileOptions::default()
    };
    let different = engine
        .compile(
            &snapshot(SOURCE),
            &options,
            assets.path(),
            &RunContext::default(),
        )
        .unwrap();
    assert_ne!(different.options_hash, results[0].options_hash);
    assert_eq!(
        fs::read_to_string(f.root.join("calls"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    assert!(fs::read_dir(&f.scratch).unwrap().next().is_none());
}

#[cfg(unix)]
#[test]
fn formats_fail_independently_and_failed_compilation_leaves_no_scratch() {
    let f = fixture();
    let assets = tempfile::tempdir().unwrap();
    let options = CompileOptions {
        formats: vec![RenderFormat::Svg, RenderFormat::Midi],
        point_and_click: true,
    };
    let result = f
        .engine
        .compile(
            &snapshot("{ c'1 }"),
            &options,
            assets.path(),
            &RunContext::default(),
        )
        .unwrap();
    assert_eq!(result.pages.len(), 1);
    assert_eq!(result.format_errors[0].format, RenderFormat::Midi);
    let result = f
        .engine
        .compile(
            &snapshot("\\version \"2.26.0\"\nFAIL"),
            &CompileOptions::default(),
            assets.path(),
            &RunContext::default(),
        )
        .unwrap();
    assert!(result.pages.is_empty());
    assert_eq!(result.format_errors[0].error.code, ErrorCode::CompileFailed);
    assert!(fs::read_dir(&f.scratch).unwrap().next().is_none());
}

#[test]
fn invalid_resource_inventories_fail_without_running_tools() {
    let dir = tempfile::tempdir().unwrap();
    let context = RunContext::default();
    assert!(Engine::verify(dir.path(), dir.path(), &context).is_err());
    let target = target();
    if let Some(archive) = target["archive"]["sha256"].as_str() {
        fs::write(dir.path().join("bad.bin"), "bad").unwrap();
        let inventory = json!({
            "schemaVersion": 1,
            "target": host_target().unwrap(),
            "engineBuildId": target["engineBuildId"],
            "archiveSha256": archive,
            "files": [{
                "path": "bad.bin",
                "sha256": ContentHash::of(b"correct"),
                "bytes": 3,
                "executable": false
            }]
        });
        fs::write(dir.path().join("resources.json"), inventory.to_string()).unwrap();
        assert!(Engine::verify(dir.path(), dir.path(), &context)
            .unwrap_err()
            .message
            .contains("damaged"));
    }
}

#[cfg(unix)]
#[test]
fn prepared_resource_verification_checks_versions_tools_and_build_identity() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let scratch = dir.path().join("scratch");
    let target = target();
    for directory in target["requiredDirectories"].as_array().unwrap() {
        let path = root.join(directory.as_str().unwrap());
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("stub"), b"stub").unwrap();
    }
    let executable = "#!/bin/sh\ncase \"$*\" in *--version*) echo 'GNU LilyPond 2.26.00';; *) echo '<svg/>' > score.svg;; esac\n";
    for tool in target["tools"].as_object().unwrap().values() {
        let path = root.join(tool["program"].as_str().unwrap());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, executable).unwrap();
        fs::set_permissions(path, Permissions::from_mode(0o755)).unwrap();
        for arg in tool["args"].as_array().unwrap() {
            let path = root.join(arg.as_str().unwrap());
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "stub").unwrap();
        }
    }
    write_inventory(root, &target);
    assert_eq!(
        Engine::verify(root, &scratch, &RunContext::default())
            .unwrap_err()
            .code,
        ErrorCode::RuntimeIncompatible
    );
    for tool in target["tools"].as_object().unwrap().values() {
        let path = root.join(tool["program"].as_str().unwrap());
        fs::write(path, executable.replace("2.26.00", "2.26.0")).unwrap();
    }
    write_inventory(root, &target);
    assert!(Engine::verify(root, &scratch, &RunContext::default()).is_ok());
    let mut other = target.clone();
    other["engineBuildId"] = json!("other-build");
    write_inventory(root, &other);
    assert!(Engine::verify(root, &scratch, &RunContext::default())
        .unwrap_err()
        .message
        .contains("locked build"));
    let midi2ly = target["tools"]["midi2ly"].as_object().unwrap().clone();
    let arg = midi2ly["args"].as_array().unwrap()[0].as_str().unwrap();
    fs::remove_file(root.join(arg)).unwrap();
    write_inventory(root, &target);
    assert!(Engine::verify(root, &scratch, &RunContext::default())
        .unwrap_err()
        .message
        .contains("tool is missing"));
    assert!(fs::read_dir(&scratch).unwrap().next().is_none());
}
