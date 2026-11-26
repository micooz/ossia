# Verification

Run the playback, audition and selection suites without a library or server:

```sh
pnpm verify
```

The combined command generates one render bundle per score source. Playback,
audition and selection checks share its SVG, MIDI and geometry data.

Pass case IDs to run all three checks for those scores:

```sh
pnpm verify bach-goldberg-aria
```

Run a suite on all cases or select case IDs:

```sh
pnpm verify:playback
pnpm verify:playback xinhu-ledger bittersweet-ties
pnpm verify:audition
pnpm verify:audition sor-original-locations
pnpm verify:selection
pnpm verify:selection doraemon-original bach-sheep-four-hands sor-original-locations
```

## MusicXML export

Run `pnpm verify:musicxml` with the installed LilyPond engine. The suite checks
complete scores and focused excerpts covering grace notes, cross-staff voices,
guitar tablature, chord diagrams, mid-measure repeats, and alternative endings.

Set `OSSIA_TEST_MUSICXML_XSD` to a local MusicXML 4.0 schema with its imports
resolved locally to enable offline schema validation with `xmllint`.

## Playback fixtures

Each case in `tests/fixtures/playback/cases.json` references a LilyPond source
derived from a reported issue. Prefer a complete score with personal metadata
removed, and verify that redaction preserves the relevant behavior. Use small
independent excerpts for distinct musical rules or isolated diagnosis. A passing
case requires the full-score structural audit and its declared checkpoints to
pass. The command builds the current engine, renders each source with the
installed LilyPond, and audits sounding-note coverage, notehead associations,
timed groups and playhead movement across every page. Any finding fails the
suite, including cases with an empty `checks` array. Manual checkpoints add
independent pitch, timing and source-position expectations; they must not be
copied from current output.
CI installs the locked LilyPond engine on macOS and runs `pnpm verify`.

To add a case, add a self-contained `.ly` source and an entry in `cases.json`.
Confirm the case fails for the reported problem before applying its fix; retain
the original score where necessary to reproduce the issue.
For `mapped` or `playhead` checks, specify `time` in seconds, MIDI `pitch`, and
a source `needle` that occurs exactly once. `offset` is the character offset
within that needle. Optional `continuation` and `continuationOffset` require
both ends of a tie to be in the same group; `duration` checks the sounding
duration. A `rest` check specifies `time`, `duration`, `needle` and `offset` for
a silent rest. A `forward` check verifies that positions exist and do not move
backwards within a time range. Use `playhead` only when the chosen notehead is
expected to intersect the line at that time; simultaneous voices may choose
another valid notehead. Keep browser tests for interactions, scheduling, and
DOM rendering that the fixture runner does not exercise.

## Individual audition

`pnpm verify:audition` renders every case in `cases.json`.
Explicit case IDs select a subset. Complete scores cover the local library's
piano, guitar, ukulele, violin and ensemble examples; focused excerpts retain
checks for individual musical constructs. It checks every visible notehead for a
clickable hit region, one timed single-pitch group, a matching sounding attack
and reachability through the page's ordinary arrow-key navigation. Tied
continuations may share their attack's group. Timed groups with no pitches are
intentionally silent notation, such as dead notes; they must remain clickable
and keyboard-reachable, and are counted separately. Coincident unisons are counted
separately when they occupy identical bounds and produce the same pitch and
onset. Every sounding onset also compares the combined audition's pitches and
counts with the MIDI attacks, including independent unison performers.

The command also runs Chrome against the generated SVG, geometry and playback
data. It clicks every notehead on every page, walks each voice forwards and
backwards through notes and rests, checks selection highlighting, and compares
Alt/Option-click against simultaneous MIDI attacks. It records calls to the
real audio player while allowing playback to run, including duplicate-attack detection across notation
and TAB. This checks scheduled pitches, not the sound heard through speakers.
A separate browser test advances a controlled audio clock through every group
onset and checks the displayed playhead against the audited trace across every
page and system. Sound output is stubbed for this positioning test. Independent
fixture checkpoints and the structural audit validate the trace itself.
Browser cases run in parallel after all data audits pass.
Install Chrome with `pnpm --filter @ossia/ui exec playwright install chrome`.

To audit the current library without changing scores:

```sh
pnpm audit:audition
pnpm audit:audition -- <document-id>
pnpm audit:audition -- --base=http://127.0.0.1:3211/api/v1/
pnpm audit:audition -- --input=path/to/playback-fixture.json
```

The JSON report includes every failed notehead's page, source location and
reason. Any finding or failed request makes the command fail. This data audit
uses the page's hit testing and navigation helpers; it does not run a browser.
Rebuild and restart the backend after Rust changes. Engraved event metadata
requires a render from the current engine.

## Measure selection

The selection suite renders the same fixtures and visits staff whitespace
between engraved barlines on every page. It tests the browser's hit function,
then checks that each selected measure resolves to source spans covering its
engraved notes and rests. Declared measure counts and staff counts also check
engraved order, including pickups and alternate endings. The report counts one
cell for each staff and engraved measure interval.

Both selection commands fail on any finding. `pnpm verify:selection` uses the
committed fixtures. `pnpm audit:selection` reads the active documents from the
development backend's project library at `.ossia` and renders each local source
in a temporary directory. Pass document IDs to audit those files directly.

```sh
pnpm audit:selection
pnpm audit:selection <document-id> [<document-id> ...]
```

## Playback audit

Start the development backend from the current checkout, then run:

```sh
pnpm audit:playback > .ossia/playback-audit-report.json
pnpm audit:playback -- <document-id>
pnpm audit:playback -- --base=http://127.0.0.1:3211/api/v1/
pnpm audit:playback -- --expect=path/to/checked-onsets.json
```

The command reads the current render of each score through the local HTTP API;
it does not modify scores. The backend must be rebuilt and restarted after Rust
changes. The JSON report records the title, onset coverage, source locations,
coordinates and times. Any finding, untimed group, failed request or
incompatible snapshot makes the command fail.
`trace` records the selected position at each group onset and the held position
when locating fails. Compare it with independently checked score examples when
validating a mapping change.

For independently checked examples, `--expect` accepts a JSON object keyed by
document ID. Each value has the exact `sourceHash` and a `checkpoints` array of
`{ "time": 0, "page": 1, "system": 0, "sourceByteStart": 123 }` objects.
Times are seconds; source offsets are zero-based UTF-8 bytes. The expected
source offset must belong to the selected note at that onset. A stale source or
an omitted expected document is an error; a mismatch is an issue. Expectations
must be established from independent evidence, not copied from the audit trace.

- `unlocated`: the same locator used by the page cannot find one unambiguous
  position for the chosen group; playback holds the previous position.
- `backward`: the chosen position moved backwards in score order. A repeat,
  alternate ending, grace note or polyphony may make this legitimate; inspect
  the score and timing before calling it a bug.
- `unmapped-note`: a sounding note has no group with the same pitch and onset.
- `unmapped-head`: a visible notehead belongs to no playback group. Silent
  notation, ornaments and unsupported notation need individual review.
- `out-of-range-group`: a timed group starts at the playback endpoint or extends
  beyond it, including a trailing rest omitted from the total duration.
- `multiple-positions`: one group contains noteheads at different positions.
  Ties and displaced chord noteheads commonly do this correctly; inspect the
  source and the note-to-time correspondence before treating it as an error.

These are diagnostic checks, not a musical oracle. A pitch/onset match does not
prove that a note was matched to the correct occurrence, nor that MIDI matches
the intended score. Simultaneous voices need not all move the playhead. The
audit does not exercise the browser, audio output, seeking, repeats or the
visible animation in real time. Verify confirmed failures with independent
score or MIDI evidence and add a focused regression test before fixing them.
