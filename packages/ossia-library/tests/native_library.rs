use ossia_library::*;
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[test]
#[ignore = "requires explicit OSSIA_TEST_LILYPOND_ROOT with verified native 2.26.0 resources"]
fn native_import_edit_check_commit_restore_and_reopen() {
    let root = PathBuf::from(
        std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").expect("set OSSIA_TEST_LILYPOND_ROOT"),
    );
    let work = tempfile::tempdir().unwrap();
    let home = work.path().join("中文 library with spaces");
    let engine = Engine::verify(
        &root,
        &work.path().join("独立 scratch"),
        &RunContext::default(),
    )
    .unwrap();
    let library = Library::open(&home).unwrap();
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/inputs");
    let mut cases = vec![];
    for (file, format, original_name) in [
        ("inline-staff.ly", InputFormat::Ly, "input.ly"),
        (
            "xml-clean.musicxml",
            InputFormat::MusicXml,
            "input.musicxml",
        ),
        ("midi-valid.mid", InputFormat::Midi, "input.mid"),
    ] {
        let input = fs::read(fixtures.join(file)).unwrap();
        let imported = library
            .import(
                &engine,
                ImportRequest {
                    title: file,
                    input: &input,
                    format,
                    confirmation: None,
                    author: "native-test",
                },
                &RunContext::default(),
            )
            .unwrap();
        let doc = imported.document.unwrap();
        let initial = doc.head_version_id.unwrap();
        let original = fs::read(&doc.source_path).unwrap();
        let edit = library
            .begin_edit(BeginEditRequest {
                document_id: doc.document_id,
                view_id: ViewId::new(),
                base_version_id: Some(initial),
                expected_source_hash: &doc.source_hash,
                author: "native-agent",
                adopt_workspace: false,
            })
            .unwrap();
        let mut changed = String::from_utf8(original.clone()).unwrap();
        if format == InputFormat::Ly {
            assert!(changed.contains("c'1"));
            changed = changed.replacen("c'1", "e'1", 1);
        } else {
            changed.push_str("\n% P03 fixed snapshot edit\n");
        }
        fs::write(&doc.source_path, changed.as_bytes()).unwrap();
        let hash = ContentHash::of(changed.as_bytes());
        let options = CompileOptions {
            formats: vec![RenderFormat::Svg, RenderFormat::Pdf, RenderFormat::Midi],
            point_and_click: true,
        };
        let checked = library
            .check(
                &engine,
                CheckRequest {
                    expect: None,
                    reference: None,
                    document_id: doc.document_id,
                    edit_id: Some(edit.edit_id),
                    expected_source_hash: &hash,
                    options: &options,
                },
                &RunContext::default(),
            )
            .unwrap();
        assert_eq!(
            checked.job.state,
            JobState::Done,
            "{file}: {:?}",
            checked.render
        );
        assert!(checked.current);
        assert!(checked.render.pdf.is_some());
        assert!(checked.render.midi.is_some());
        let version = library
            .commit(CommitRequest {
                edit_id: edit.edit_id,
                base_version_id: Some(initial),
                expected_source_hash: &hash,
                summary: "Native checked change",
            })
            .unwrap();
        assert_eq!(version.seq, 2);
        assert_eq!(version.parent_version_id, Some(initial));
        assert_eq!(
            library
                .version_snapshot(version.version_id)
                .unwrap()
                .bytes(),
            changed.as_bytes()
        );
        assert_eq!(fs::read(&doc.source_path).unwrap(), changed.as_bytes());
        let restored = library
            .restore(
                &engine,
                RestoreRequest {
                    target_version_id: initial,
                    base_version_id: Some(version.version_id),
                    expected_source_hash: &hash,
                    author: "native-test",
                    summary: "Restore initial",
                },
                &RunContext::default(),
            )
            .unwrap();
        assert_eq!(restored.seq, 3);
        assert_eq!(restored.parent_version_id, Some(version.version_id));
        assert_eq!(restored.source_hash, doc.source_hash);
        assert_eq!(fs::read(&doc.source_path).unwrap(), original);
        assert_eq!(
            fs::read(
                Path::new(&doc.source_path)
                    .parent()
                    .unwrap()
                    .join("original")
                    .join(original_name)
            )
            .unwrap(),
            input
        );
        assert!(!library.get_document(doc.document_id).unwrap().dirty);
        cases.push(json!({"input":file,"sourceHash":doc.source_hash,"editedSourceHash":hash,"restoredSourceHash":restored.source_hash,"versions":3,"svgPages":checked.render.pages.len(),"pdf":checked.render.pdf.is_some(),"midi":checked.render.midi.is_some()}));
    }
    let loss = fs::read(fixtures.join("xml-loss.musicxml")).unwrap();
    let pending = library
        .import(
            &engine,
            ImportRequest {
                title: "loss",
                input: &loss,
                format: InputFormat::MusicXml,
                confirmation: None,
                author: "native-test",
            },
            &RunContext::default(),
        )
        .unwrap();
    assert_eq!(pending.report.status, ImportStatus::NeedsConfirmation);
    assert!(pending.document.is_none());
    let confirmation = pending.report.confirmation.unwrap();
    let accepted = library
        .import(
            &engine,
            ImportRequest {
                title: "confirmed loss",
                input: &loss,
                format: InputFormat::MusicXml,
                confirmation: Some(&confirmation),
                author: "native-test",
            },
            &RunContext::default(),
        )
        .unwrap();
    assert!(accepted.report.confirmed);
    assert!(accepted.document.unwrap().head_version_id.is_some());
    let refused_input = fs::read(fixtures.join("xml-negative-skip.musicxml")).unwrap();
    let refused = library
        .import(
            &engine,
            ImportRequest {
                title: "refused",
                input: &refused_input,
                format: InputFormat::MusicXml,
                confirmation: None,
                author: "native-test",
            },
            &RunContext::default(),
        )
        .unwrap();
    assert_eq!(refused.report.status, ImportStatus::Refused);
    assert!(refused.document.is_none());
    let broken = b"\\version \"2.26.0\"\n\\score { { c'1 \\unknownPThreeCommand } \\layout {} }\n";
    let draft = library
        .import(
            &engine,
            ImportRequest {
                title: "repairable",
                input: broken,
                format: InputFormat::Ly,
                confirmation: None,
                author: "native-test",
            },
            &RunContext::default(),
        )
        .unwrap()
        .document
        .unwrap();
    assert!(draft.head_version_id.is_none());
    assert_eq!(draft.last_check.as_ref().unwrap().state, JobState::Error);
    let repair = library
        .begin_edit(BeginEditRequest {
            document_id: draft.document_id,
            view_id: ViewId::new(),
            base_version_id: None,
            expected_source_hash: &draft.source_hash,
            author: "native-fixer",
            adopt_workspace: false,
        })
        .unwrap();
    let fixed = b"\\version \"2.26.0\"\n\\score { { c'1 } \\layout {} }\n";
    fs::write(&draft.source_path, fixed).unwrap();
    let hash = ContentHash::of(fixed);
    let checked = library
        .check(
            &engine,
            CheckRequest {
                expect: None,
                reference: None,
                document_id: draft.document_id,
                edit_id: Some(repair.edit_id),
                expected_source_hash: &hash,
                options: &CompileOptions::default(),
            },
            &RunContext::default(),
        )
        .unwrap();
    assert_eq!(checked.job.state, JobState::Done);
    let fixed_version = library
        .commit(CommitRequest {
            edit_id: repair.edit_id,
            base_version_id: None,
            expected_source_hash: &hash,
            summary: "First successful compilation",
        })
        .unwrap();
    assert_eq!(fixed_version.origin, VersionOrigin::Import);
    assert_eq!(fixed_version.seq, 1);
    drop(library);
    let library = Library::open(&home).unwrap();
    assert_eq!(library.list_documents().unwrap().len(), 5);
    for doc in library.list_documents().unwrap() {
        assert!(!doc.dirty);
        assert!(doc.last_successful_render.is_some());
    }
    let evidence = json!({"target":format!("{}-{}",std::env::consts::OS,std::env::consts::ARCH),"engineBuildId":engine.build_id(),"cases":cases,"lossConfirmation":true,"refusedPublished":false,"repairableImport":true,"reopenedDocuments":5});
    if let Some(path) = std::env::var_os("OSSIA_NATIVE_EVIDENCE") {
        fs::write(
            path,
            format!("{}\n", serde_json::to_string_pretty(&evidence).unwrap()),
        )
        .unwrap();
    }
}

#[test]
#[ignore = "requires verified native LilyPond 2.26.0"]
fn native_scale_expectations_gate_commit_before_output() {
    let root = PathBuf::from(std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").unwrap());
    let work = tempfile::tempdir().unwrap();
    let engine =
        Engine::verify(&root, &work.path().join("scratch"), &RunContext::default()).unwrap();
    let library = Library::open(&work.path().join("library")).unwrap();
    let source="\\version \"2.26.0\"\n\\header { title = \"Scale\" }\nMusic = { \\time 4/4 c4 d e f | g4 a b c' | }\n\\score { \\new Staff \\Music \\layout {} \\midi {} }";
    let imported = library
        .import(
            &engine,
            ImportRequest {
                title: "Scale",
                input: source.as_bytes(),
                format: InputFormat::Ly,
                confirmation: None,
                author: "test",
            },
            &RunContext::default(),
        )
        .unwrap();
    let music = imported.music.unwrap();
    assert_eq!(music.source.measures, Some(2));
    assert_eq!(
        music
            .performance
            .notes
            .iter()
            .map(|n| n.pitch)
            .collect::<Vec<_>>(),
        [48, 50, 52, 53, 55, 57, 59, 60]
    );
    let doc = imported.document.unwrap();
    let first = &music.performance.notes[0];
    let pitches = vec![60, 62, 64, 65, 67, 69, 71, 72];
    let expect = MusicExpectation {
        note_count: Some(8),
        tracks: vec![TrackExpectation {
            track: first.track,
            channel: first.channel,
            pitches: Some(pitches.clone()),
            notes: Some(
                pitches
                    .iter()
                    .enumerate()
                    .map(|(i, p)| ExpectedNote {
                        pitch: *p,
                        start: QuarterTime::new(i as u64, 1).unwrap(),
                        duration: QuarterTime::new(1, 1).unwrap(),
                    })
                    .collect(),
            ),
        }],
    };
    let edit = library
        .begin_edit(BeginEditRequest {
            document_id: doc.document_id,
            view_id: ViewId::new(),
            base_version_id: doc.head_version_id,
            expected_source_hash: &doc.source_hash,
            author: "test",
            adopt_workspace: false,
        })
        .unwrap();
    let options = CompileOptions {
        formats: vec![RenderFormat::Svg, RenderFormat::Midi],
        point_and_click: true,
    };
    let checked = library
        .check(
            &engine,
            CheckRequest {
                expect: Some(&expect),
                reference: None,
                document_id: doc.document_id,
                edit_id: Some(edit.edit_id),
                expected_source_hash: &doc.source_hash,
                options: &options,
            },
            &RunContext::default(),
        )
        .unwrap();
    let result: CheckResult = serde_json::from_value(checked.job.result.unwrap()).unwrap();
    assert_eq!(result.verification.status, VerificationStatus::Failed);
    assert!(library
        .commit(CommitRequest {
            edit_id: edit.edit_id,
            base_version_id: edit.base_version_id,
            expected_source_hash: &doc.source_hash,
            summary: "wrong octave"
        })
        .is_err());
    let page = library
        .read_source(&ReadSourceArgs {
            document_id: doc.document_id,
            view_id: edit.owner_view_id,
            expected_source_hash: doc.source_hash.clone(),
            start: Some(SourceReadStart::Start),
            max_lines: None,
            max_bytes: None,
        })
        .unwrap();
    assert_eq!(page.content, source);
    let original_notes = "c4 d e f | g4 a b c'";
    let start = source.find(original_notes).unwrap();
    let span = ossia_score::SourceMap::new(source)
        .span(start, start + original_notes.len())
        .unwrap();
    let original_job = imported.job.unwrap().job_id;
    let original_svg_path = work.path().join("original.svg");
    let original_svg = library
        .output(
            &engine,
            &OutputArgs {
                source: OutputSource::Job {
                    job_id: original_job,
                },
                format: OutputFormat::Svg,
                page: None,
                width: None,
                output_path: Some(original_svg_path.to_string_lossy().into_owned()),
                wait_ms: None,
            },
            &original_svg_path,
            &RunContext::default(),
        )
        .unwrap();
    let written = library
        .write_source(&WriteSourceArgs {
            view_id: edit.owner_view_id,
            edit_id: edit.edit_id,
            expected_source_hash: doc.source_hash.clone(),
            change: SourceWriteChange::Edit {
                edits: vec![SourceEdit {
                    span,
                    expected_text: original_notes.into(),
                    replacement: "c'4 d' e' f' | g'4 a' b' c''".into(),
                }],
            },
            preview: true,
        })
        .unwrap();
    let hash = written.source_hash;
    let output_dir = tempfile::tempdir().unwrap();
    let checked = library
        .check(
            &engine,
            CheckRequest {
                expect: Some(&expect),
                reference: None,
                document_id: doc.document_id,
                edit_id: Some(edit.edit_id),
                expected_source_hash: &hash,
                options: &options,
            },
            &RunContext::default(),
        )
        .unwrap();
    for format in [
        OutputFormat::Svg,
        OutputFormat::Midi,
        OutputFormat::Pdf,
        OutputFormat::Png,
    ] {
        let path = output_dir.path().join(format!("new-{format:?}"));
        let artifact = library
            .output(
                &engine,
                &OutputArgs {
                    source: OutputSource::Job {
                        job_id: checked.job.job_id,
                    },
                    format,
                    page: None,
                    width: None,
                    output_path: Some(path.to_string_lossy().into_owned()),
                    wait_ms: None,
                },
                &path,
                &RunContext::default(),
            )
            .unwrap();
        assert!(Path::new(&artifact.path).is_file());
        assert_eq!(artifact.source_hash, hash);
        if format == OutputFormat::Png {
            let bytes = fs::read(&artifact.path).unwrap();
            assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
            assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 1600);
        }
    }
    let old_svg_path = work.path().join("old.svg");
    let old_svg = library
        .output(
            &engine,
            &OutputArgs {
                source: OutputSource::Job {
                    job_id: original_job,
                },
                format: OutputFormat::Svg,
                page: None,
                width: None,
                output_path: Some(old_svg_path.to_string_lossy().into_owned()),
                wait_ms: None,
            },
            &old_svg_path,
            &RunContext::default(),
        )
        .unwrap();
    assert_eq!(old_svg.sha256, original_svg.sha256);
    assert_ne!(old_svg.source_hash, hash);
    let invalid_path = work.path().join("invalid.png");
    assert!(library
        .output(
            &engine,
            &OutputArgs {
                source: OutputSource::Job {
                    job_id: checked.job.job_id,
                },
                format: OutputFormat::Png,
                page: None,
                width: Some(4097),
                output_path: Some(invalid_path.to_string_lossy().into_owned()),
                wait_ms: None,
            },
            &invalid_path,
            &RunContext::default()
        )
        .is_err());
    let result: CheckResult = serde_json::from_value(checked.job.result.unwrap()).unwrap();
    assert_eq!(
        result.verification.status,
        VerificationStatus::Passed,
        "{result:?}"
    );
    library
        .commit(CommitRequest {
            edit_id: edit.edit_id,
            base_version_id: edit.base_version_id,
            expected_source_hash: &hash,
            summary: "verified scale",
        })
        .unwrap();
}

#[test]
#[ignore = "requires verified native LilyPond 2.26.0"]
fn native_import_expectation_rejects_before_publishing_a_version() {
    let root = PathBuf::from(std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").unwrap());
    let work = tempfile::tempdir().unwrap();
    let engine =
        Engine::verify(&root, &work.path().join("scratch"), &RunContext::default()).unwrap();
    let library = Library::open(&work.path().join("library")).unwrap();
    let low = "\\version \"2.26.0\"\nMusic = { \\time 4/4 c4 d e f | g4 a b c' | }\n\\score { \\new Staff \\Music \\layout {} \\midi {} }";
    let expect = MusicExpectation {
        note_count: Some(8),
        tracks: vec![TrackExpectation {
            track: 1,
            channel: 0,
            pitches: Some(vec![60, 62, 64, 65, 67, 69, 71, 72]),
            notes: None,
        }],
    };
    let error = library
        .import_with_expect(
            &engine,
            ImportRequest {
                title: "low scale",
                input: low.as_bytes(),
                format: InputFormat::Ly,
                confirmation: None,
                author: "test",
            },
            Some(&expect),
            &RunContext::default(),
        )
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidInput);
    assert_eq!(
        error.detail.as_ref().unwrap()["verification"]["status"],
        "failed"
    );
    assert_eq!(library.document_count().unwrap(), 0);

    let high = "\\version \"2.26.0\"\nMusic = \\relative c' { \\time 4/4 c4 d e f | g a b c }\n\\score { \\new Staff \\Music \\layout {} \\midi {} }";
    let imported = library
        .import_with_expect(
            &engine,
            ImportRequest {
                title: "high scale",
                input: high.as_bytes(),
                format: InputFormat::Ly,
                confirmation: None,
                author: "test",
            },
            Some(&expect),
            &RunContext::default(),
        )
        .unwrap();
    assert!(imported.document.unwrap().head_version_id.is_some());
    assert_eq!(library.document_count().unwrap(), 1);
}
