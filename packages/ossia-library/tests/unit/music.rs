use crate::{test_support::Fixture, *};

#[test]
fn latest_explicit_check_wins_and_preview_cannot_authorize_commit() {
    let f = Fixture::new();
    let edit = f.begin();
    let hash = f.write("changed c'1");
    f.check(&edit, &hash);
    let expectation = MusicExpectation {
        note_count: Some(8),
        tracks: vec![],
    };
    let old = f
        .library
        .queue_check(CheckRequest {
            expect: None,
            document_id: f.doc.document_id,
            edit_id: Some(edit.edit_id),
            expected_source_hash: &hash,
            options: &CompileOptions::default(),
        })
        .unwrap();
    let latest = f
        .library
        .queue_check(CheckRequest {
            expect: Some(&expectation),
            document_id: f.doc.document_id,
            edit_id: Some(edit.edit_id),
            expected_source_hash: &hash,
            options: &CompileOptions::default(),
        })
        .unwrap();
    assert!(f.commit(&edit, &hash).is_err());
    f.library
        .run_check(&f.engine, latest.job_id, &RunContext::default())
        .unwrap();
    f.library
        .run_check(&f.engine, old.job_id, &RunContext::default())
        .unwrap();
    let result: CheckResult =
        serde_json::from_value(f.library.get_job(latest.job_id).unwrap().result.unwrap()).unwrap();
    assert_eq!(result.verification.status, VerificationStatus::Unavailable);
    f.library
        .check(
            &f.engine,
            CheckRequest {
                expect: None,
                document_id: f.doc.document_id,
                edit_id: None,
                expected_source_hash: &hash,
                options: &CompileOptions::default(),
            },
            &RunContext::default(),
        )
        .unwrap();
    assert!(f.commit(&edit, &hash).is_err());
    f.check(&edit, &hash);
    f.commit(&edit, &hash).unwrap();
}

#[test]
fn existing_schema_adds_check_binding_without_trusting_old_receipts() {
    let root = tempfile::tempdir().unwrap();
    let library = Library::open(root.path()).unwrap();
    drop(library);
    let conn = Connection::open(root.path().join("library.sqlite")).unwrap();
    conn.execute_batch("ALTER TABLE check_receipts DROP COLUMN job_id; ALTER TABLE check_requests DROP COLUMN edit_id; PRAGMA user_version=1;").unwrap();
    drop(conn);
    let library = Library::open(root.path()).unwrap();
    assert_eq!(
        library
            .db()
            .unwrap()
            .pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        1
    );
    assert!(library.list_documents().unwrap().is_empty());
}

#[test]
fn source_writes_require_owned_edit_hash_and_recover_after_replacement() {
    let f = Fixture::new();
    let edit = f.begin();
    let mut args = WriteSourceArgs {
        view_id: edit.owner_view_id,
        edit_id: edit.edit_id,
        expected_source_hash: f.doc.source_hash.clone(),
        change: SourceWriteChange::Replace {
            text: "new c'1".into(),
        },
    };
    args.view_id = ViewId::new();
    assert_eq!(
        f.library.write_source(&args).unwrap_err().code,
        ErrorCode::Forbidden
    );
    args.view_id = edit.owner_view_id;
    args.expected_source_hash = ContentHash::of(b"stale");
    assert_eq!(
        f.library.write_source(&args).unwrap_err().code,
        ErrorCode::Conflict
    );
    args.expected_source_hash = f.doc.source_hash.clone();
    let state = f.library.write_source(&args).unwrap();
    let SourceWriteChange::Replace { text } = &args.change else {
        unreachable!()
    };
    assert_eq!(state.source_hash, ContentHash::of(text.as_bytes()));
    assert!(state.dirty);
    args.expected_source_hash = state.source_hash;
    args.change = SourceWriteChange::Replace {
        text: "after crash c'1".into(),
    };
    f.hook("after-replace", |_| Err(storage_error("interrupted write")));
    assert!(f.library.write_source(&args).is_err());
    let home = f.library.home().to_owned();
    drop(f.library);
    let library = Library::open(&home).unwrap();
    assert_eq!(
        library.source_snapshot(f.doc.document_id).unwrap().source(),
        "after crash c'1"
    );
    assert_eq!(library.history(f.doc.document_id).unwrap().len(), 1);
}

#[test]
fn source_reads_page_by_lines_spans_and_utf8_boundaries() {
    let source = "\u{feff}first\r\n第二行\nthird line\nfourth";
    let f = Fixture::source(source);
    let line = f
        .library
        .read_source(&ReadSourceArgs {
            document_id: f.doc.document_id,
            view_id: ViewId::new(),
            expected_source_hash: f.doc.source_hash.clone(),
            start: Some(SourceReadStart::Line { line: 2 }),
            max_lines: Some(1),
            max_bytes: None,
        })
        .unwrap();
    assert_eq!(line.content, "第二行\n");
    assert!(!line.complete);
    assert_eq!(line.next.unwrap().start.line, 3);
    assert_eq!(line.span.start.line, 2);
    assert_eq!(line.total_lines, 4);
    assert_eq!(
        f.library
            .read_source(&ReadSourceArgs {
                document_id: f.doc.document_id,
                view_id: ViewId::new(),
                expected_source_hash: f.doc.source_hash.clone(),
                start: Some(SourceReadStart::Line { line: 99 }),
                max_lines: None,
                max_bytes: None,
            })
            .unwrap_err()
            .code,
        ErrorCode::InvalidInput
    );

    let map = ossia_score::SourceMap::new(source);
    let exact = map
        .span(source.find("第二").unwrap(), source.find("行").unwrap())
        .unwrap();
    let span = f
        .library
        .read_source(&ReadSourceArgs {
            document_id: f.doc.document_id,
            view_id: ViewId::new(),
            expected_source_hash: f.doc.source_hash.clone(),
            start: Some(SourceReadStart::Span { span: exact }),
            max_lines: None,
            max_bytes: None,
        })
        .unwrap();
    assert_eq!(span.content, "第二");
    assert_eq!(span.span, exact);
    assert!(span.complete);
    let mut invalid_span = exact;
    invalid_span.start.line += 1;
    assert_eq!(
        f.library
            .read_source(&ReadSourceArgs {
                document_id: f.doc.document_id,
                view_id: ViewId::new(),
                expected_source_hash: f.doc.source_hash.clone(),
                start: Some(SourceReadStart::Span { span: invalid_span }),
                max_lines: None,
                max_bytes: None,
            })
            .unwrap_err()
            .code,
        ErrorCode::InvalidInput
    );

    let first = f
        .library
        .read_source(&ReadSourceArgs {
            document_id: f.doc.document_id,
            view_id: ViewId::new(),
            expected_source_hash: f.doc.source_hash.clone(),
            start: None,
            max_lines: None,
            max_bytes: Some(6),
        })
        .unwrap();
    assert_eq!(first.content, "\u{feff}fir");
    assert!(!first.complete);
    assert_eq!(first.next.unwrap().byte_start, first.span.byte_end);

    let stale = f
        .library
        .read_source(&ReadSourceArgs {
            document_id: f.doc.document_id,
            view_id: ViewId::new(),
            expected_source_hash: ContentHash::of(b"stale"),
            start: None,
            max_lines: None,
            max_bytes: None,
        })
        .unwrap_err();
    assert_eq!(stale.code, ErrorCode::Conflict);

    let unicode = Fixture::source("中");
    let scalar = unicode
        .library
        .read_source(&ReadSourceArgs {
            document_id: unicode.doc.document_id,
            view_id: ViewId::new(),
            expected_source_hash: unicode.doc.source_hash.clone(),
            start: None,
            max_lines: None,
            max_bytes: Some(1),
        })
        .unwrap();
    assert_eq!(scalar.content, "中");
    assert_eq!(scalar.returned_bytes, "中".len());
    assert!(scalar.complete);

    let empty = Fixture::source("");
    let empty_page = empty
        .library
        .read_source(&ReadSourceArgs {
            document_id: empty.doc.document_id,
            view_id: ViewId::new(),
            expected_source_hash: empty.doc.source_hash.clone(),
            start: None,
            max_lines: None,
            max_bytes: None,
        })
        .unwrap();
    assert!(empty_page.content.is_empty());
    assert!(empty_page.complete);
    assert_eq!(empty_page.total_lines, 1);
}

#[test]
fn source_edits_are_validated_together_and_applied_in_one_replacement() {
    let source = "alpha c4 omega\nsecond c4\n";
    let f = Fixture::source(source);
    let edit = f.begin();
    f.check(&edit, &f.doc.source_hash);
    let map = ossia_score::SourceMap::new(source);
    let first = map.span(6, 8).unwrap();
    let second = map
        .span(source.rfind("c4").unwrap(), source.len() - 1)
        .unwrap();
    let args = WriteSourceArgs {
        view_id: edit.owner_view_id,
        edit_id: edit.edit_id,
        expected_source_hash: f.doc.source_hash.clone(),
        change: SourceWriteChange::Edit {
            edits: vec![
                SourceEdit {
                    span: second,
                    expected_text: "c4".into(),
                    replacement: "g'4".into(),
                },
                SourceEdit {
                    span: first,
                    expected_text: "c4".into(),
                    replacement: "c'4".into(),
                },
            ],
        },
    };
    let state = f.library.write_source(&args).unwrap();
    assert_eq!(
        f.commit(&edit, &state.source_hash).unwrap_err().code,
        ErrorCode::CompileFailed
    );
    assert_eq!(
        f.library
            .source_snapshot(f.doc.document_id)
            .unwrap()
            .source(),
        "alpha c'4 omega\nsecond g'4\n"
    );
    assert_eq!(
        state.source_hash,
        ContentHash::of(b"alpha c'4 omega\nsecond g'4\n")
    );

    let current = f.library.source_snapshot(f.doc.document_id).unwrap();
    let current_map = ossia_score::SourceMap::new(current.source());
    let overlap_first = current_map.span(0, 5).unwrap();
    let overlap_second = current_map.span(3, 8).unwrap();
    let mut invalid = args;
    invalid.expected_source_hash = state.source_hash;
    invalid.change = SourceWriteChange::Edit {
        edits: vec![
            SourceEdit {
                span: overlap_first,
                expected_text: "alpha".into(),
                replacement: "ALPHA".into(),
            },
            SourceEdit {
                span: overlap_second,
                expected_text: current_map.slice(overlap_second).unwrap().into(),
                replacement: "HERE".into(),
            },
        ],
    };
    assert_eq!(
        f.library.write_source(&invalid).unwrap_err().code,
        ErrorCode::InvalidInput
    );
    assert_eq!(
        f.library
            .source_snapshot(f.doc.document_id)
            .unwrap()
            .source(),
        "alpha c'4 omega\nsecond g'4\n"
    );
}

#[test]
fn source_edits_support_insert_delete_and_reject_ambiguous_targets() {
    let source = "remove\nkeep\n";
    let f = Fixture::source(source);
    let edit = f.begin();
    let map = ossia_score::SourceMap::new(source);
    let state = f
        .library
        .write_source(&WriteSourceArgs {
            view_id: edit.owner_view_id,
            edit_id: edit.edit_id,
            expected_source_hash: f.doc.source_hash.clone(),
            change: SourceWriteChange::Edit {
                edits: vec![
                    SourceEdit {
                        span: map.span(0, "remove\n".len()).unwrap(),
                        expected_text: "remove\n".into(),
                        replacement: String::new(),
                    },
                    SourceEdit {
                        span: map.span(source.len(), source.len()).unwrap(),
                        expected_text: String::new(),
                        replacement: "inserted\n".into(),
                    },
                ],
            },
        })
        .unwrap();
    assert_eq!(
        f.library
            .source_snapshot(f.doc.document_id)
            .unwrap()
            .source(),
        "keep\ninserted\n"
    );

    let current = f.library.source_snapshot(f.doc.document_id).unwrap();
    let current_map = ossia_score::SourceMap::new(current.source());
    let invalid = WriteSourceArgs {
        view_id: edit.owner_view_id,
        edit_id: edit.edit_id,
        expected_source_hash: state.source_hash,
        change: SourceWriteChange::Edit {
            edits: vec![
                SourceEdit {
                    span: current_map.span(0, 0).unwrap(),
                    expected_text: String::new(),
                    replacement: "a".into(),
                },
                SourceEdit {
                    span: current_map.span(0, 0).unwrap(),
                    expected_text: String::new(),
                    replacement: "b".into(),
                },
            ],
        },
    };
    assert_eq!(
        f.library.write_source(&invalid).unwrap_err().code,
        ErrorCode::InvalidInput
    );
    assert_eq!(
        f.library
            .source_snapshot(f.doc.document_id)
            .unwrap()
            .source(),
        "keep\ninserted\n"
    );
}

#[test]
fn source_edits_reject_stale_text_excess_ranges_and_oversized_results() {
    let source = "a";
    let f = Fixture::source(source);
    let edit = f.begin();
    let span = ossia_score::SourceMap::new(source).span(0, 1).unwrap();
    let mut args = WriteSourceArgs {
        view_id: edit.owner_view_id,
        edit_id: edit.edit_id,
        expected_source_hash: f.doc.source_hash.clone(),
        change: SourceWriteChange::Edit {
            edits: vec![SourceEdit {
                span,
                expected_text: "stale".into(),
                replacement: "b".into(),
            }],
        },
    };
    assert_eq!(
        f.library.write_source(&args).unwrap_err().code,
        ErrorCode::Conflict
    );
    assert_eq!(
        f.library
            .source_snapshot(f.doc.document_id)
            .unwrap()
            .source(),
        source
    );

    args.change = SourceWriteChange::Edit {
        edits: (0..65)
            .map(|_| SourceEdit {
                span,
                expected_text: source.into(),
                replacement: String::new(),
            })
            .collect(),
    };
    assert_eq!(
        f.library.write_source(&args).unwrap_err().code,
        ErrorCode::InvalidInput
    );

    args.change = SourceWriteChange::Edit {
        edits: vec![SourceEdit {
            span: ossia_score::SourceMap::new(source).span(0, 0).unwrap(),
            expected_text: String::new(),
            replacement: "x".repeat(1024 * 1024 + 1),
        }],
    };
    assert_eq!(
        f.library.write_source(&args).unwrap_err().code,
        ErrorCode::InvalidInput
    );
    assert_eq!(
        f.library
            .source_snapshot(f.doc.document_id)
            .unwrap()
            .source(),
        source
    );
}
