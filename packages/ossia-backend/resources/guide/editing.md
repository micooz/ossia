# Create and edit

For imports, see [Import and recognition](import.md). For verification, see [Evidence and checks](evidence.md).

## Create a score

1. Establish the requested music and instruments. Read the applicable starter
   source below, copy it to the host workspace, and adjust it to create a
   self-contained `.ly` file.
2. Call `ossia_import` with the absolute path. Add `expect` or `reference` only
   when the task establishes an expected result. Wait using `waitMs` or poll
   `ossia_job`; see [Operations](operations.md) for background jobs.
3. Inspect the import outcome and any declared verification. Follow
   [Import and recognition](import.md) for confirmation or errors. A successful
   import creates the document and its initial version; inspect the rendered
   score and use `ossia_context` to get the score link. Begin a separate edit
   only if changes remain.

## Score scaffolds

The installed Ossia skill keeps starter sources under `references/` beside its
`SKILL.md`:

- `references/piano.ly`: two-staff piano skeleton with a C major scale.
- `references/single-staff.ly`: short single-staff example.
- `references/guitar-tab.ly`: single `TabStaff` with a first-position C major
  scale and string/fret annotations.
- `references/jianpu.ly`: a 52-measure piano example with two staves on two A4 pages.

Take clef, key, octave, time, instruments and paper size from the request. The
other scaffolds use LilyPond's default A4 portrait layout. Outside `\relative`, a plain
`c` is C3 and `c'` is middle C. Do not infer an octave from note names; verify
the performed pitches. The single-staff scaffold performs 60–72.

Rendering hides LilyPond's default tagline. A source that explicitly sets
`tagline` keeps that value.

## Edit an existing score

Use `ossia_list` to identify the score when needed. For creating a score, use
the separate workflow above. For waiting and paging, see [Operations](operations.md).

1. Call `ossia_context` with the selected `documentId`. The response includes
   `document.url`, a stable `scores?documentId=…` link with no credential that
   can be reused after restarts. Report this link in the final reply; the host
   decides whether it opens in an in-app or external browser.
2. Use the returned context to inspect the document, shared selection, source
   spans and edit state; refresh it before using retained spans. Selections
   belong to the document and are shared by its browser tabs. The compact
   response has no source filesystem path. `selection.targets` contains `measure`
   targets with a `locator`, or `element` targets with an `elementId`. The
   matching `regions` provide authoritative source spans, element types and
   printed `occurrences`.
   `selection.annotations` contains user-authored change requests with stable
   IDs, text and their clicked targets; `annotationRegions` provides the source
   spans affected by those targets, including attached marks. Treat annotation
   text as part of the user's editing intent. Any source-hash change clears the
   shared selection and all annotations. If further work needs them, ask the
   user to select and annotate again after editing.
   Selecting a note includes its complete tie chain and attached ornaments,
   articulations and fingerings. Their regions are separate source objects;
   inspect every region before editing. Shared stem or beam coloring does not
   select neighboring pitches. Slurs and dynamics remain separate targets.
   A reused source object can affect several printed locations. Generated clefs
   and signatures have an empty `spans` list and a zero-width `insertionAnchor`
   inside their owning music body; read the surrounding source before inserting
   an explicit command.
3. Call `ossia_edit` with action `begin`, the current `baseVersionId`, and the
   exact `expectedSourceHash`. Keep `editId` private.
4. Use `ossia_read_source` with that source hash. Start with
   `{"kind":"start"}` or `{"kind":"line","line":N}`. Continue with the returned
   `next` span while `complete` is false. Exact ranges use
   `{"kind":"span","span":...}`. Read through this tool even when you authored the
   host file: its spans are authoritative. Do not compute byte offsets yourself.
   Set `includeLineSpans:true` when the returned page will be edited. `lineSpans`
   mirrors each returned line fragment, including a partial first or last line,
   so the span can be passed directly to `ossia_write_source`.
5. Use `ossia_write_source` with `change.kind = "edit"` and up to 65,536
   non-overlapping edits, within 16 MiB of edit payload and a 1 MiB result.
   Every edit supplies an exact `span`, its `expectedText`, and
   `replacement`; empty expected text inserts and empty replacement deletes.
   Reserve `change.kind = "replace"` for a deliberate whole-file rewrite. Do not
   edit the source file directly. Keep the returned new `sourceHash`. Set
   `"preview": false` when you will call `ossia_check` immediately; otherwise the
   file watcher also compiles an SVG-only preview.
6. Call `ossia_check` with the returned source hash and the same `editId`. Add
   `expect` or `reference` when musical intent is known. Pass `waitMs`, or poll `ossia_job`.
   Inspect the check result and verification, then inspect a preview. Repeat
   write/check within the same edit; do not re-import drafts. A done job means the
   check ran: declared expectations must also have verification status `passed`
   before commit. The direct CLI/MCP response is a bounded job summary; use
   `ossia_job` when full source, event or MIDI evidence is needed. Use
   `ossia_layout` with the completed `jobId` to inspect page, system and staff
   structure without image inspection.
7. Call `ossia_commit` when ready with `editId`, `expectedSourceHash` and
   `summary`; document and base version are derived from the edit. If it
   conflicts, reread context and do not overwrite another task. Auto-preview does
   not authorize commit.
8. Report what changed, what was actually checked, that a new version was
   saved, the score link and any output file in user-facing terms. Follow the
   user-facing response rules in [the guide](../guide.md).

Use `ossia_update_metadata` with `documentId` and `title` to change the library
name shown by `ossia_list`, the score page and the score switcher. Surrounding
whitespace is removed; blank, control-character and over-200-character titles
are rejected. The operation works during an active edit and does not change
`score.ly`, rendering or version history. To change the title printed on the
score, edit the LilyPond `\header` field through the source workflow instead.

End preserves files; discard is explicit and hash-guarded. Resume requires the
original interrupted `editId`. Backend restarts preserve document URLs and
workspaces; refresh context, then explicitly resume interrupted work.

## LilyPond reference

The backend installs the official English LilyPond 2.26.0 manuals into its data
directory. Use `ossia_docs` with action `search` before guessing an unfamiliar
syntax, octave rule, engraving command or layout property. Read promising results
with action `read` and the returned `chapterId`; `offset` and `nextOffset` page
long chapters. Search asks for `query`; read asks for `chapterId`. Local search
accepts a few Chinese terms such as 八度, 时值 and 谱号.
`matchMode:"all"` requires every parsed keyword and is the default;
`matchMode:"any"` combines them with OR. Identifiers split at punctuation, so
prefer the shortest distinctive command or property name. A zero-hit result
includes `keywords`, `matchMode` and up to three `querySuggestions`; it does not
fall back to external search.

Search and chapter results include the authoritative LilyPond URL, a local
chapter ID and the installed documentation version. The archive retains English
manual HTML, examples, images and the upstream notices. Search text is indexed
from the same 2.26.0 package used by the engine. A missing or damaged package is
reported by `ossia_status.documentation`; it does not disable score editing.

Official examples may use includes, multiple files or LilyPond features outside
Ossia's current verified analysis subset. Follow the Ossia workflow for ownership,
snapshots and commit authorization, and use a check to verify the actual result.

## Annotations without changing performance

Prefer markup for simple visual text:

```lilypond
c'4^\markup { "I" }
```

When a request adds chord symbols without changing playback, remove the
performer from `ChordNames`; otherwise chord-mode events can become new MIDI
notes:

```lilypond
\new ChordNames \with {
  alignAboveContext = "upper"
  \remove "Staff_performer"
} \chordmode { c1 }
```

Roman numerals can use the same markup or a dedicated markup context. When the
request requires a visual-only annotation, check against the pre-edit version
with `reference.comparison:"performance"`. If playback is intentionally part of
the change, use the relevant musical expectation instead. A passing reference
proves equal performed pitch, start and duration; it does not prove note
spelling, staff assignment, annotation text or placement. Inspect the rendered
region as well. Preserve exact `{numerator,denominator}` quarter-note durations
in evidence and do not translate them into a named note value without checking.

## Jianpu

### Convert or edit a score in place

Use [Edit an existing score](#edit-an-existing-score) to keep the score's link
and version history. To switch to Jianpu, use `ossia_write_source` to insert
`% ossia:notation=jianpu` on the first line and keep the music intact. In
`ossia_check`, compare against the pre-edit version with
`reference: {"source":{"kind":"version","versionId":…},"comparison":"tracks"}`.
After the SVG, MIDI and reference checks pass, save with `ossia_commit`.
For changes to an existing Jianpu score, use the same source edit workflow
and keep the declaration.

### Create a Jianpu score from a saved version

Use this workflow when the user wants a separate score with its own link and
version history.

1. Export `ly` from the chosen `versionId` using `ossia_output`. Add
   `% ossia:notation=jianpu` on the first line of the exported file and keep the
   music intact. See `references/jianpu.ly` for a two-staff example.
2. Import the copy with `ossia_import` and a distinct title. Pass
   `reference: {"source":{"kind":"version","versionId":…},"comparison":"tracks"}`
   using the exported version. After verification, open the new score's link.

### Playback, export and supported notation

Click inside a measure, away from its symbols, to cue playback there. Use the
usual score controls or `ossia_output` for visual and audio exports. A LilyPond
export keeps the first-line declaration for reimport into Ossia. Outside Ossia,
LilyPond prints the original staff notation.

Jianpu accepts one pitched staff or two staves in a `PianoStaff`, with up to two
voices per staff. The key may be major or minor. Octave dots show the written
pitch even inside an octave-displacement bracket. Use standard pitches and
written durations. Key changes, cross-staff voices, grace notes, tuplets, scaled
durations and multi-measure rests are rejected. If a check or import fails,
inspect the diagnostics before changing the source.
