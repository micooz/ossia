use crate::*;
use include_dir::{include_dir, Dir};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::HashSet,
    path::{Component, Path},
    sync::Arc,
    time::Duration,
};

static STARTER_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/resources/starter");

const MANIFEST_FILE: &str = "manifest.json";
const MANIFEST_SCHEMA_VERSION: u32 = 1;
const IMPORT_TIMEOUT: Duration = Duration::from_secs(10 * 60);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    scores: Vec<ManifestScore>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManifestScore {
    id: String,
    title: String,
    file: String,
    format: InputFormat,
    #[serde(default)]
    author: Option<String>,
}

struct StarterScore {
    id: String,
    title: String,
    file: String,
    format: InputFormat,
    author: String,
    input: &'static [u8],
}

pub(crate) fn schedule(runtime: Arc<Runtime>) {
    match runtime.library.initial_documents_pending() {
        Ok(false) => return,
        Ok(true) => {}
        Err(error) => {
            startup_note(format!(
                "ossia: cannot inspect starter-score state: {error}"
            ));
            return;
        }
    }
    tokio::spawn(async move {
        if let Err(error) = run(runtime.clone()).await {
            startup_note(format!("ossia: cannot import starter scores: {error}"));
            runtime.emit("starter-error", None, None, json!({"error": error}));
        }
    });
}

async fn run(runtime: Arc<Runtime>) -> Result<()> {
    let scores = manifest()?;
    import_scores(&runtime, &scores).await?;
    runtime.library.finish_initial_documents()
}

fn manifest() -> Result<Vec<StarterScore>> {
    let manifest_file = STARTER_DIR
        .get_file(MANIFEST_FILE)
        .ok_or_else(|| invalid("starter manifest is missing"))?;
    let manifest: Manifest = serde_json::from_slice(manifest_file.contents())
        .map_err(|error| invalid(format!("invalid starter manifest: {error}")))?;
    if manifest.schema_version != MANIFEST_SCHEMA_VERSION {
        return Err(invalid(format!(
            "unsupported starter manifest schema {}",
            manifest.schema_version
        )));
    }
    if manifest.scores.is_empty() {
        return Err(invalid("starter manifest contains no scores"));
    }
    let mut ids = HashSet::new();
    let mut files = HashSet::new();
    let mut titles = HashSet::new();
    let mut scores = Vec::with_capacity(manifest.scores.len());
    for entry in manifest.scores {
        let id = entry.id.trim();
        let title = entry.title.trim();
        let file = entry.file.trim();
        if id.is_empty() || title.is_empty() || file.is_empty() {
            return Err(invalid(
                "starter score id, title and file must not be empty",
            ));
        }
        if title.chars().count() > 200 {
            return Err(invalid(format!(
                "starter score {id} title exceeds 200 characters"
            )));
        }
        if !confined_path(file) {
            return Err(invalid(format!(
                "starter score {id} has an invalid file path"
            )));
        }
        if entry.format.is_omr() {
            return Err(invalid(format!(
                "starter score {id} requires remote score recognition"
            )));
        }
        if !ids.insert(id.to_owned()) {
            return Err(invalid(format!("duplicate starter score id {id}")));
        }
        if !files.insert(file.to_owned()) {
            return Err(invalid(format!("duplicate starter score file {file}")));
        }
        if !titles.insert(title.to_owned()) {
            return Err(invalid(format!("duplicate starter score title {title}")));
        }
        let input = STARTER_DIR
            .get_file(file)
            .ok_or_else(|| invalid(format!("starter score {id} file {file} is missing")))?;
        let author = entry
            .author
            .as_deref()
            .map(str::trim)
            .filter(|author| !author.is_empty())
            .unwrap_or(DEFAULT_AUTHOR);
        scores.push(StarterScore {
            id: id.to_owned(),
            title: title.to_owned(),
            file: file.to_owned(),
            format: entry.format,
            author: author.to_owned(),
            input: input.contents(),
        });
    }
    Ok(scores)
}

async fn import_scores(runtime: &Runtime, scores: &[StarterScore]) -> Result<()> {
    // The management library defaults to most recently modified first.
    for score in scores.iter().rev() {
        let hash = ContentHash::of(score.input);
        if runtime.library.has_initial_content(&hash)? {
            continue;
        }
        let job = runtime.import(
            ImportArgs {
                title: Some(score.title.clone()),
                format: Some(score.format),
                path: score.file.clone(),
                confirmation: None,
                author: Some(score.author.clone()),
                expect: None,
                reference: None,
                wait_ms: Some(0),
            },
            score.input.to_vec(),
        )?;
        let job = wait_for_job(runtime, job.job_id).await?;
        if !matches!(job.state, JobState::Done) {
            let mut error = job.error.unwrap_or_else(|| {
                AppError::new(
                    ErrorCode::Cancelled,
                    format!("starter score {} import was interrupted", score.id),
                )
            });
            error.message = format!("starter score {} failed: {}", score.id, error.message);
            return Err(error);
        }
        if !runtime.library.has_initial_content(&hash)? {
            return Err(storage(format!(
                "starter score {} finished without creating a document",
                score.id
            )));
        }
    }
    Ok(())
}

async fn wait_for_job(runtime: &Runtime, id: JobId) -> Result<Job> {
    let deadline = tokio::time::Instant::now() + IMPORT_TIMEOUT;
    loop {
        let job = runtime.library.get_job(id)?;
        if !matches!(job.state, JobState::Queued | JobState::Running) {
            return Ok(job);
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(AppError::new(
                ErrorCode::RuntimeUnavailable,
                format!("starter import {id} timed out"),
            ));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

fn confined_path(value: &str) -> bool {
    !value.is_empty()
        && !value.contains(['\\', ':'])
        && Path::new(value)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

#[cfg(test)]
#[path = "../tests/unit/starter.rs"]
mod tests;
