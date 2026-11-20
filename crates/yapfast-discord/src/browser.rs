//! Discord's web client as the browser runs it: the page its requests come
//! from, and the client that asks Discord's hosts. Both present as the
//! browser [`yapfast_http`] makes every connection as; requests to anyone
//! else go through [`yapfast_http::web`], without Discord's cookies.

use std::{sync::OnceLock, time::Duration};

use anyhow::{Context, Result, anyhow};
use yapfast_http::{
    Kind,
    wreq::{Client, Method, Version},
};

/// The page Discord's web client runs at, which its requests come from:
/// their Origin and Referer name it, and Sec-Fetch-Site says how their
/// address stands to it.
pub(crate) const PAGE: &str = "https://discord.com/channels/@me";
/// The web client's script asking Discord's API, or another site's.
pub(crate) const SCRIPT: Kind = Kind::Script(PAGE);
/// A picture the web client shows.
pub(crate) const IMAGE: Kind = Kind::Image(PAGE);
/// A video the web client plays, a part at a time.
pub(crate) const VIDEO: Kind = Kind::Video(PAGE);
/// A WebSocket the web client opens.
pub(crate) const SOCKET: Kind = Kind::WebSocket(PAGE);

/// How long a connection may take to open.
const CONNECT: Duration = Duration::from_secs(15);

/// The client for Discord's hosts and the test server, shared by the
/// process: Discord's cookies ([`crate::cookies`]), and one pool of
/// connections, so the requests to a host share one HTTP/2 connection, as a
/// browser's do, and the WebSockets the same TLS.
pub(crate) fn discord() -> Result<&'static Client> {
    static CLIENT: OnceLock<Result<Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            yapfast_http::builder()
                .cookie_provider(crate::cookies::jar())
                .connect_timeout(CONNECT)
                .build()
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|error| anyhow!("could not set up HTTPS: {error}"))
}

/// How [`echo`] asks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Echo {
    /// As an address typed into a tab: what opening it in Firefox sends.
    Page,
    /// As the web client's script would, from `PAGE`.
    Script,
    /// As a picture on `PAGE`.
    Image,
    /// As a typed address over HTTP/1.1, the ALPN of `http/1.1` alone that
    /// the WebSockets' connections offer.
    Http1,
}

/// One GET of `url` through the client Discord's requests go through, with
/// no login, no super properties and no cookie Discord set, returning the
/// status and the body: for an echo service to show what the connection
/// looks like (`src/bin/yapfast-fingerprint.rs`).
pub async fn echo(url: &str, how: Echo) -> Result<(u16, String)> {
    let client = discord()?;
    let request = match how {
        Echo::Page => yapfast_http::request(client, Kind::Navigate, Method::GET, url),
        Echo::Script => yapfast_http::request(client, SCRIPT, Method::GET, url),
        Echo::Image => yapfast_http::request(client, IMAGE, Method::GET, url),
        Echo::Http1 => yapfast_http::request(client, Kind::Navigate, Method::GET, url)
            .version(Version::HTTP_11),
    };
    let response = request
        .send()
        .await
        .context("the echo service didn't answer")?;
    let status = response.status().as_u16();
    let body = response
        .text()
        .await
        .context("the echo service's answer broke off")?;
    Ok((status, body))
}
