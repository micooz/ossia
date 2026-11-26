# Rendering and output

For version history, see [History and deletion](library.md). For musical evidence,
see [Evidence and checks](evidence.md). For job polling, see [Operations](operations.md).

## Source locations

Locations use UTF-8 byte spans, one-based pages/measures, zero-based staves and
SVG user units.

## Rendered structure

`ossia_layout` accepts a completed import or check `jobId` and returns
`pageCount`, `systemCount`, `staffCount`, and per-page `systems[].staves` with
bounds in SVG user units. Each system also reports `bars`, `measureStart` and
`measureEnd` when the source measure grid is reliable. Use these fields instead
of counting measures in a PNG. `measureMapping` is `complete`, `partial` or
`none`; inspect the page reason when it is not complete. `ossia_layout` does not
identify clefs, keys or time signatures; read the source and docs for those.

## Output files

Call `ossia_output` with a `source` of `{"kind":"job","jobId":…}` or
`{"kind":"version","versionId":…}` and a `format` of `ly`, `svg`, `png`, `midi`,
`pdf`, `wav` or `musicxml`, then poll the job. The result includes absolute
file paths, format, size and source provenance. Use `ossia_job` with `detail:"full"`
to inspect file hashes and media types. Read output files with the host's
file/image tool. Job outputs use the captured import/check snapshot; version
outputs use the immutable version snapshot.

MusicXML writes one uncompressed `.musicxml` file for a complete layout score.
It preserves standard pitched staves and voices, notes, rests, common phrasing,
repeats, lyrics and chord symbols for editing in notation software. Grace notes
before their principal note retain their written value, slash, beam and slur.
A tablature staff preserves its tuning and each note's string and fret. A
FretBoards track with matching chord symbols supplies editable chord diagrams.
A voice can move between staves within a piano or grand staff part, including
across a beam.
Repeat boundaries inside a measure are represented as consecutive partial
measures when no written note or rest crosses the boundary.
The receiving editor reflows the score. Unsupported notation returns an error
naming the feature, such as a second movement, after-grace notes or a
voice moving between separate parts.

For PDF, PNG and SVG, `includeDrawing: true` includes the document's shared
freehand drawing as it existed when the output job was submitted. It defaults to
false. Drawings use page-relative positions, so a different score layout can
place them over different notation. Cropped SVG/PNG outputs include only the
part of the drawing inside the crop.

MIDI and WAV accept `audioRange: {"start":1,"end":4}` for an inclusive one-based
measure range. Notes overlapping the range are retained, including notes that
began before it and continue into it; notes are clipped at the range end. MIDI
preserves channel, controller, program, SysEx, tempo and time-signature events.
WAV uses the same piano playback mix as the score page and may extend beyond the
range end by its release tail. Pickup/partial measures, SMPTE timing, format-2
MIDI and unreliable time-signature boundaries reject range export.

Output accepts an absolute `outputPath` for one file or `outputDirectory` for a
batch; the two cannot be combined. When the applicable destination is omitted,
output goes to the system temporary directory, so pass one for files to keep.
`outputPath` must not already exist, and its parent directory must exist.

SVG/PNG accept a one-based `pages` array (default `[1]`). One page may use
`outputPath`. Multiple pages use `outputDirectory` and return a batch. PDF
exports the selected pages in one file and defaults to all pages. MIDI, WAV,
MusicXML and LilyPond export without a page selection. PNG defaults to a white
1600-pixel-wide page. Optional `width` is 1..4096, and output is limited to
16 million pixels. WAV writes 44.1 kHz stereo 16-bit PCM with a fixed sample
rate, channel count and bit depth. Generate missing formats with the same
engine build or re-check first.

`crop` selects a visual region and supports SVG/PNG. Use
`{"kind":"measures","range":{"start":1,"end":4}}` or
`{"kind":"system","page":1,"systemIndex":0}`; optional `staff` selects one
zero-based staff and `paddingStaffSpaces` defaults to 2. A range crossing pages
returns one verified file per page in page order, merging matches on the same
page. A one-item `pages` list restricts a measure crop to one page; for a
system crop it must equal the crop's `page`. `outputPath` selects a crop that
resolves to exactly one page; multi-page crops publish under `outputDirectory`.
The result is an `OutputBatchOutcome` with ordered `files`; for a single-file
crop, `directory` is the file's parent. Indexed files can be downloaded or
revealed through HTTP routes ending in `/outputs/<index>` and
`/outputs/<index>/reveal`.

## Freehand drawing data

Use `ossia_read_drawing` to inspect a score's shared drawing, optionally filtered
by one-based `page`. Results contain global zero-based stroke indices and a
`revision`; follow `nextOffset` with the same `page` and `expectedRevision`.
The default page contains up to 25 strokes; `limit` may be 1–100. A single
stroke that exceeds the response limit must be exported to a file instead.

For backup or migration, pass an absolute, unused `outputPath` to export the
complete drawing as JSON. Import it into the destination with
`ossia_write_drawing`, the destination's current `expectedRevision`, and
`change: {"kind":"import","inputPath":"/absolute/path.json"}`. Files are
limited to 32 MiB; existing output files are never overwritten. The exported
revision belongs to the source and does not authorize a destination write.

For local changes use `change: {"kind":"edit","edits":[...]}` with `add`,
`delete` or `replace` edits. Delete and replace take a global `index`, and add
and replace take a `stroke`; all indices address the original revision. Added
strokes are appended. Use `change: {"kind":"replace","strokes":[]}` to clear
all ink. For example, `edits: [{"kind":"delete","index":3},
{"kind":"add","stroke":{"page":1,"width":0.01,"color":"black",
"points":[{"x":0.5,"y":0.5}]}}]` deletes one stroke and appends one.
Coordinates and width are page-relative; point coordinates are in 0–1, width
is greater than 0 and at most 1, and color is black, blue, green, yellow or red.
Read again after a conflict rather than overwriting another editor's changes. Drawings are independent of score versions and use page-relative
coordinates; a different layout may place strokes over different notation.
