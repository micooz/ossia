use super::*;
#[cfg(unix)]
use std::sync::Arc;
use std::time::Duration;

#[cfg(unix)]
fn snapshot(text: &str) -> SourceSnapshot {
    SourceSnapshot::new(
        "10000000-0000-4000-8000-000000000001".parse().unwrap(),
        text.as_bytes().to_vec(),
    )
    .unwrap()
}
const SOURCE: &str = "\\version \"2.26.0\"\n\\score { { c'1 } \\layout {} }\n";
fn empty_manifest() -> RenderManifest {
    RenderManifest {
        source_hash: ContentHash::of(SOURCE.as_bytes()),
        engine_build_id: "test-build".into(),
        options_hash: ContentHash::of(b"options"),
        pages: vec![],
        pdf: None,
        midi: None,
        diagnostics: vec![],
        format_errors: vec![],
    }
}

#[test]
fn derived_text_respects_comments_strings_scheme_and_actual_score_boundaries() {
    let text = r#"\pointAndClickOff
%{ \score { fake } %{ nested %} %}
name = "\\score { escaped \" quote"
#(display "}")
\score { { c'1 } \layout {} }
\header { title = "tail" }"#;
    let derived = source::derive(text, RenderFormat::Midi, true).unwrap();
    assert_eq!(derived.text.matches("\\midi {}").count(), 1);
    assert!(derived.text.find("\\midi {}").unwrap() < derived.text.find("\\header").unwrap());
    for (byte, ch) in text.char_indices() {
        if let Some(mapped) = derived
            .text
            .char_indices()
            .find_map(|(d, _)| (derived.original_byte(d) == Some(byte)).then_some(d))
        {
            if byte >= "\\pointAndClickOff".len() {
                assert_eq!(derived.text[mapped..].chars().next(), Some(ch));
            }
        }
    }
    assert_eq!(text.matches("\\midi").count(), 0);
    for opaque in [
        "{ c'1 }",
        "\\score { c'1 } \\score { d'1 }",
        "\\score \\Music",
        "\\score { c'1",
        "\\score { #{ c #} }",
    ] {
        assert_eq!(
            source::derive(opaque, RenderFormat::Midi, true)
                .unwrap_err()
                .code,
            ErrorCode::UnsupportedSource
        );
    }
    let active = "\\score { { c'1 } \\midi { \\tempo 4 = 123 } }";
    assert_eq!(
        source::derive(active, RenderFormat::Midi, true)
            .unwrap()
            .text,
        active
    );
}

#[test]
fn svg_links_and_diagnostics_map_original_unicode_and_never_map_library_files() {
    let original = "\\pointAndClickOff\r\n\\score { { \\mark \\markup { \"中文😀\" }\tc'1 } }\r\n";
    let derived = source::derive(original, RenderFormat::Midi, true).unwrap();
    let byte = derived.text.find("c'1").unwrap();
    let p = ossia_score::SourceMap::new(&derived.text)
        .position(byte)
        .unwrap();
    let start = derived.text[..byte].rfind('\n').unwrap() + 1;
    let column = derived.text[start..byte].chars().fold(1, |n, c| {
        if c == '\t' {
            n + 8 - (n - 1) % 8
        } else {
            n + 1
        }
    });
    let svg = format!("<svg xmlns:xlink='http://www.w3.org/1999/xlink'><a xlink:href='textedit:///private/tmp/job/score.ly:{}:{}:{}'><text>A &amp; B</text></a><a xlink:href='textedit:///engine/english.ly:1:0:1'/></svg>",p.line,p.column-1,column);
    let normalized = source::normalize_svg(&svg, &derived, original).unwrap();
    assert!(normalized.contains(&format!(
        "textedit://score.ly:{}:{}:{}",
        p.line,
        p.column - 1,
        column
    )));
    assert!(!normalized.contains("/private/tmp") && !normalized.contains("english.ly"));
    let diagnostics = diagnostics::parse(
        &format!(
            "score.ly:{}:{}: error: bad\n/engine/english.ly:1:1: warning: standard",
            p.line, column
        ),
        Path::new("/private/tmp/job"),
        Path::new("/engine"),
        Some(&derived),
        Some(original),
    );
    assert_eq!(
        diagnostics[0].position.unwrap().byte,
        original.find("c'1").unwrap()
    );
    assert_eq!(diagnostics[1].position, None);
    assert!(diagnostics::redact(
        "C:\\Users\\somebody\\file.ly /Users/person/private/file.ly",
        Path::new("/scratch"),
        Path::new("/engine")
    )
    .contains("<path>"));
}

#[test]
fn cache_recovers_and_rejects_escape_or_corrupted_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let cache = Cache::new(temp.path()).unwrap();
    let mut manifest = empty_manifest();
    manifest.pages.push(RenderPage {
        page: 1,
        artifact: cache.asset(b"<svg/>", "svg").unwrap(),
    });
    cache.publish("key", &manifest).unwrap();
    assert_eq!(
        cache.read("key", &manifest, RenderFormat::Svg).unwrap(),
        Some(manifest.clone())
    );
    let mut mismatch = manifest.clone();
    mismatch.engine_build_id.push('2');
    assert_eq!(
        cache.read("key", &mismatch, RenderFormat::Svg).unwrap(),
        None
    );
    fs::write(
        temp.path().join(&manifest.pages[0].artifact.path),
        "damaged",
    )
    .unwrap();
    assert_eq!(
        cache.read("key", &manifest, RenderFormat::Svg).unwrap(),
        None
    );
    cache.asset(b"<svg/>", "svg").unwrap();
    for path in [
        "../outside.svg",
        "/absolute.svg",
        "C:\\outside.svg",
        "assets/../../outside",
    ] {
        let mut invalid = manifest.clone();
        invalid.pages[0].artifact.path = path.into();
        cache.publish("key", &invalid).unwrap();
        assert!(cache.read("key", &manifest, RenderFormat::Svg).is_err());
    }
    fs::write(temp.path().join("renders/key.json"), b"truncated {").unwrap();
    assert_eq!(
        cache.read("key", &manifest, RenderFormat::Svg).unwrap(),
        None
    );
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), temp.path().join("escape")).unwrap();
        assert!(cache::confined(temp.path(), "escape/data").is_err());
    }
    assert_ne!(
        Cache::key(
            &manifest.source_hash,
            "one",
            &manifest.options_hash,
            RenderFormat::Svg
        ),
        Cache::key(
            &manifest.source_hash,
            "two",
            &manifest.options_hash,
            RenderFormat::Svg
        )
    );
    assert_ne!(
        Cache::key(
            &manifest.source_hash,
            "one",
            &manifest.options_hash,
            RenderFormat::Svg
        ),
        Cache::key(
            &manifest.source_hash,
            "one",
            &manifest.options_hash,
            RenderFormat::Pdf
        )
    );
}

#[test]
fn cache_lock_wait_observes_deadline_and_cancellation() {
    let temp = tempfile::tempdir().unwrap();
    let cache = Cache::new(temp.path()).unwrap();
    let _guard = cache.lock("same", &RunContext::default()).unwrap();
    let context = RunContext::new(Duration::from_millis(30));
    assert!(cache.lock("same", &context).is_err());
    let context = RunContext::default();
    context.cancellation.cancel();
    assert!(cache.lock("same", &context).is_err());
}

#[test]
fn output_groups_require_contiguous_numeric_pages_and_unique_exports() {
    let dir = tempfile::tempdir().unwrap();
    for n in [10, 3, 1, 7, 4, 8, 2, 5, 9, 6] {
        fs::write(dir.path().join(format!("score-{n}.svg")), "<svg/>").unwrap();
    }
    assert_eq!(
        collect(dir.path(), RenderFormat::Svg)
            .unwrap()
            .iter()
            .map(|x| x.0)
            .collect::<Vec<_>>(),
        (1..=10).collect::<Vec<_>>()
    );
    fs::remove_file(dir.path().join("score-5.svg")).unwrap();
    assert!(collect(dir.path(), RenderFormat::Svg).is_err());
    fs::write(dir.path().join("score.pdf"), "%PDF-1.4").unwrap();
    fs::write(dir.path().join("score-other.pdf"), "%PDF-1.4").unwrap();
    assert!(collect(dir.path(), RenderFormat::Pdf).is_err());
    fs::write(dir.path().join("another.svg"), "<svg/>").unwrap();
    assert!(collect(dir.path(), RenderFormat::Svg).is_err());
}

#[cfg(unix)]
fn fake_engine() -> (tempfile::TempDir, Engine) {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("引擎 with spaces");
    fs::create_dir(&root).unwrap();
    let scratch = dir.path().join("scratch");
    fs::create_dir(&scratch).unwrap();
    fs::create_dir_all(root.join("share/lilypond/2.26.0/ly")).unwrap();
    fs::write(
        root.join("share/lilypond/2.26.0/ly/english.ly"),
        "% standard",
    )
    .unwrap();
    let midi = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/local/inputs/midi-valid.mid");
    fs::copy(midi, root.join("valid.mid")).unwrap();
    let script = r#"#!/bin/sh
root=${0%/*}
case "$*" in
  *--version*) echo 'GNU LilyPond 2.26.0'; exit 0 ;;
esac
if test -f input.musicxml; then
  if grep -q negative input.musicxml; then
    i=0; while test "$i" -lt 250; do echo progress; i=$((i+1)); done
    echo 'warning: Negative skip' >&2
  elif grep -q loss input.musicxml; then echo 'warning: Invalid bpm' >&2; fi
  printf '\\version "2.26.0"\n\\score { { c\0471 } \\layout {} }\n' > converted.ly
  exit 0
fi
printf 'call\n' >> "$root/calls"
if grep -q FAIL score.ly; then echo 'score.ly:2:1: error: invalid' >&2; exit 1; fi
case "$*" in
  *--svg*) printf '<svg viewBox="0 0 100 100"><path d="M 0 0 L 10 10"/></svg>' > score.svg ;;
  *) printf '%%PDF-1.4\n' > score.pdf; cp "$root/valid.mid" score.midi ;;
esac
"#;
    fs::write(root.join("tool"), script).unwrap();
    fs::set_permissions(root.join("tool"), fs::Permissions::from_mode(0o755)).unwrap();
    let tool = Tool {
        program: "tool".into(),
        args: vec![],
    };
    let engine = Engine {
        root,
        scratch_root: scratch,
        version: "2.26.0".into(),
        target: Target {
            engine_build_id: "fake-build-1".into(),
            archive: None,
            tools: BTreeMap::from([
                ("lilypond".into(), tool.clone()),
                ("musicxml2ly".into(), tool.clone()),
                ("midi2ly".into(), tool),
            ]),
            required_directories: vec![],
        },
    };
    (dir, engine)
}

#[cfg(unix)]
#[test]
fn single_file_validation_keeps_opaque_handwritten_source_but_rejects_user_dependencies() {
    let (_temp, engine) = fake_engine();
    for allowed in [
        "\\include \"english.ly\"\n{ c'1 }",
        "%{ \\include \"user.ly\" %}\n{ c'1 }",
        "not valid lilypond but repairable",
        "#(display \"hello\")",
    ] {
        assert!(
            engine
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
    ] {
        assert_eq!(
            engine
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
        engine
            .convert(&[0xff], InputFormat::Ly, None, &RunContext::default())
            .unwrap_err()
            .code,
        ErrorCode::InvalidEncoding
    );
}

#[cfg(unix)]
#[test]
fn import_confirmation_is_bound_and_negative_skip_cannot_be_hidden_after_progress() {
    let (_temp, engine) = fake_engine();
    let context = RunContext::default();
    let loss = engine
        .convert(b"loss", InputFormat::MusicXml, None, &context)
        .unwrap();
    assert_eq!(loss.status, ImportStatus::NeedsConfirmation);
    let token = loss.confirmation.unwrap();
    assert!(
        engine
            .convert(b"loss", InputFormat::MusicXml, Some(&token), &context)
            .unwrap()
            .confirmed
    );
    assert!(engine
        .convert(b"other loss", InputFormat::MusicXml, Some(&token), &context)
        .is_err());
    assert_eq!(
        engine
            .convert(b"negative", InputFormat::MusicXml, None, &context)
            .unwrap()
            .status,
        ImportStatus::Refused
    );
    assert_eq!(
        engine
            .convert(b"negative", InputFormat::MusicXml, Some(&token), &context)
            .unwrap_err()
            .code,
        ErrorCode::ImportRefused
    );
}

#[cfg(unix)]
#[test]
fn concurrent_compilers_share_cache_and_options_builds_are_isolated() {
    let (_temp, engine) = fake_engine();
    let assets = tempfile::tempdir().unwrap();
    let engine = Arc::new(engine);
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
        fs::read_to_string(engine.root.join("calls"))
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
    let mut other = (*engine).clone();
    other.target.engine_build_id = "fake-build-2".into();
    other
        .compile(
            &snapshot(SOURCE),
            &CompileOptions::default(),
            assets.path(),
            &RunContext::default(),
        )
        .unwrap();
    assert_eq!(
        fs::read_to_string(engine.root.join("calls"))
            .unwrap()
            .lines()
            .count(),
        3
    );
    assert!(fs::read_dir(&engine.scratch_root).unwrap().next().is_none());
}

#[cfg(unix)]
#[test]
fn formats_fail_independently_and_failed_compilation_leaves_no_scratch() {
    let (_temp, engine) = fake_engine();
    let assets = tempfile::tempdir().unwrap();
    let options = CompileOptions {
        formats: vec![RenderFormat::Svg, RenderFormat::Midi],
        point_and_click: true,
    };
    let result = engine
        .compile(
            &snapshot("{ c'1 }"),
            &options,
            assets.path(),
            &RunContext::default(),
        )
        .unwrap();
    assert_eq!(result.pages.len(), 1);
    assert_eq!(result.format_errors[0].format, RenderFormat::Midi);
    let result = engine
        .compile(
            &snapshot("\\version \"2.26.0\"\nFAIL"),
            &CompileOptions::default(),
            assets.path(),
            &RunContext::default(),
        )
        .unwrap();
    assert!(result.pages.is_empty());
    assert_eq!(result.format_errors[0].error.code, ErrorCode::CompileFailed);
    assert!(fs::read_dir(&engine.scratch_root).unwrap().next().is_none());
}

#[test]
fn invalid_resource_inventories_fail_without_running_tools() {
    let dir = tempfile::tempdir().unwrap();
    let context = RunContext::default();
    assert!(Engine::verify(dir.path(), dir.path(), &context).is_err());
    let lock: Lock = serde_json::from_str(include_str!("../../../vendor/ENGINE.lock")).unwrap();
    let target = &lock.targets[host_target().unwrap()];
    if let Some(archive) = &target.archive {
        fs::write(dir.path().join("bad.bin"), "bad").unwrap();
        let inventory = serde_json::json!({"schemaVersion":1,"target":host_target().unwrap(),"engineBuildId":target.engine_build_id,"archiveSha256":archive.sha256,"files":[{"path":"bad.bin","sha256":ContentHash::of(b"correct"),"bytes":3,"executable":false}]});
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
    let root = tempfile::tempdir().unwrap();
    let scratch = tempfile::tempdir().unwrap();
    let lock: Lock = serde_json::from_str(include_str!("../../../vendor/ENGINE.lock")).unwrap();
    let target = &lock.targets[host_target().unwrap()];
    let archive = target.archive.as_ref().unwrap();
    for directory in &target.required_directories {
        fs::create_dir_all(root.path().join(directory)).unwrap();
        fs::write(root.path().join(directory).join("stub"), b"stub").unwrap();
    }
    let executable="#!/bin/sh\ncase \"$*\" in *--version*) echo 'GNU LilyPond 2.26.00';; *) echo '<svg/>' > score.svg;; esac\n";
    for tool in target.tools.values() {
        let path = root.path().join(&tool.program);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, executable).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        for arg in &tool.args {
            let path = root.path().join(arg);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "stub").unwrap();
        }
    }
    let inventory = |build: &str| {
        let mut paths = BTreeSet::new();
        resource_files(root.path(), root.path(), &mut paths).unwrap();
        let entries:Vec<_>=paths.into_iter().map(|path| {
            let file=root.path().join(&path); let bytes=fs::read(&file).unwrap();
            serde_json::json!({"path":path,"sha256":ContentHash::of(&bytes),"bytes":bytes.len(),"executable":fs::metadata(file).unwrap().permissions().mode() & 0o111 !=0})
        }).collect();
        fs::write(root.path().join("resources.json"),serde_json::json!({"schemaVersion":1,"target":host_target().unwrap(),"engineBuildId":build,"archiveSha256":archive.sha256,"files":entries}).to_string()).unwrap();
    };
    inventory(&target.engine_build_id);
    assert_eq!(
        Engine::verify(root.path(), scratch.path(), &RunContext::default())
            .unwrap_err()
            .code,
        ErrorCode::RuntimeIncompatible
    );
    for tool in target.tools.values() {
        let path = root.path().join(&tool.program);
        fs::write(path, executable.replace("2.26.00", "2.26.0")).unwrap();
    }
    inventory(&target.engine_build_id);
    assert!(Engine::verify(root.path(), scratch.path(), &RunContext::default()).is_ok());
    inventory("other-build");
    assert!(
        Engine::verify(root.path(), scratch.path(), &RunContext::default())
            .unwrap_err()
            .message
            .contains("locked build")
    );
    fs::remove_file(root.path().join(&target.tools["midi2ly"].args[0])).unwrap();
    inventory(&target.engine_build_id);
    assert!(
        Engine::verify(root.path(), scratch.path(), &RunContext::default())
            .unwrap_err()
            .message
            .contains("tool is missing")
    );
    assert!(fs::read_dir(scratch.path()).unwrap().next().is_none());
}
