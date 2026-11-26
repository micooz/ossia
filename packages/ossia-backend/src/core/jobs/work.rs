use super::*;
use std::path::PathBuf;

pub(crate) enum Work {
    Check,
    Evidence,
    VersionRender(DocumentId, VersionId),
    Import(ImportWork),
    Restore(RestoreArgs),
    Output(
        OutputArgs,
        OutputDestination,
        Option<Drawing>,
        Option<ScoreSettings>,
    ),
}

pub(crate) enum OutputDestination {
    File(PathBuf),
    CropFile(PathBuf),
    PageBatch {
        parent: PathBuf,
        directory_name: String,
    },
    Batch {
        parent: PathBuf,
        directory_name: String,
    },
}

pub(crate) struct ImportWork {
    pub(super) title: String,
    pub(super) format: InputFormat,
    pub(super) author: String,
    pub(super) confirmation: Option<ContentHash>,
    pub(super) expect: Option<MusicExpectation>,
    pub(super) reference: Option<ReferenceMusic>,
    pub(super) bytes: Vec<u8>,
}

pub(super) fn output_destination(
    args: &OutputArgs,
    document: DocumentId,
) -> Result<OutputDestination> {
    let pages = match &args.pages {
        Some(pages) if pages.is_empty() => {
            return Err(invalid("pages must contain at least one page"));
        }
        Some(pages) => {
            let mut pages = pages.clone();
            pages.sort_unstable();
            if pages.contains(&0) || pages.windows(2).any(|pair| pair[0] == pair[1]) {
                return Err(invalid("pages must be positive and unique"));
            }
            Some(pages)
        }
        None => None,
    };
    if args.crop.is_some() {
        if !matches!(args.format, OutputFormat::Svg | OutputFormat::Png) {
            return Err(invalid("crop output supports only SVG and PNG"));
        }
        if pages.as_ref().is_some_and(|pages| pages.len() > 1) {
            return Err(invalid("crop output accepts at most one page filter"));
        }
        if let Some(OutputCrop::System { page, .. }) = &args.crop {
            if pages
                .as_ref()
                .and_then(|pages| pages.first())
                .is_some_and(|requested| requested != page)
            {
                return Err(invalid("page must match the crop system page"));
            }
        }
        if args.output_directory.is_some() && args.output_path.is_some() {
            return Err(invalid(
                "outputDirectory and outputPath are mutually exclusive",
            ));
        }
        if let Some(path) = &args.output_path {
            let path = PathBuf::from(path);
            if !path.is_absolute() {
                return Err(invalid("outputPath must be absolute"));
            }
            return Ok(OutputDestination::CropFile(path));
        }
        let parent = output_parent(args.output_directory.as_deref())?;
        std::fs::create_dir_all(&parent).map_err(storage)?;
        return Ok(OutputDestination::Batch {
            parent,
            directory_name: format!("{}-{}", document, uuid::Uuid::new_v4()),
        });
    }
    if args.padding_staff_spaces.is_some() {
        return Err(invalid("paddingStaffSpaces requires crop"));
    }
    if args.audio_range.is_some() && !matches!(args.format, OutputFormat::Midi | OutputFormat::Wav)
    {
        return Err(invalid("audioRange supports only MIDI and WAV"));
    }
    if let Some(range) = args.audio_range {
        range.validate().map_err(|error| invalid(error.message))?;
    }
    if args.output_directory.is_some() && args.output_path.is_some() {
        return Err(invalid(
            "outputDirectory and outputPath are mutually exclusive",
        ));
    }
    if !matches!(args.format, OutputFormat::Svg | OutputFormat::Png) {
        if pages.is_some() && args.format != OutputFormat::Pdf {
            return Err(invalid("pages is supported only for PDF, SVG and PNG"));
        }
        if args.output_directory.is_some() {
            return Err(invalid("outputDirectory requires crop or multiple pages"));
        }
        if let Some(path) = &args.output_path {
            let path = PathBuf::from(path);
            if !path.is_absolute() {
                return Err(invalid("outputPath must be absolute"));
            }
            return Ok(OutputDestination::File(path));
        }
        let folder = temp_folder()?;
        let extension = match args.format {
            OutputFormat::Ly => "ly",
            OutputFormat::Svg => "svg",
            OutputFormat::Png => "png",
            OutputFormat::Pdf => "pdf",
            OutputFormat::Midi => "mid",
            OutputFormat::Wav => "wav",
        };
        return Ok(OutputDestination::File(folder.join(format!(
            "{}-{}.{}",
            document,
            uuid::Uuid::new_v4(),
            extension
        ))));
    }
    let pages = pages.unwrap_or_else(|| vec![1]);
    if pages.len() > 1 {
        if args.output_path.is_some() {
            return Err(invalid("outputPath requires exactly one page"));
        }
        let parent = output_parent(args.output_directory.as_deref())?;
        std::fs::create_dir_all(&parent).map_err(storage)?;
        return Ok(OutputDestination::PageBatch {
            parent,
            directory_name: format!("{}-{}", document, uuid::Uuid::new_v4()),
        });
    }
    if args.output_directory.is_some() && args.pages.is_none() {
        return Err(invalid(
            "outputDirectory requires crop or an explicit pages list",
        ));
    }
    if let Some(path) = &args.output_path {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err(invalid("outputPath must be absolute"));
        }
        return Ok(OutputDestination::File(path));
    }
    if args.output_directory.is_some() {
        let parent = output_parent(args.output_directory.as_deref())?;
        std::fs::create_dir_all(&parent).map_err(storage)?;
        return Ok(OutputDestination::PageBatch {
            parent,
            directory_name: format!("{}-{}", document, uuid::Uuid::new_v4()),
        });
    }
    let folder = temp_folder()?;
    let extension = if args.format == OutputFormat::Png {
        "png"
    } else {
        "svg"
    };
    Ok(OutputDestination::File(folder.join(format!(
        "{}-{}.{}",
        document,
        uuid::Uuid::new_v4(),
        extension
    ))))
}

fn temp_folder() -> Result<PathBuf> {
    let folder = std::env::temp_dir().join("ossia");
    std::fs::create_dir_all(&folder).map_err(storage)?;
    Ok(folder)
}

fn output_parent(path: Option<&str>) -> Result<PathBuf> {
    match path {
        Some(path) => {
            let path = PathBuf::from(path);
            if path.is_absolute() {
                Ok(path)
            } else {
                Err(invalid("outputDirectory must be absolute"))
            }
        }
        None => temp_folder(),
    }
}

pub(crate) struct Pending {
    pub(crate) job: Job,
    pub(crate) work: Work,
    pub(crate) preview: bool,
    pub(crate) context: RunContext,
}
