//! Bounded HTTPS access to GitHub releases.

use super::{Error, Result};
use reqwest::{
    blocking::{Client, RequestBuilder},
    header::{ACCEPT, AUTHORIZATION},
    redirect::Policy,
    Url,
};
use std::time::Duration;

const GITHUB_ACCEPT: &str = "application/vnd.github+json";
const GITHUB_API_VERSION: &str = "2022-11-28";

pub fn client() -> Result<Client> {
    Client::builder()
        .user_agent(concat!("ossia/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(8))
        .redirect(Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || !secure(attempt.url()) {
                attempt.error("invalid update redirect")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|error| Error::other(error.to_string()))
}

#[cfg(test)]
pub fn no_redirect_client() -> Result<Client> {
    Client::builder()
        .user_agent(concat!("ossia/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(8))
        .redirect(Policy::none())
        .build()
        .map_err(|error| Error::other(error.to_string()))
}

pub fn api_get(client: &Client, url: &str, token: Option<&str>) -> Result<RequestBuilder> {
    request(client, url, token, GITHUB_ACCEPT)
}

fn request(
    client: &Client,
    url: &str,
    token: Option<&str>,
    accept: &str,
) -> Result<RequestBuilder> {
    ensure_secure(url)?;
    let request = client
        .get(url)
        .header(ACCEPT, accept)
        .header("X-GitHub-Api-Version", GITHUB_API_VERSION);
    let Some(token) = token else {
        return Ok(request);
    };
    if !token_destination(url)? {
        return Err(Error::other(
            "refusing to send a GitHub token outside https://api.github.com",
        ));
    }
    Ok(request.header(AUTHORIZATION, format!("Bearer {token}")))
}

#[cfg(test)]
pub fn asset_get(client: &Client, url: &str, token: Option<&str>) -> Result<RequestBuilder> {
    request(client, url, token, "application/octet-stream")
}

pub fn ensure_secure(url: &str) -> Result<()> {
    let parsed = Url::parse(url).map_err(|error| Error::other(format!("update URL: {error}")))?;
    if secure(&parsed) {
        Ok(())
    } else {
        Err(Error::other(format!(
            "refusing to download an update over {}",
            parsed.scheme()
        )))
    }
}

pub fn ensure_public_https(url: &str) -> Result<()> {
    let parsed = Url::parse(url).map_err(|error| Error::other(format!("URL: {error}")))?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(Error::other("only HTTPS links can be opened"));
    }
    Ok(())
}

pub fn token_destination(url: &str) -> Result<bool> {
    let parsed = Url::parse(url).map_err(|error| Error::other(format!("update URL: {error}")))?;
    Ok(
        (parsed.scheme() == "https" && parsed.host_str() == Some("api.github.com"))
            || (parsed.scheme() == "http" && loopback(&parsed)),
    )
}

fn secure(url: &Url) -> bool {
    url.scheme() == "https" || (url.scheme() == "http" && loopback(url))
}

fn loopback(url: &Url) -> bool {
    url.host_str().is_some_and(|host| {
        let bare = host
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
            .unwrap_or(host);
        bare == "localhost"
            || bare
                .parse::<std::net::IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    })
}
