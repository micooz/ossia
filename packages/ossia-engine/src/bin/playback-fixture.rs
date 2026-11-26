use ossia_engine::*;
use std::{env, fs, path::PathBuf, time::Duration};

const JIANPU_MARKER: &str = "% ossia:notation=jianpu\n";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os().skip(1);
    let root = PathBuf::from(args.next().ok_or("missing LilyPond root")?);
    let path = PathBuf::from(args.next().ok_or("missing score fixture")?);
    let (check_playback, check_selection, jianpu) = match args.next() {
        None => (true, false, false),
        Some(flag) if flag == "--selection" => (false, true, false),
        Some(flag) if flag == "--all" => (true, true, false),
        Some(flag) if flag == "--jianpu" => (true, true, true),
        Some(flag) => {
            return Err(format!("unknown fixture mode: {}", flag.to_string_lossy()).into())
        }
    };
    let input = fs::read_to_string(path)?;
    let marked = ossia_score::is_jianpu_source(&input);
    let line_offset = usize::from(jianpu && !marked);
    let (bom, body) = input
        .strip_prefix('\u{feff}')
        .map_or(("", input.as_str()), |body| ("\u{feff}", body));
    let source = if line_offset == 1 {
        format!("{bom}{JIANPU_MARKER}{body}")
    } else {
        input.clone()
    };
    let work_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/playback-verify");
    fs::create_dir_all(&work_root)?;
    let work = tempfile::tempdir_in(work_root)?;
    let context = RunContext::new(Duration::from_secs(120));
    let engine = Engine::verify(&root, &work.path().join("scratch"), &context)?;
    let snapshot = SourceSnapshot::new(DocumentId::new(), source.as_bytes().to_vec())?;
    let assets = work.path().join("assets");
    let mut formats = vec![RenderFormat::Svg];
    if check_playback {
        formats.push(RenderFormat::Midi);
    }
    if jianpu {
        formats.push(RenderFormat::Pdf);
    }
    let render = engine.compile(
        &snapshot,
        &CompileOptions {
            formats,
            point_and_click: true,
        },
        &assets,
        &context,
    );
    let render = match render {
        Ok(render) => render,
        Err(error) if jianpu => {
            println!(
                "{}",
                serde_json::json!({ "compileError": error, "lineOffset": line_offset })
            );
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };
    if !render.format_errors.is_empty() {
        if jianpu {
            println!(
                "{}",
                serde_json::json!({
                    "formatErrors": render.format_errors, "pageCount": render.pages.len(),
                    "pdf": render.pdf.is_some(), "lineOffset": line_offset,
                })
            );
            return Ok(());
        }
        return Err(format!("{:?}", render.format_errors).into());
    }
    let pages = render
        .pages
        .iter()
        .map(|page| {
            Ok((
                page.page,
                fs::read_to_string(assets.join(&page.artifact.path))?,
            ))
        })
        .collect::<Result<Vec<_>, std::io::Error>>()?;
    let index = ossia_score::build_index(&source);
    let layout = ossia_score::build_layout(
        &index,
        &pages
            .iter()
            .map(|(page, svg)| ossia_score::SvgPage { page: *page, svg })
            .collect::<Vec<_>>(),
    )?;
    let key = ContentHash::of(source.as_bytes());
    let mut jianpu_checks = None;
    let (playback, midi_note_count) = if check_playback {
        let midi_bytes = fs::read(assets.join(render.midi.ok_or("missing MIDI")?.path))?;
        if jianpu {
            let baseline_source = if marked {
                let (_, music) = body.split_once('\n').unwrap_or((body, ""));
                format!("{bom}{music}")
            } else {
                input
            };
            let baseline = SourceSnapshot::new(DocumentId::new(), baseline_source.into_bytes())?;
            let baseline = engine.compile(
                &baseline,
                &CompileOptions {
                    formats: vec![RenderFormat::Midi],
                    point_and_click: true,
                },
                &assets,
                &context,
            )?;
            let same_midi = baseline
                .midi
                .map(|asset| fs::read(assets.join(asset.path)))
                .transpose()?
                .is_some_and(|bytes| bytes == midi_bytes);
            jianpu_checks = Some(serde_json::json!({
                "pdf": render.pdf.is_some(), "sameMidi": same_midi,
                "baselineErrors": baseline.format_errors, "lineOffset": line_offset,
                "sourceMeasureCount": index.measure_count().ok(),
            }));
        }
        let midi = ossia_score::parse_midi(&midi_bytes)?;
        (
            Some(ossia_score::build_playback(
                &index,
                &layout,
                &midi,
                key.clone(),
            )?),
            Some(
                midi.tracks
                    .iter()
                    .map(|track| track.notes.len())
                    .sum::<usize>(),
            ),
        )
    } else {
        (None, None)
    };
    let selection_geometry = ossia_score::selection_geometry(&index, &layout)?;
    let selection = check_selection
        .then(|| {
            let engraved = selection_geometry.has_engraved_measures();
            let reliable =
                engraved || ossia_score::music_identity_with_index(&index, &source).reliable;
            let pages = selection_geometry
                .iter()
                .map(|page| {
                    page.regions
                        .iter()
                        .map(|region| selection_geometry.resolve_measure(&index, &region.locator))
                        .collect::<Result<Vec<_>, _>>()
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok::<_, ossia_types::AppError>(serde_json::json!({
                "engraved": engraved,
                "reliable": reliable,
                "measureCountError": index.measure_count().err(),
                "staves": index.staves,
                "voices": index.voices,
                "pages": pages
            }))
        })
        .transpose()?;
    let geometry = serde_json::json!({ "renderKey": key, "sourceHash": index.source_hash, "pages": selection_geometry.pages });
    let mut bundle = serde_json::json!({"playback": playback, "geometry": geometry, "layout": layout, "selection": selection, "pages": pages, "midiNoteCount": midi_note_count});
    if let Some(checks) = jianpu_checks {
        bundle["jianpu"] = checks;
    }
    println!("{bundle}");
    Ok(())
}
