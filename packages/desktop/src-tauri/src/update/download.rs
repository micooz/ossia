//! Streaming asset downloads with checksum computation.

use super::{fetch, Error, Result};
use reqwest::{
    header::{HeaderMap, LOCATION},
    Response,
};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Write, path::Path, time::Duration};

const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);

pub async fn to_file(
    url: &str,
    token: Option<&str>,
    dest: &Path,
    progress: &mut (dyn FnMut(u64, u64) + Send),
) -> Result<String> {
    let mut response = asset_response(url, token, DOWNLOAD_TIMEOUT).await?;
    let total = response.content_length().unwrap_or(0);
    let mut file = File::create(dest)?;
    let mut hasher = Sha256::new();
    let mut written = 0u64;
    loop {
        let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| Error::other(format!("download: {error}")))?
        else {
            break;
        };
        file.write_all(&chunk)?;
        hasher.update(&chunk);
        written += chunk.len() as u64;
        progress(written, total);
    }
    file.flush()?;
    Ok(hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

pub async fn text(url: &str, token: Option<&str>) -> Result<String> {
    let mut response = asset_response(url, token, Duration::from_secs(30)).await?;
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| Error::other(format!("download: {error}")))?
    {
        let remaining = (1024 * 1024usize).saturating_sub(body.len());
        body.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        if remaining <= chunk.len() {
            break;
        }
    }
    String::from_utf8(body).map_err(|error| Error::other(format!("download: {error}")))
}

async fn asset_response(url: &str, token: Option<&str>, timeout: Duration) -> Result<Response> {
    fetch::ensure_secure(url)?;
    let client = async_client(false)?;
    let mut request = client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/octet-stream")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .timeout(timeout);
    if let Some(token) = token {
        if !fetch::token_destination(url)? {
            return Err(Error::other(
                "refusing to send a GitHub token outside https://api.github.com",
            ));
        }
        request = request.bearer_auth(token);
    }
    let response = request
        .send()
        .await
        .map_err(|error| Error::other(format!("download: {error}")))?;
    let status = response.status().as_u16();
    let response = if (300..400).contains(&status) {
        let location = redirect_location(url, response.headers())?;
        fetch::ensure_secure(&location)?;
        async_client(true)?
            .get(location)
            .timeout(timeout)
            .send()
            .await
            .map_err(|error| Error::other(format!("download: {error}")))?
    } else {
        response
    };
    success(response).await
}

fn async_client(redirect: bool) -> Result<reqwest::Client> {
    let policy = if redirect {
        reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5
                || fetch::ensure_secure(attempt.url().as_str()).is_err()
            {
                attempt.error("invalid update redirect")
            } else {
                attempt.follow()
            }
        })
    } else {
        reqwest::redirect::Policy::none()
    };
    reqwest::Client::builder()
        .user_agent(concat!("ossia/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(8))
        .redirect(policy)
        .build()
        .map_err(|error| Error::other(error.to_string()))
}

async fn success(mut response: Response) -> Result<Response> {
    let status = response.status().as_u16();
    if (200..300).contains(&status) {
        return Ok(response);
    }
    let headers = response.headers().clone();
    let mut body = Vec::new();
    while let Ok(Some(chunk)) = response.chunk().await {
        let remaining = 8192usize.saturating_sub(body.len());
        body.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        if remaining <= chunk.len() {
            break;
        }
    }
    Err(Error::other(format!(
        "download: {}",
        super::github::describe_failure(status, &headers, &String::from_utf8_lossy(&body))
    )))
}

fn redirect_location(url: &str, headers: &HeaderMap) -> Result<String> {
    let location = headers
        .get(LOCATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .ok_or_else(|| Error::other("download redirect has no location"))?;
    reqwest::Url::parse(url)
        .and_then(|base| base.join(&location))
        .map(|url| url.to_string())
        .map_err(|error| Error::other(format!("download redirect: {error}")))
}
