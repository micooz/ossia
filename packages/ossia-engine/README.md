# ossia-engine

Synchronous native LilyPond conversion and snapshot compilation. Callers supply prepared resources, scratch and artifact roots, cancellation and deadlines. No database, GUI or Node.js runtime is required.

## P03 interface

```no_run
use ossia_engine::*;
use std::path::Path;

fn check(
    engine_root: &Path,
    scratch_root: &Path,
    artifact_root: &Path,
    document_id: DocumentId,
    source_bytes: Vec<u8>,
) -> Result<RenderManifest, AppError> {
    let engine = Engine::verify(engine_root, scratch_root, &RunContext::default())?;
    let snapshot = SourceSnapshot::new(document_id, source_bytes)?;
    let manifest = engine.compile(
        &snapshot,
        &CompileOptions::default(),
        artifact_root,
        &RunContext::default(),
    )?;
    // A successful SVG result is the compilation gate; P03 must also check ownership/head/source.
    if manifest.pages.is_empty() {
        return Err(manifest.format_errors.first()
            .map(|failure| failure.error.clone())
            .unwrap_or_else(|| AppError::new(ErrorCode::CompileFailed, "no SVG preview")));
    }
    Ok(manifest)
}
```

- `Engine::verify(root, scratch_root, context)` validates the embedded target lock against `resources.json`, hashes every immutable resource, checks tools and versions, and performs an SVG smoke compile. Prepare the root with the development installer before calling it. `Engine::from_resources` uses `<bundle resources>/engine`; the executable owner resolves the bundle path. There is no PATH/cwd discovery or automatic repair in the Rust core.
- `convert(bytes, InputFormat, confirmation, context)` returns `ImportReport`. `clean` is usable; `needs-confirmation` requires an explicit second call with the report's confirmation hash and a `confirmed` result before P03 creates a document. The hash binds the input, converted source, engine build and normalized losses. `refused` exposes no source; `Negative skip` cannot be confirmed. Preserve original bytes separately in P03. Index counts are diagnostic and may be incomplete for opaque valid syntax.
- `compile(snapshot, options, artifact_root, context)` returns a manifest containing successful artifacts and independent `formatErrors`. Request SVG for the commit gate, PDF/MIDI for exports. A format failure does not invalidate another format. Global cancellation, deadline, validation and artifact-store failures return `Err`.
- `RunContext::new(timeout)` sets one deadline for all passes and cache waits in that operation. Clone its `cancellation` handle to cancel from another thread. Use a new context per operation. Runtime scheduling belongs to P04.
- Cache keys include the exact original source hash, locked build identity, normalized options and format. The engine creates `assets/`, `renders/` and `locks/` beneath the supplied `artifact_root`; keep that namespace root stable across restarts. Artifact references are relative to it, with byte counts and SHA-256. Cache manifests are published after assets; concurrent same-key compilers use OS file locks. Missing/damaged files rebuild on access. Orphan content after interruption is safe to collect later; no document or database writes occur here.

## Source and diagnostic boundaries

Single-file validation accepts static includes confined to the engine's standard `ly` tree. User includes, dynamic includes and known external image/file commands are unsupported. This is not a Scheme sandbox: source is trusted local executable input, per the architecture decision.

MIDI derivation supports one explicit literal `score` block. Existing literal `midi` blocks are retained; otherwise an empty block is inserted at the balanced score boundary. No score, multiple scores/books, bookparts, or opaque Scheme reader constructs can make MIDI unavailable. SVG/PDF can compile ordinary implicit-score input. Ambiguous multi-output groups and noncontiguous SVG pages are refused. A MIDI result must contain parsed note-on events.

Point-and-click source overrides are replaced with equal-length whitespace only in a derived file. Inserted MIDI text has no original source position; unchanged segments map byte-for-byte. SVG links are normalized to `textedit://score.ly:line:char:column`; library/inserted-text links are removed. Diagnostics retain sanitized raw locations plus optional original byte/scalar positions. Library diagnostics never point into user source.

Subprocesses use a private cwd, writable HOME/cache/tmp directories, UTF-8 C locale and no inherited Python/Guile module overrides. Calls pass arguments directly to the native executable or bundled Python. Unix process groups and Windows kill-on-close jobs terminate descendants on every exit path. The deadline and 16 MiB log limit control runaway tools. Signed resources remain read-only.

## Validation

```sh
pnpm engine:install
pnpm test:engine
cargo test -p ossia-engine
OSSIA_TEST_LILYPOND_ROOT="$PWD/vendor/lilypond/2.26.0-darwin-arm64" \
OSSIA_NATIVE_EVIDENCE="$PWD/target/native-engine.json" \
cargo test -p ossia-engine --test native_engine -- --ignored
```

Ordinary tests explicitly ignore native integration and two subprocess helper entrypoints. The process-tree regression invokes those helpers itself and verifies real descendants terminate. Explicit native integration fails if the supplied resources are absent or invalid. It relocates the complete tree into a Chinese/space path, exercises conversions, all formats, original locations, restart cache hits and damage recovery.

macOS ARM64 native validation is recorded in the P02 handoff. Windows code is cross-checked but has not run on Windows. Linux ARM64, other native targets, signed bundles and final distribution remain unverified. Toolchain recipes and dependency pins are described in `vendor/README.md`.
