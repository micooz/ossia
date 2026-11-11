# ossia-library

Local document library: immutable source snapshots, public `score.ly` files, editing ownership, checked commits and recoverable history. No GUI, network or task scheduler is required.

## Opening a library

`Library::open(home)` takes an explicit path, canonicalizes it and holds `library.lock` until dropped. Only the owning process opens the database for writing; a second opener receives `library-busy`. P04 owns runtime discovery and transports around this library.

Schema 1 uses `library.sqlite`, SQLite application ID `0x4f535349`, WAL, foreign keys and FULL synchronization. Unknown markers or schema versions return `runtime-incompatible`.

Public source is `documents/<documentId>/score.ly`. `original/input.ly`, `original/input.musicxml` or `original/input.mid` preserves the supplied input. Source bytes, including BOM and CRLF, are stored as gzip CAS objects in `contents/<SHA-256>.gz`; the hash addresses uncompressed bytes. Engine assets stay in the P02 namespace beneath `artifacts/`. Interrupted writes can leave unreferenced content or import directories; idle maintenance removes them.

## Storage maintenance

Schema 2 records job and render timestamps so maintenance can apply age and count
retention without deleting current sources or committed versions. The runtime
cleans regenerable data only: terminal jobs older than 30 days (or beyond 200 per
document), render records older than 90 days (or beyond 500 per document), stale
locks, abandoned imports, old scratch directories, and unreferenced content
objects. Every document keeps its latest 20 jobs and renders, its current render,
active edit receipts and open-view artifacts.

The default `artifacts/` cache budget is 1 GiB and can be set to 512 MiB,
3 GiB or 10 GiB. Exceeding the budget evicts the least-recently-used
non-protected render records and their assets until usage falls to 75% of the
limit. Version history, public source files and original imports are never
pruned automatically. A core data directory above 2 GiB produces a warning in
the management window. Automatic cleanup is enabled by default and can be
disabled independently from manual cleanup.

## Import and editing

The caller provides a verified `Engine` and a fresh `RunContext` per operation. `EngineBackend` is the seam for deterministic fault/concurrency adapters; production uses the existing native engine. Compilation runs outside the SQLite mutex.

- `import` returns an `ImportOutcome`. Refused or unconfirmed reports have no document. Retry losses with the exact confirmation hash. Converted input without a successful SVG is not published; a handwritten `.ly` with compiler diagnostics remains a repairable document with no head.
- A no-head document is dirty, but its initial snapshot is a known baseline for `begin_edit`. Unknown changes require `adopt_workspace: true` and the exact current hash. This attributes the workspace to a task without replacing the file.
- `begin_edit` returns the edit capability without a source filesystem path. `DocumentState` separately exposes the public source path for host diagnostics, while `read_source` and `write_source` provide the Agent editing boundary. P04 uses `get_edit(editId)` to verify ownerViewId without enumerating capabilities.
- `source_snapshot` reads stable UTF-8 bytes. `observe_source` recomputes state without creating versions. Missing, invalid or unstable source is reported without resetting it.
- `check` binds a successful SVG receipt to the active edit, exact source, engine build and options. A preview without editId does not authorize an edit commit. PDF/MIDI failures remain independent.
- `commit` requires that receipt, matching head and current bytes. It appends a version and ends the edit in one transaction without rewriting source. Unchanged source ends the task and returns the current version. A repaired no-head document receives its first `origin=import` version.

```no_run
use ossia_library::*;
use std::{fs, path::Path};

fn edit_example(home: &Path, engine: &Engine) -> Result<Version> {
    let library = Library::open(home)?;
    let imported = library.import(engine, ImportRequest {
        title: "Example",
        input: b"\\version \"2.26.0\"\n\\score { { c'1 } \\layout {} }\n",
        format: InputFormat::Ly,
        confirmation: None,
        author: "user",
    }, &RunContext::default())?;
    let doc = imported.document.ok_or_else(||
        AppError::new(ErrorCode::ImportConfirmRequired, "review import report"))?;
    let edit = library.begin_edit(BeginEditRequest {
        document_id: doc.document_id,
        view_id: ViewId::new(),
        base_version_id: doc.head_version_id,
        expected_source_hash: &doc.source_hash,
        author: "agent",
        adopt_workspace: false,
    })?;
    let page = library.read_source(&ReadSourceArgs {
        document_id: doc.document_id,
        view_id: edit.owner_view_id,
        expected_source_hash: doc.source_hash.clone(),
        start: Some(SourceReadStart::Start),
        max_lines: None,
        max_bytes: None,
    })?;
    let start = page.content.find("c'1").expect("fixture note");
    let span = ossia_score::SourceMap::new(&page.content)
        .span(start, start + 3)?;
    let written = library.write_source(&WriteSourceArgs {
        view_id: edit.owner_view_id,
        edit_id: edit.edit_id,
        expected_source_hash: doc.source_hash,
        change: SourceWriteChange::Edit {
            edits: vec![SourceEdit {
                span,
                expected_text: "c'1".into(),
                replacement: "d'1".into(),
            }],
        },
        preview: true,
    })?;
    library.check(engine, CheckRequest {
        expect: None,
        document_id: doc.document_id,
        edit_id: Some(edit.edit_id),
        expected_source_hash: &written.source_hash,
        options: &CompileOptions::default(),
    }, &RunContext::default())?;
    library.commit(CommitRequest {
        edit_id: edit.edit_id,
        base_version_id: edit.base_version_id,
        expected_source_hash: &written.source_hash,
        summary: "Raise the note to D",
    })
}
```

## P04 task integration

`queue_check(request)` captures source and persists a queued Job plus private edit/options parameters. Return its jobId to the caller, then schedule `run_check(engine, jobId, context)`. The worker uses `job_snapshot`, even if the public file changes while queued. `check` is the synchronous convenience wrapper over these same operations.

`create_job`, `get_job`, `job_snapshot` and `transition_job` persist other runtime tasks. Transitions compare the expected state, permit queued/running phase updates and cannot move a terminal job back to running. Runtime cancellation must also cancel the engine's `RunContext`; recording a terminal state alone does not stop native work. Cancellation after compilation starts cannot publish a result over an already terminal Job. Engine failures include jobId in the returned error detail.

A successful old check remains queryable by jobId, with `current=false`, and cannot replace the last current successful render or authorize a newer source. `DocumentState.lastCheck` addresses the current source's latest check/import job; its result contains diagnostics, including failed imports. `lastSuccessfulRender` may describe older source: compare its sourceHash with the document before presenting it as current.

## Ending, discarding and restoring

`end_edit` preserves the file. `discard(document, editId?, baseVersionId, expectedHash)` is a separate operation; an active/interrupted task requires its capability. Without a head there is no automatic reset target.

`restore` requires a clean workspace and no active/interrupted edit. It verifies the historical snapshot and SVG, then appends a new `origin=restore` node with a distinct versionId and seq, even when content repeats. `history` is ordered by seq; `version_snapshot` addresses an immutable version, never a mutable file.

File-changing operations first commit a worktree intent, then replace the public file using a synchronized same-directory temporary file, then finalize the database transaction. On open:

- Queued/running jobs become interrupted; active edits become interrupted and require `resume_edit` with the original capability and explicit new owner view. Successful old receipts are cleared so resumption requires a new check.
- A pending intent with the old file hash is cancelled. A target hash with the expected head finalizes the planned operation. Any other state preserves the file and reports `recovery-conflict` through the document's editState.
- `resolve_recovery_conflict(document, expectedHash)` explicitly acknowledges that conflict without changing source or head. Then resume an interrupted edit, adopt the unknown workspace or discard it as appropriate.

Application locking does not exclude arbitrary file tools. Expected hashes detect observed conflicts; a save after commit's final read remains a new dirty workspace. File rewrites cannot provide compare-and-swap against unrelated writers that ignore the protocol.

## Validation

```sh
cargo test -p ossia-library
OSSIA_TEST_LILYPOND_ROOT="$PWD/vendor/lilypond/2.26.0-darwin-arm64" \
OSSIA_NATIVE_EVIDENCE="$PWD/target/native-library.json" \
cargo test -p ossia-library --test native_library -- --ignored
```

Ordinary tests include a subprocess helper marked ignored; lock/crash regressions explicitly launch it and verify process death recovery. Native integration is opt-in and fails when resources are missing. The recorded native evidence covers macOS ARM64 only. Other operating systems, actual power loss, signed bundles and P04 runtime integration remain unverified. The workspace requires Rust 1.90; validation uses the installed Rust 1.98.1.
