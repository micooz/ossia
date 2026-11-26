use super::transfer::{events, upload};
use super::{
    bind_document, body, id, job_args, page_args, AppErrorResponse, WebState, CLIENT_ID_HEADER,
};
use crate::*;
use axum::{
    extract::{Request, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use percent_encoding::{percent_decode_str, utf8_percent_encode, NON_ALPHANUMERIC};
use serde_json::json;

pub(super) async fn api(
    State(state): State<WebState>,
    request: Request,
) -> std::result::Result<Response, AppErrorResponse> {
    let path = request
        .uri()
        .path()
        .trim_start_matches("/api/v1/")
        .to_owned();
    let parts: Vec<_> = path.split('/').collect();
    let method = request.method().clone();
    let pagination = page_args(request.uri().query().unwrap_or(""))?;
    let client_id = request
        .headers()
        .get(CLIENT_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| value.len() <= 128 && value.bytes().all(|byte| byte.is_ascii_graphic()))
        .map(str::to_owned);
    if parts == ["events"] && method == "GET" {
        return events(state, request.uri().query().unwrap_or("")).map_err(Into::into);
    }
    if parts == ["documents"] && method == "POST" {
        return upload(state.runtime, request).await.map_err(Into::into);
    }
    if parts.len() == 4 && parts[0] == "renders" && parts[2] == "pages" && method == "GET" {
        let key: ContentHash = parts[1].to_string().try_into()?;
        let page: u32 = id(parts[3])?;
        let condition = request
            .headers()
            .get(header::IF_NONE_MATCH)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let library = state.runtime.library.clone();
        let (etag, bytes) = tokio::task::spawn_blocking(move || -> Result<_> {
            let (_, render) = library.render(&key)?;
            let asset = &render
                .pages
                .iter()
                .find(|candidate| candidate.page == page)
                .ok_or_else(|| AppError::new(ErrorCode::NotFound, "render page not found"))?
                .artifact;
            let etag = format!("\"{}\"", asset.sha256);
            let unchanged = condition.as_deref().is_some_and(|condition| {
                condition.split(',').any(|value| {
                    let value = value.trim();
                    value == "*" || value.strip_prefix("W/").unwrap_or(value) == etag
                })
            });
            let bytes = if unchanged {
                None
            } else {
                Some(library.read_artifact(asset)?)
            };
            Ok((etag, bytes))
        })
        .await
        .map_err(storage)??;
        return Ok((
            if bytes.is_some() {
                StatusCode::OK
            } else {
                StatusCode::NOT_MODIFIED
            },
            [
                (header::CONTENT_TYPE, "image/svg+xml".to_owned()),
                (header::CACHE_CONTROL, "private, no-cache".to_owned()),
                (header::ETAG, etag),
                (
                    header::CONTENT_SECURITY_POLICY,
                    "default-src 'none'; style-src 'unsafe-inline'; sandbox".to_owned(),
                ),
            ],
            bytes.unwrap_or_default(),
        )
            .into_response());
    }

    if parts.len() == 3 && parts[0] == "jobs" && parts[2] == "output" && method == "GET" {
        let filename = output_filename(request.uri().query().unwrap_or(""))?;
        let (outcome, bytes) = state.runtime.export_bytes(id(parts[1])?)?;
        let mut response = bytes.into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_str(&outcome.media_type)
                .map_err(|_| storage("invalid output media type"))?,
        );
        response.headers_mut().insert(
            header::CONTENT_DISPOSITION,
            attachment_header(filename.as_deref())?,
        );
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("private, no-store"),
        );
        return Ok(response);
    }
    if parts.len() == 4 && parts[0] == "jobs" && parts[2] == "outputs" && method == "GET" {
        let index = parts[3]
            .parse::<usize>()
            .map_err(|_| invalid("output file index must be a nonnegative integer"))?;
        let (media_type, bytes) = state.runtime.export_bytes_at(id(parts[1])?, index)?;
        let mut response = bytes.into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_str(&media_type).map_err(|_| storage("invalid output media type"))?,
        );
        response.headers_mut().insert(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment"),
        );
        response.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("private, no-store"),
        );
        return Ok(response);
    }
    let runtime = state.runtime.clone();
    let query = request.uri().query().unwrap_or("").to_owned();
    let data = if method == "POST" || method == "PUT" {
        let limit = if method == "PUT"
            && parts.len() == 3
            && parts[0] == "documents"
            && parts[2] == "drawing"
        {
            32 * 1024 * 1024
        } else {
            1024 * 1024
        };
        body(request, limit).await?
    } else {
        json!({})
    };
    let path = parts.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let result = tokio::task::spawn_blocking(move || -> Result<(Value, bool)> {
        let p: Vec<_> = path.iter().map(String::as_str).collect();
        let mut queued = false;
        let result = match (method.as_str(), p.as_slice()) {
            ("GET", ["renders", key, "geometry"]) => {
                value(runtime.geometry(&key.to_string().try_into()?)?)?
            }
            ("GET", ["renders", key, "playback"]) => {
                value(runtime.playback(&key.to_string().try_into()?)?)?
            }
            ("POST", ["documents", doc, "versions", version, "render"]) => {
                queued = true;
                value(runtime.render_version(id(doc)?, id(version)?)?)?
            }
            ("POST", ["jobs", job, "reveal"]) => {
                runtime.reveal_export(id(job)?)?;
                json!({"opened":true})
            }
            ("POST", ["jobs", job, "outputs", index, "reveal"]) => {
                let index = index
                    .parse::<usize>()
                    .map_err(|_| invalid("output file index must be a nonnegative integer"))?;
                runtime.reveal_export_at(id(job)?, index)?;
                json!({"opened":true})
            }
            ("GET", ["status"]) => value(runtime.agent_status()?)?,
            ("PUT", ["ui-language"]) => {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct LanguageChange {
                    language: UiLanguage,
                }
                let change: LanguageChange = decode(data)?;
                let _guard = preferences::lock(runtime.library.home())?;
                let mut current = preferences::load(runtime.library.home())?;
                current.language = Some(change.language);
                preferences::save(runtime.library.home(), &current)?;
                runtime.set_ui_language(current.language);
                value(change.language)?
            }

            ("GET", ["library", "scores"]) => {
                value(runtime.management_scores(pagination.clone())?)?
            }
            ("POST", ["library", "scores", doc, "preview"]) => {
                let retry = data.get("retry").and_then(Value::as_bool).unwrap_or(false);
                value(
                    runtime.management_score_preview(ManagementScorePreviewArgs {
                        document_id: id(doc)?,
                        retry,
                    })?,
                )?
            }
            ("GET", ["documents"]) => value(runtime.agent_list(pagination.clone())?)?,
            ("GET", ["documents", doc]) => {
                let doc = id(doc)?;
                json!({
                    "document": runtime.observe(doc)?,
                    "versions": runtime.agent_history(HistoryArgs {
                        document_id: doc,
                        limit: pagination.limit,
                        offset: pagination.offset,
                    })?
                })
            }
            ("GET", ["documents", doc, "context"]) => value(runtime.context(id(doc)?)?)?,
            ("GET", ["documents", doc, "settings"]) => {
                value(runtime.library.score_settings(id(doc)?)?)?
            }
            ("PUT", ["documents", doc, "settings"]) => {
                let document = id(doc)?;
                let settings = runtime
                    .library
                    .save_score_settings(document, decode(data)?)?;
                runtime.emit(
                    "score-settings-changed",
                    Some(document),
                    None,
                    json!({"revision":settings.revision}),
                );
                value(settings)?
            }
            ("GET", ["documents", doc, "drawing"]) => value(runtime.library.drawing(id(doc)?)?)?,
            ("PUT", ["documents", doc, "drawing"]) => {
                let document = id(doc)?;
                let drawing = runtime.save_drawing(document, decode(data)?)?;
                value(drawing)?
            }
            ("POST", ["documents", doc, "locate"]) => {
                value(runtime.locate(id(doc)?, decode(data)?)?)?
            }
            ("PUT", ["documents", doc, "selection"]) => {
                let document_id = id(doc)?;
                let args: Selection = decode(data)?;
                if args.document_id != document_id {
                    return Err(AppError::new(
                        ErrorCode::Forbidden,
                        "path document does not match request",
                    ));
                }
                value(runtime.selection_with_client(args, client_id)?)?
            }
            ("POST", ["documents", doc, "edits"]) => {
                bind_document(&data, id(doc)?)?;
                let args: EditArgs = decode(data)?;
                runtime.edit(args)?
            }
            ("POST", ["documents", doc, "check"]) => {
                let document_id = id(doc)?;
                let mut args: CheckArgs = decode(data)?;
                if args
                    .document_id
                    .is_some_and(|document| document != document_id)
                {
                    return Err(AppError::new(
                        ErrorCode::Forbidden,
                        "path document does not match request",
                    ));
                }
                args.document_id = Some(document_id);
                queued = true;
                value(runtime.check(args, false)?)?
            }
            ("POST", ["documents", doc, "commit"]) => {
                let document_id = id(doc)?;
                let args: CommitArgs = decode(data)?;
                if runtime.library.get_edit(args.edit_id)?.document_id != document_id {
                    return Err(AppError::new(
                        ErrorCode::Forbidden,
                        "path document does not match request",
                    ));
                }
                value(runtime.commit(args)?)?
            }
            ("GET", ["documents", doc, "versions"]) => {
                value(runtime.agent_history(HistoryArgs {
                    document_id: id(doc)?,
                    limit: pagination.limit,
                    offset: pagination.offset,
                })?)?
            }
            ("POST", ["documents", doc, "restore"]) => {
                let document_id = id(doc)?;
                let args: RestoreArgs = decode(data)?;
                if runtime
                    .library
                    .version_snapshot(args.version_id)?
                    .document_id()
                    != document_id
                {
                    return Err(AppError::new(
                        ErrorCode::Forbidden,
                        "path document does not match request",
                    ));
                }
                queued = true;
                value(runtime.restore(args)?)?
            }
            ("POST", ["outputs"]) => {
                queued = true;
                value(runtime.output(decode(data)?)?)?
            }
            ("GET", ["jobs", job]) => {
                value(runtime.job_outcome(job_args(&query, id(job)?, JobAction::Get)?)?)?
            }
            ("POST", ["jobs", job, "cancel"]) => {
                value(runtime.job_outcome(job_args(&query, id(job)?, JobAction::Cancel)?)?)?
            }
            _ => return Err(AppError::new(ErrorCode::NotFound, "route not found")),
        };
        Ok((result, queued))
    })
    .await
    .map_err(storage)??;
    let (mut result, queued) = result;
    normalize_outbound_paths(&mut result);
    Ok((
        if queued {
            StatusCode::ACCEPTED
        } else {
            StatusCode::OK
        },
        axum::Json(result),
    )
        .into_response())
}

fn output_filename(query: &str) -> Result<Option<String>> {
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key != "filename" {
            continue;
        }
        let value = value.replace('+', "%20");
        let filename = percent_decode_str(&value)
            .decode_utf8()
            .map_err(|_| invalid("output filename must be UTF-8"))?;
        if filename.is_empty()
            || filename.chars().count() > 240
            || filename
                .chars()
                .any(|character| character.is_control() || matches!(character, '/' | '\\' | '"'))
        {
            return Err(invalid("invalid output filename"));
        }
        return Ok(Some(filename.into_owned()));
    }
    Ok(None)
}

fn attachment_header(filename: Option<&str>) -> Result<HeaderValue> {
    let Some(filename) = filename else {
        return Ok(HeaderValue::from_static("attachment"));
    };
    let fallback: String = filename
        .chars()
        .map(|character| {
            if character.is_ascii_graphic() && !matches!(character, '"' | '\\') {
                character
            } else {
                '_'
            }
        })
        .collect();
    let encoded = utf8_percent_encode(filename, NON_ALPHANUMERIC);
    HeaderValue::from_str(&format!(
        "attachment; filename=\"{fallback}\"; filename*=UTF-8''{encoded}"
    ))
    .map_err(|_| invalid("invalid output filename"))
}
