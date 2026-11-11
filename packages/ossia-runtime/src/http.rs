use crate::*;
use axum::{
    body::to_bytes,
    extract::{DefaultBodyLimit, FromRequest, Multipart, Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{any, get},
    Router,
};
use serde_json::{json, Value};
use std::{convert::Infallible, path::PathBuf, sync::Arc, time::Duration};

#[derive(Clone)]
pub struct WebState {
    pub runtime: Arc<Runtime>,
    pub dev_origin: Option<String>,
}
impl IntoResponse for AppErrorResponse {
    fn into_response(self) -> Response {
        let code = match self.0.code {
            ErrorCode::Unauthorized => StatusCode::UNAUTHORIZED,
            ErrorCode::Forbidden => StatusCode::FORBIDDEN,
            ErrorCode::NotFound => StatusCode::NOT_FOUND,
            ErrorCode::Conflict
            | ErrorCode::EditBusy
            | ErrorCode::DirtyWorkspace
            | ErrorCode::StaleSelection
            | ErrorCode::LibraryBusy => StatusCode::CONFLICT,
            ErrorCode::InvalidRequest | ErrorCode::InvalidInput | ErrorCode::InvalidEncoding => {
                StatusCode::BAD_REQUEST
            }
            ErrorCode::StorageError => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::RuntimeUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            _ => StatusCode::UNPROCESSABLE_ENTITY,
        };
        (code, axum::Json(json!({"error":self.0}))).into_response()
    }
}
pub struct AppErrorResponse(pub AppError);
impl From<AppError> for AppErrorResponse {
    fn from(error: AppError) -> Self {
        Self(error)
    }
}

pub fn router(
    runtime: Arc<Runtime>,
    static_dir: Option<PathBuf>,
    dev_origin: Option<String>,
) -> Router {
    let state = WebState {
        runtime,
        dev_origin,
    };
    let mut app = Router::new()
        .route(
            "/",
            get(|| async { axum::response::Html(include_str!("landing.html")) }),
        )
        .route(
            "/health",
            get(|| async { axum::Json(json!({"alive":true,"protocolVersion":PROTOCOL_VERSION})) }),
        )
        .route("/api/v1/{*path}", any(api))
        .layer(DefaultBodyLimit::max(33 * 1024 * 1024));
    if let Some(path) = static_dir {
        app = app.fallback_service(tower_http::services::ServeDir::new(path));
    }
    app.layer(middleware::from_fn_with_state(state.clone(), boundary))
        .with_state(state)
}

async fn boundary(State(state): State<WebState>, request: Request, next: Next) -> Response {
    let expected = state
        .runtime
        .info
        .web_base_url
        .trim_start_matches("http://");
    if request
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        != Some(expected)
    {
        return AppErrorResponse(AppError::new(ErrorCode::Forbidden, "invalid loopback Host"))
            .into_response();
    }
    let origin = request.headers().get(header::ORIGIN).cloned();
    if let Some(origin) = &origin {
        let allowed = origin.to_str().ok().is_some_and(|s| {
            s == state.runtime.info.web_base_url || state.dev_origin.as_deref() == Some(s)
        });
        if !allowed {
            return AppErrorResponse(AppError::new(ErrorCode::Forbidden, "Origin is not allowed"))
                .into_response();
        }
    }
    let mut response = if request.method() == axum::http::Method::OPTIONS {
        (
            StatusCode::NO_CONTENT,
            [
                (header::ACCESS_CONTROL_ALLOW_METHODS, "GET, POST, PUT"),
                (
                    header::ACCESS_CONTROL_ALLOW_HEADERS,
                    "Authorization, Content-Type",
                ),
            ],
        )
            .into_response()
    } else {
        next.run(request).await
    };
    if let Some(origin) = origin {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
        response
            .headers_mut()
            .insert(header::VARY, HeaderValue::from_static("Origin"));
    }
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

fn bearer(request: &Request) -> Result<&str> {
    request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .ok_or_else(|| AppError::new(ErrorCode::Unauthorized, "bearer token required"))
}
fn same_view(actual: ViewId, requested: ViewId) -> Result<()> {
    if actual != requested {
        Err(AppError::new(
            ErrorCode::Forbidden,
            "cross-view operation rejected",
        ))
    } else {
        Ok(())
    }
}
fn id<T: std::str::FromStr>(text: &str) -> Result<T> {
    text.parse().map_err(|_| invalid("invalid identifier"))
}
async fn body(request: Request) -> Result<Value> {
    let bytes = to_bytes(request.into_body(), 1024 * 1024)
        .await
        .map_err(|_| invalid("invalid or oversized JSON request"))?;
    serde_json::from_slice(&bytes).map_err(|e| invalid(e.to_string()))
}
fn bind_document(data: &Value, document: DocumentId) -> Result<()> {
    if data.get("documentId").and_then(Value::as_str) != Some(document.to_string().as_str()) {
        return Err(invalid("body documentId must match route"));
    }
    Ok(())
}

fn page_args(query: &str) -> Result<ListArgs> {
    let mut limit = None;
    let mut offset = 0;
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        match key {
            "limit" => limit = Some(value.parse().map_err(|_| invalid("invalid limit"))?),
            "offset" => offset = value.parse().map_err(|_| invalid("invalid offset"))?,
            _ => {}
        }
    }
    Ok(ListArgs { limit, offset })
}

fn job_args(query: &str, job_id: JobId, action: JobAction) -> Result<JobArgs> {
    let mut args = JobArgs {
        detail: DetailLevel::Summary,
        job_id,
        action,
        wait_ms: None,
        section: None,
        offset: 0,
        limit: None,
    };
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        match key {
            "detail" => {
                args.detail = match value {
                    "summary" => DetailLevel::Summary,
                    "full" => DetailLevel::Full,
                    _ => return Err(invalid("invalid detail")),
                }
            }
            "waitMs" => args.wait_ms = Some(value.parse().map_err(|_| invalid("invalid waitMs"))?),
            "section" => {
                args.section = Some(match value {
                    "performance" => AnalysisSection::Performance,
                    "source" => AnalysisSection::Source,
                    "differences" => AnalysisSection::Differences,
                    _ => return Err(invalid("invalid section")),
                })
            }
            "offset" => args.offset = value.parse().map_err(|_| invalid("invalid offset"))?,
            "limit" => args.limit = Some(value.parse().map_err(|_| invalid("invalid limit"))?),
            _ => {}
        }
    }
    Ok(args)
}

async fn api(
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
    if parts == ["sessions", "exchange"] && method == "POST" {
        let args: ExchangeRequest = decode(body(request).await?)?;
        return Ok(axum::Json(state.runtime.exchange(&args.ticket)?).into_response());
    }
    let token = bearer(&request)?.to_owned();
    let view = state.runtime.authenticate(&token)?;
    if parts == ["events"] && method == "GET" {
        return events(state, view, token, request.uri().query().unwrap_or("")).map_err(Into::into);
    }
    if parts == ["documents"] && method == "POST" {
        return upload(state.runtime, request).await.map_err(Into::into);
    }
    if parts.len() == 4 && parts[0] == "renders" && parts[2] == "pages" && method == "GET" {
        let key: ContentHash = parts[1].to_string().try_into()?;
        let page = id(parts[3])?;
        let bytes = state.runtime.library.read_render_page(&key, page)?;
        return Ok((
            [
                (header::CONTENT_TYPE, "image/svg+xml"),
                (header::CACHE_CONTROL, "private, no-store"),
                (
                    header::CONTENT_SECURITY_POLICY,
                    "default-src 'none'; style-src 'unsafe-inline'; sandbox",
                ),
            ],
            bytes,
        )
            .into_response());
    }
    let runtime = state.runtime.clone();
    let query = request.uri().query().unwrap_or("").to_owned();
    let data = if method == "POST" || method == "PUT" {
        body(request).await?
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
            ("POST", ["documents", doc, "versions", version, "render"]) => {
                queued = true;
                value(runtime.render_version(id(doc)?, id(version)?)?)?
            }
            ("POST", ["jobs", job, "reveal"]) => {
                runtime.reveal_export(id(job)?)?;
                json!({"opened":true})
            }
            ("GET", ["status"]) => value(runtime.agent_status()?)?,
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
            ("GET", ["views", v]) => {
                let v = id(v)?;
                same_view(view, v)?;
                value(runtime.context(v)?)?
            }
            ("PUT", ["views", v, "document"]) => {
                let v = id(v)?;
                same_view(view, v)?;
                let args: OpenArgs = decode(data)?;
                value(runtime.select_document(v, args.document_id)?)?
            }
            ("POST", ["views", v, "locate"]) => {
                let v = id(v)?;
                same_view(view, v)?;
                value(runtime.locate(v, decode(data)?)?)?
            }
            ("PUT", ["views", v, "selection"]) => {
                let v = id(v)?;
                same_view(view, v)?;
                let args: Selection = decode(data)?;
                same_view(view, args.view_id)?;
                value(runtime.selection(args)?)?
            }
            ("POST", ["documents", doc, "edits"]) => {
                bind_document(&data, id(doc)?)?;
                let args: EditArgs = decode(data)?;
                same_view(view, args.view_id)?;
                runtime.edit(args, false)?
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
                same_view(view, args.view_id)?;
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
                same_view(view, args.view_id)?;
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
    Ok((
        if result.1 {
            StatusCode::ACCEPTED
        } else {
            StatusCode::OK
        },
        axum::Json(result.0),
    )
        .into_response())
}

async fn upload(runtime: Arc<Runtime>, request: Request) -> Result<Response> {
    let mut multipart = Multipart::from_request(request, &())
        .await
        .map_err(|_| invalid("expected multipart/form-data"))?;
    let mut metadata = None;
    let mut bytes = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| invalid("invalid or oversized multipart upload"))?
    {
        match field.name() {
            Some("metadata") if metadata.is_none() => {
                let text = field
                    .text()
                    .await
                    .map_err(|_| invalid("invalid metadata"))?;
                let mut data: Value =
                    serde_json::from_str(&text).map_err(|_| invalid("metadata must be JSON"))?;
                if !data.is_object() {
                    return Err(invalid("metadata must be an object"));
                }
                if data.get("path").is_some() {
                    return Err(invalid("page uploads cannot import local paths"));
                }
                data["path"] = json!("");
                metadata = Some(decode::<ImportArgs>(data)?);
            }
            Some("file") if bytes.is_none() => {
                let data = field
                    .bytes()
                    .await
                    .map_err(|_| invalid("invalid file upload"))?;
                if data.len() > 32 * 1024 * 1024 {
                    return Err(invalid("file exceeds 32 MiB"));
                }
                bytes = Some(data.to_vec());
            }
            _ => return Err(invalid("expected one metadata and one file field")),
        }
    }
    let job = runtime.import(
        metadata.ok_or_else(|| invalid("missing metadata"))?,
        bytes.ok_or_else(|| invalid("missing file"))?,
    )?;
    Ok((StatusCode::ACCEPTED, axum::Json(job)).into_response())
}

fn events(state: WebState, view: ViewId, token: String, query: &str) -> Result<Response> {
    let mut instance = None;
    let mut after = None;
    for pair in query.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            match key {
                "instanceId" => instance = Some(value.to_string()),
                "after" => after = Some(id::<u64>(value)?),
                _ => {}
            }
        }
    }
    let instance = instance.ok_or_else(|| invalid("events requires snapshot instanceId"))?;
    let mut after = after.ok_or_else(|| invalid("events requires snapshot cursor after"))?;
    state.runtime.events(view, &instance, after)?;
    let stream = async_stream::stream! {loop{
        let notified=state.runtime.changed.notified();
        if state.runtime.authenticate(&token).is_err(){yield Ok::<_,Infallible>(Event::default().event("session-expired").data("{}"));break;}
        match state.runtime.events(view,&instance,after){Ok(events)=>{for event in events{after=event.seq;yield Ok(Event::default().id(event.seq.to_string()).event("runtime").data(serde_json::to_string(&event).expect("event serialization")));}},Err(_)=>{yield Ok(Event::default().event("resync-required").data("{}"));break;}}
        if state.runtime.stopping.load(std::sync::atomic::Ordering::Acquire){break;}
        tokio::select!{_=notified=>{},_=tokio::time::sleep(Duration::from_secs(1))=>{}}
    }};
    Ok(Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response())
}
