use ossia_library::*;
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[test]
#[ignore = "requires explicit OSSIA_TEST_LILYPOND_ROOT with verified native 2.26.0 resources"]
fn native_import_requires_version_and_confirms_unplayable_book() {
    let root = PathBuf::from(std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").unwrap());
    let work = tempfile::tempdir().unwrap();
    let engine =
        Engine::verify(&root, &work.path().join("scratch"), &RunContext::default()).unwrap();
    let library = Library::open(&work.path().join("library")).unwrap();
    let import = |input: &[u8], confirmation: Option<&ContentHash>| {
        library
            .import(
                &engine,
                ImportRequest {
                    title: "test",
                    input,
                    format: InputFormat::Ly,
                    confirmation,
                    author: "test",
                },
                &RunContext::default(),
            )
            .unwrap()
    };
    let old = import(
        b"\\version \"2.14.2\"\n\\score { { c'1 } \\layout {} }",
        None,
    );
    assert_eq!(old.report.status, ImportStatus::Refused);
    assert!(old.report.blocking[0].contains("2.14.2"));
    assert_eq!(library.document_count().unwrap(), 0);
    let source = b"\\version \"2.26.0\"\n\\book { \\score { { c'1 } \\layout {} } }";
    let result = import(source, None);
    assert_eq!(result.report.status, ImportStatus::NeedsConfirmation);
    assert!(result
        .report
        .playback_error
        .as_ref()
        .unwrap()
        .message
        .contains("\\book"));
    assert_eq!(library.document_count().unwrap(), 0);
    let accepted = import(source, result.report.confirmation.as_ref());
    assert!(accepted.document.is_some());
    assert_eq!(library.document_count().unwrap(), 1);
}

#[test]
#[ignore = "requires explicit OSSIA_TEST_LILYPOND_ROOT with verified native 2.26.0 resources"]
fn native_visual_outputs_include_the_same_page_drawing() {
    let root = PathBuf::from(
        std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").expect("set OSSIA_TEST_LILYPOND_ROOT"),
    );
    let work = tempfile::tempdir().unwrap();
    let engine =
        Engine::verify(&root, &work.path().join("scratch"), &RunContext::default()).unwrap();
    let library = Library::open(&work.path().join("library")).unwrap();
    let imported = library
        .import(
            &engine,
            ImportRequest {
                title: "Drawing test",
                input: include_bytes!("../../../tests/fixtures/inputs/inline-staff.ly"),
                format: InputFormat::Ly,
                confirmation: None,
                author: "test",
            },
            &RunContext::default(),
        )
        .unwrap();
    let document = imported.document.unwrap();
    let drawing = Drawing {
        revision: 1,
        strokes: vec![DrawingStroke {
            page: 1,
            width: 0.01,
            color: DrawingColor::Black,
            points: vec![
                DrawingPoint { x: 0.2, y: 0.3 },
                DrawingPoint { x: 0.8, y: 0.3 },
            ],
        }],
    };
    for format in [OutputFormat::Svg, OutputFormat::Png, OutputFormat::Pdf] {
        let base = OutputArgs {
            source: OutputSource::Version {
                version_id: document.head_version_id.unwrap(),
            },
            format,
            pages: Some(vec![1]),
            width: (format == OutputFormat::Png).then_some(800),
            pdf_password: None,
            png_quality: None,
            png_transparent: (format == OutputFormat::Png).then_some(true),
            include_drawing: Some(true),
            output_path: None,
            output_directory: None,
            crop: None,
            padding_staff_spaces: None,
            audio_range: None,
            wait_ms: None,
        };
        let plain_path = work.path().join(format!("plain-{format:?}"));
        let marked_path = work.path().join(format!("marked-{format:?}"));
        library
            .output_with_drawing(&engine, &base, &plain_path, &RunContext::default(), None)
            .unwrap();
        library
            .output_with_drawing(
                &engine,
                &base,
                &marked_path,
                &RunContext::default(),
                Some(&drawing),
            )
            .unwrap();
        let plain = fs::read(plain_path).unwrap();
        let marked = fs::read(marked_path).unwrap();
        assert_ne!(plain, marked, "{format:?} has no drawing");
        if format == OutputFormat::Svg {
            assert!(std::str::from_utf8(&marked)
                .unwrap()
                .contains("<polyline points="));
        }
        if format == OutputFormat::Png {
            let decode = |bytes: Vec<u8>| {
                let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
                decoder.set_transformations(png::Transformations::EXPAND);
                let mut reader = decoder.read_info().unwrap();
                let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
                let info = reader.next_frame(&mut pixels).unwrap();
                (info, pixels)
            };
            let (plain_info, plain_pixels) = decode(plain);
            let (marked_info, marked_pixels) = decode(marked);
            assert_eq!(marked_info.color_type, png::ColorType::Rgba);
            assert_eq!(
                (plain_info.width, plain_info.height),
                (marked_info.width, marked_info.height)
            );
            assert!(marked_pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[3] == 0));
            let x = (marked_info.width as f64 * 0.5) as usize;
            let y = (marked_info.height as f64 * 0.3) as usize;
            let index = (y * marked_info.width as usize + x) * 4 + 3;
            assert!(marked_pixels[index] > plain_pixels[index]);
            continue;
        }
        if format == OutputFormat::Pdf {
            assert!(marked.windows(5).any(|part| part == b"1 J 1"));
            let mut protected = base.clone();
            protected.pdf_password = Some("secret".into());
            let protected_path = work.path().join("protected.pdf");
            library
                .output_with_drawing(
                    &engine,
                    &protected,
                    &protected_path,
                    &RunContext::default(),
                    Some(&drawing),
                )
                .unwrap();
            assert!(fs::read(protected_path)
                .unwrap()
                .windows(8)
                .any(|part| part == b"/Encrypt"));
            let mut invalid = base.clone();
            invalid.pages = Some(vec![2]);
            assert!(library
                .output_with_drawing(
                    &engine,
                    &invalid,
                    &work.path().join("missing.pdf"),
                    &RunContext::default(),
                    None
                )
                .is_err());
        }
    }
}

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
        ("xml-clean.mxl", InputFormat::Mxl, "input.mxl"),
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
                    discard_current_edit: false,
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
    let negative_input = fs::read(fixtures.join("xml-negative-skip.musicxml")).unwrap();
    let negative = library
        .import(
            &engine,
            ImportRequest {
                title: "negative skip",
                input: &negative_input,
                format: InputFormat::MusicXml,
                confirmation: None,
                author: "native-test",
            },
            &RunContext::default(),
        )
        .unwrap();
    assert_eq!(negative.report.status, ImportStatus::NeedsConfirmation);
    assert!(negative.document.is_none());
    assert!(negative
        .report
        .losses
        .iter()
        .any(|loss| loss.contains("Negative skip")));
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
    assert_eq!(library.list_documents().unwrap().len(), 6);
    for doc in library.list_documents().unwrap() {
        assert!(!doc.dirty);
        assert!(doc.last_successful_render.is_some());
    }
    let evidence = json!({"target":format!("{}-{}",std::env::consts::OS,std::env::consts::ARCH),"engineBuildId":engine.build_id(),"cases":cases,"lossConfirmation":true,"refusedPublished":false,"repairableImport":true,"reopenedDocuments":6});
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
    let engine = Engine::verify(&root, &work.path().join("scratch"), &RunContext::default())
        .unwrap()
        .with_audio_root(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ui/public/audio"));
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
            expected_source_hash: doc.source_hash.clone(),
            start: Some(SourceReadStart::Start),
            max_lines: None,
            max_bytes: None,
            include_line_spans: false,
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
                pages: None,
                width: None,
                pdf_password: None,
                png_quality: None,
                png_transparent: None,
                output_path: Some(original_svg_path.to_string_lossy().into_owned()),
                output_directory: None,
                crop: None,
                padding_staff_spaces: None,
                audio_range: None,
                include_drawing: None,
                wait_ms: None,
            },
            &original_svg_path,
            &RunContext::default(),
        )
        .unwrap();
    let written = library
        .write_source(&WriteSourceArgs {
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
        OutputFormat::Wav,
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
                    pages: None,
                    width: None,
                    pdf_password: None,
                    png_quality: None,
                    png_transparent: None,
                    output_path: Some(path.to_string_lossy().into_owned()),
                    output_directory: None,
                    crop: None,
                    padding_staff_spaces: None,
                    audio_range: None,
                    include_drawing: None,
                    wait_ms: None,
                },
                &path,
                &RunContext::default(),
            )
            .unwrap();
        assert!(Path::new(&artifact.path).is_file());
        assert_eq!(artifact.source_hash, hash);
        if matches!(format, OutputFormat::Pdf | OutputFormat::Svg) {
            let bytes = fs::read(&artifact.path).unwrap();
            assert!(!bytes.windows(9).any(|window| window == b"textedit:"));
        }
        if format == OutputFormat::Png {
            let bytes = fs::read(&artifact.path).unwrap();
            assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
            assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 1600);
        }
        if format == OutputFormat::Wav {
            let bytes = fs::read(&artifact.path).unwrap();
            assert!(bytes.starts_with(b"RIFF"));
            assert_eq!(&bytes[8..12], b"WAVE");
            assert_eq!(artifact.media_type, "audio/wav");
        }
    }
    for format in [OutputFormat::Midi, OutputFormat::Wav] {
        let path = output_dir.path().join(format!("range-{format:?}"));
        let artifact = library
            .output(
                &engine,
                &OutputArgs {
                    source: OutputSource::Job {
                        job_id: checked.job.job_id,
                    },
                    format,
                    pages: None,
                    width: None,
                    pdf_password: None,
                    png_quality: None,
                    png_transparent: None,
                    output_path: Some(path.to_string_lossy().into_owned()),
                    output_directory: None,
                    crop: None,
                    padding_staff_spaces: None,
                    audio_range: Some(MeasureRange { start: 1, end: 1 }),
                    include_drawing: None,
                    wait_ms: None,
                },
                &path,
                &RunContext::default(),
            )
            .unwrap();
        let bytes = fs::read(&artifact.path).unwrap();
        match format {
            OutputFormat::Midi => {
                let midi = ossia_score::parse_midi(&bytes).unwrap();
                assert!(midi.tracks.iter().any(|track| !track.notes.is_empty()));
            }
            OutputFormat::Wav => {
                assert!(bytes.starts_with(b"RIFF"));
                assert_eq!(&bytes[8..12], b"WAVE");
            }
            _ => unreachable!(),
        }
    }
    let first_crop = library
        .crop_output(
            &engine,
            &OutputArgs {
                source: OutputSource::Job {
                    job_id: checked.job.job_id,
                },
                format: OutputFormat::Png,
                pages: None,
                width: Some(1200),
                pdf_password: None,
                png_quality: None,
                png_transparent: None,
                output_path: None,
                output_directory: None,
                crop: Some(OutputCrop::Measures {
                    range: MeasureRange { start: 1, end: 2 },
                    staff: None,
                }),
                padding_staff_spaces: Some(2.0),
                audio_range: None,
                include_drawing: None,
                wait_ms: None,
            },
            output_dir.path(),
            "first-crop",
            &RunContext::default(),
        )
        .unwrap();
    assert_eq!(first_crop.files.len(), 1);
    assert_eq!(first_crop.files[0].page, 1);
    let first_png = fs::read(&first_crop.files[0].path).unwrap();
    assert_eq!(&first_png[..8], b"\x89PNG\r\n\x1a\n");
    let first_sha = first_crop.files[0].sha256.clone();
    let second_crop = library
        .crop_output(
            &engine,
            &OutputArgs {
                source: OutputSource::Job {
                    job_id: checked.job.job_id,
                },
                format: OutputFormat::Png,
                pages: None,
                width: Some(1200),
                pdf_password: None,
                png_quality: None,
                png_transparent: None,
                output_path: None,
                output_directory: None,
                crop: Some(OutputCrop::Measures {
                    range: MeasureRange { start: 1, end: 2 },
                    staff: None,
                }),
                padding_staff_spaces: Some(2.0),
                audio_range: None,
                include_drawing: None,
                wait_ms: None,
            },
            output_dir.path(),
            "second-crop",
            &RunContext::default(),
        )
        .unwrap();
    assert_eq!(second_crop.files[0].sha256, first_sha);
    let old_svg_path = work.path().join("old.svg");
    let old_svg = library
        .output(
            &engine,
            &OutputArgs {
                source: OutputSource::Job {
                    job_id: original_job,
                },
                format: OutputFormat::Svg,
                pages: None,
                width: None,
                pdf_password: None,
                png_quality: None,
                png_transparent: None,
                output_path: Some(old_svg_path.to_string_lossy().into_owned()),
                output_directory: None,
                crop: None,
                padding_staff_spaces: None,
                audio_range: None,
                include_drawing: None,
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
                pages: None,
                width: Some(4097),
                pdf_password: None,
                png_quality: None,
                png_transparent: None,
                output_path: Some(invalid_path.to_string_lossy().into_owned()),
                output_directory: None,
                crop: None,
                padding_staff_spaces: None,
                audio_range: None,
                include_drawing: None,
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
fn native_silent_chord_annotation_matches_reference_and_crops() {
    let root = PathBuf::from(std::env::var_os("OSSIA_TEST_LILYPOND_ROOT").unwrap());
    let work = tempfile::tempdir().unwrap();
    let engine =
        Engine::verify(&root, &work.path().join("scratch"), &RunContext::default()).unwrap();
    let library = Library::open(&work.path().join("library")).unwrap();
    let base = "\\version \"2.26.0\"\n\
upper = { \\time 4/4 c'2 e'2 | }\n\
lower = { \\time 4/4 c2 g,2 | }\n\
\\score { \\new PianoStaff << \\new Staff = \"upper\" \\upper \\new Staff = \"lower\" \\lower >> \\layout { } \\midi { } }\n";
    let imported = library
        .import(
            &engine,
            ImportRequest {
                title: "Annotated",
                input: base.as_bytes(),
                format: InputFormat::Ly,
                confirmation: None,
                author: "test",
            },
            &RunContext::default(),
        )
        .unwrap();
    let document = imported.document.unwrap();
    let original_version = document.head_version_id.unwrap();
    let report = library
        .cached_evidence(
            document.document_id,
            &document.source_hash,
            engine.build_id(),
            EVIDENCE_ANALYSIS_VERSION,
        )
        .unwrap()
        .unwrap();
    let reference = ReferenceMusic {
        source: EvidenceSource::Version {
            version_id: original_version,
        },
        source_hash: report.source_hash,
        engine_build_id: report.engine_build_id,
        comparison: ComparisonMode::Performance,
        performance: report.music.performance,
    };
    let edit = library
        .begin_edit(BeginEditRequest {
            document_id: document.document_id,
            base_version_id: Some(original_version),
            expected_source_hash: &document.source_hash,
            author: "test",
            adopt_workspace: false,
        })
        .unwrap();
    let audible = base.replace(
        "\\new Staff = \"upper\" \\upper",
        "\\new Staff = \"upper\" << \\new ChordNames \\with { alignAboveContext = \"upper\" } \\chordmode { c1 } \\upper >>",
    );
    let written = library
        .write_source(&WriteSourceArgs {
            edit_id: edit.edit_id,
            expected_source_hash: document.source_hash.clone(),
            change: SourceWriteChange::Replace { text: audible },
            preview: false,
        })
        .unwrap();
    let failed = library
        .check(
            &engine,
            CheckRequest {
                expect: None,
                reference: Some(&reference),
                document_id: document.document_id,
                edit_id: Some(edit.edit_id),
                expected_source_hash: &written.source_hash,
                options: &CompileOptions {
                    formats: vec![RenderFormat::Svg, RenderFormat::Midi],
                    point_and_click: true,
                },
            },
            &RunContext::default(),
        )
        .unwrap();
    let failed: CheckResult = serde_json::from_value(failed.job.result.unwrap()).unwrap();
    assert_eq!(failed.verification.status, VerificationStatus::Failed);
    let silent_source = base.replace(
        "\\new Staff = \"upper\" \\upper",
        "\\new Staff = \"upper\" << \\new ChordNames \\with { alignAboveContext = \"upper\" \\remove \"Staff_performer\" } \\chordmode { c1 } \\upper >>",
    );
    let silent = library
        .write_source(&WriteSourceArgs {
            edit_id: edit.edit_id,
            expected_source_hash: written.source_hash,
            change: SourceWriteChange::Replace {
                text: silent_source,
            },
            preview: false,
        })
        .unwrap();
    let passed = library
        .check(
            &engine,
            CheckRequest {
                expect: None,
                reference: Some(&reference),
                document_id: document.document_id,
                edit_id: Some(edit.edit_id),
                expected_source_hash: &silent.source_hash,
                options: &CompileOptions {
                    formats: vec![RenderFormat::Svg, RenderFormat::Midi],
                    point_and_click: true,
                },
            },
            &RunContext::default(),
        )
        .unwrap();
    let passed_result: CheckResult = serde_json::from_value(passed.job.result.unwrap()).unwrap();
    assert_eq!(
        passed_result.verification.status,
        VerificationStatus::Passed
    );
    let batch = library
        .crop_output(
            &engine,
            &OutputArgs {
                source: OutputSource::Job {
                    job_id: passed.job.job_id,
                },
                format: OutputFormat::Png,
                pages: None,
                width: Some(1200),
                pdf_password: None,
                png_quality: None,
                png_transparent: None,
                output_path: None,
                output_directory: None,
                crop: Some(OutputCrop::Measures {
                    range: MeasureRange { start: 1, end: 1 },
                    staff: None,
                }),
                padding_staff_spaces: Some(2.0),
                audio_range: None,
                include_drawing: None,
                wait_ms: None,
            },
            work.path(),
            "annotation-crop",
            &RunContext::default(),
        )
        .unwrap();
    assert_eq!(batch.files.len(), 1);
    assert!(Path::new(&batch.files[0].path).is_file());
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
