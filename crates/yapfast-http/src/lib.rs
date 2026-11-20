//! Every HTTPS request and WebSocket Yapfast makes, presenting as one
//! browser: Firefox, on this machine's operating system. wreq makes the
//! connections over BoringSSL, and wreq-util's profile of that Firefox gives
//! what the connection looks like: the TLS ClientHello (JA3, JA4), the
//! HTTP/2 SETTINGS, window update, priorities and pseudo-header order (the
//! Akamai fingerprint), and the headers a typed address sends. [`PROFILE`] and
//! [`PLATFORM`] choose the profile, and everything else that names the browser
//! comes from it: [`user_agent`], [`version`], [`accept_language`], and the
//! headers of each [`Kind`] of request, which go out in Firefox's order,
//! [`ORDER`], whatever order the code adds them in. Discord's client
//! properties (yapfast-discord's `client.rs`) name the browser from here too,
//! so the two can't disagree.
//!
//! The headers of the kinds other than [`Kind::Navigate`] follow Firefox's
//! networking code (`netwerk/protocol/http`, `dom/security/SecFetch.cpp`,
//! `image/imgLoader.cpp`, `dom/media`) and the Fetch standard. The
//! `yapfast-fingerprint` program of yapfast-discord prints what an echo
//! service sees of a request, to hold against a real Firefox of the same
//! version.

use std::{net::IpAddr, sync::OnceLock, time::Duration};

use bytes::Bytes;
use http_body_util::BodyExt;
pub use wreq;
use wreq::{
    Client, ClientBuilder, IntoEmulation, Method, RequestBuilder, Response, Uri, Version,
    header::{
        ACCEPT, ACCEPT_ENCODING, ACCEPT_LANGUAGE, CACHE_CONTROL, CONNECTION, HeaderMap, HeaderName,
        HeaderValue, ORIGIN, OrigHeaderMap, PRAGMA, REFERER, SEC_WEBSOCKET_VERSION, TE, UPGRADE,
        USER_AGENT,
    },
    http1::Http1Options,
    http2::Http2Options,
    tls::{TlsOptions, trust::CertStore},
};
pub use wreq_util::{Platform, Profile};

/// The browser Yapfast is: the newest Firefox wreq-util has a profile of.
/// Moving on is this line and wreq-util's version in Cargo.toml; the version
/// Discord is told follows by itself.
pub const PROFILE: Profile = Profile::Firefox151;

/// The operating system the browser runs on: this machine's.
#[cfg(windows)]
pub const PLATFORM: Platform = Platform::Windows;
/// The operating system the browser runs on: this machine's.
#[cfg(not(windows))]
pub const PLATFORM: Platform = Platform::Linux;

/// Every header Yapfast's requests carry, in the order Firefox writes them,
/// spelled as HTTP/1.1 spells them; HTTP/2 lowercases them. A request's
/// headers go out in this order, whatever order the code adds them in, and a
/// header missing here would go after them all, so each one a request adds
/// belongs here.
///
/// Firefox writes its headers in the order its code sets them: the standard
/// four as the channel opens (`nsHttpHandler::AddStandardRequestHeaders`),
/// the page's own in the order the page sets them, then Content-Length,
/// Origin, Connection, Referer and Cookie, Sec-Fetch-* (`SecFetch.cpp`), the
/// cache's, Priority, and Upgrade as the transaction is made; TE: trailers
/// comes last, and only on HTTP/2 (`Http2Compression.cpp`).
pub const ORDER: &[&str] = &[
    "Host",
    "User-Agent",
    "Accept",
    "Accept-Language",
    "Accept-Encoding",
    "Range",
    // Discord's web client's own, in the order it sets them.
    "Content-Type",
    "Authorization",
    "X-Super-Properties",
    "X-Discord-Locale",
    "X-Discord-Timezone",
    "X-Debug-Options",
    "X-Discord-MFA-Authorization",
    "X-Context-Properties",
    "X-Audit-Log-Reason",
    "X-Captcha-Key",
    "X-Captcha-Rqtoken",
    "X-Captcha-Session-Id",
    "Sec-WebSocket-Version",
    "Content-Length",
    "Origin",
    "Sec-WebSocket-Extensions",
    "Sec-WebSocket-Key",
    "Connection",
    "Referer",
    "Cookie",
    "Upgrade-Insecure-Requests",
    "Sec-Fetch-Dest",
    "Sec-Fetch-Mode",
    "Sec-Fetch-Site",
    "Sec-Fetch-User",
    "If-None-Match",
    "Pragma",
    "Cache-Control",
    "Priority",
    "TE",
    "Upgrade",
];

/// Firefox's Accept-Encoding over plain HTTP (`network.http.accept-encoding`);
/// HTTPS takes the profile's, which adds br and zstd.
const PLAIN_ENCODINGS: &str = "gzip, deflate";
/// The Accept of a script's request, a WebSocket's and whatever else Firefox
/// has no type for (`ACCEPT_HEADER_ALL`).
const ANY: &str = "*/*";
/// The Accept of a picture (`nsHttpHandler.cpp`'s `ImageAcceptHeader`).
const IMAGES: &str = "image/avif,image/webp,image/png,image/svg+xml,image/*;q=0.8,*/*;q=0.5";
/// The Accept of a video (`HTMLVideoElement::SetAcceptHeader`).
const VIDEOS: &str =
    "video/webm,video/ogg,video/*;q=0.9,application/ogg;q=0.7,audio/*;q=0.6,*/*;q=0.5";
/// A picture's Priority: loaded at low priority, a step below Firefox's
/// default urgency of 4, and incrementally (`imgLoader.cpp`).
const IMAGE_PRIORITY: &str = "u=5, i";
/// A video's Priority: Firefox's default urgency. A script's request and a
/// WebSocket have the "unblocked" class, urgency 3, which sends none.
const VIDEO_PRIORITY: &str = "u=4";
/// How often a connection's HTTP/2 ping goes while requests wait on it, and
/// how long one may go unanswered before the connection closes, so the next
/// request opens a new one rather than waiting on a lost one.
const PING: Duration = Duration::from_secs(10);
/// How long [`web`]'s client waits for a connection.
const CONNECT: Duration = Duration::from_secs(15);

static SEC_FETCH_DEST: HeaderName = HeaderName::from_static("sec-fetch-dest");
static SEC_FETCH_MODE: HeaderName = HeaderName::from_static("sec-fetch-mode");
static SEC_FETCH_SITE: HeaderName = HeaderName::from_static("sec-fetch-site");
static PRIORITY: HeaderName = HeaderName::from_static("priority");

/// The profile, read once.
struct Browser {
    /// Its TLS, HTTP/1 and HTTP/2 settings.
    tls: Option<TlsOptions>,
    http1: Option<Http1Options>,
    http2: Option<Http2Options>,
    /// Its headers, in its order: what typing an address sends.
    navigation: HeaderMap,
    /// The version its User-Agent names, such as `151.0`.
    version: String,
}

fn browser() -> &'static Browser {
    static BROWSER: OnceLock<Browser> = OnceLock::new();
    BROWSER.get_or_init(|| {
        let mut emulation = wreq_util::Emulation::builder()
            .profile(PROFILE)
            .platform(PLATFORM)
            .build()
            .into_emulation();
        // Each request carries its kind's headers, so the clients keep none.
        let navigation = std::mem::take(&mut emulation.headers);
        // Pings go only while requests wait, and don't show in the SETTINGS
        // a server fingerprints.
        if let Some(http2) = emulation.http2_options.as_mut() {
            http2.keep_alive_interval = Some(PING);
            http2.keep_alive_timeout = PING;
        }
        let version = navigation
            .get(USER_AGENT)
            .and_then(|agent| agent.to_str().ok())
            .and_then(firefox_version)
            .expect("wreq-util's Firefox profiles name their version in the User-Agent")
            .to_owned();
        Browser {
            tls: emulation.tls_options,
            http1: emulation.http1_options,
            http2: emulation.http2_options,
            navigation,
            version,
        }
    })
}

/// The version a Firefox User-Agent names, such as `151.0`.
fn firefox_version(agent: &str) -> Option<&str> {
    let version = agent.split("Firefox/").nth(1)?.split_whitespace().next()?;
    (!version.is_empty()).then_some(version)
}

/// One of the profile's own headers.
fn profile_header(name: HeaderName) -> &'static HeaderValue {
    browser().navigation.get(name).expect(
        "wreq-util's Firefox profiles carry User-Agent, Accept-Language, Accept-Encoding and TE",
    )
}

/// The browser's User-Agent.
pub fn user_agent() -> &'static str {
    profile_header(USER_AGENT).to_str().unwrap_or_default()
}

/// The browser's version, as its User-Agent names it: `151.0`.
pub fn version() -> &'static str {
    &browser().version
}

/// The languages the browser asks for, an English Firefox's: `en-US,en;q=0.5`.
pub fn accept_language() -> &'static str {
    profile_header(ACCEPT_LANGUAGE).to_str().unwrap_or_default()
}

/// What a request is, to the browser making it. A page's requests name the
/// page they come from, the address in its tab: Origin and Referer come from
/// it, and Sec-Fetch-Site from how the request's address stands to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// An address typed into a tab: the profile's own headers, from no page.
    Navigate,
    /// A page's script asking with `XMLHttpRequest` or `fetch()`, as
    /// Discord's web client calls its API, or Spotify's.
    Script(&'static str),
    /// A picture a page shows.
    Image(&'static str),
    /// A video a page plays, asked for in parts with Range. Firefox offers no
    /// encodings for it (`HTMLMediaElement::SetRequestHeaders`).
    Video(&'static str),
    /// A page's WebSocket: an HTTP/1.1 upgrade, to an `http` or `https`
    /// address. The caller adds Sec-WebSocket-Key.
    WebSocket(&'static str),
}

/// How a request's address stands to its page's, as Sec-Fetch-Site says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Site {
    SameOrigin,
    SameSite,
    CrossSite,
}

impl Site {
    fn of(page: Option<&Uri>, target: Option<&Uri>) -> Site {
        let (Some(page), Some(target)) = (page.and_then(Place::of), target.and_then(Place::of))
        else {
            return Site::CrossSite;
        };
        if page == target {
            Site::SameOrigin
        } else if page.scheme == target.scheme
            && registrable(&page.host) == registrable(&target.host)
        {
            Site::SameSite
        } else {
            Site::CrossSite
        }
    }

    fn name(self) -> &'static str {
        match self {
            Site::SameOrigin => "same-origin",
            Site::SameSite => "same-site",
            Site::CrossSite => "cross-site",
        }
    }
}

/// An address's origin: its scheme, as HTTP names it, host and port.
#[derive(PartialEq, Eq)]
struct Place {
    scheme: &'static str,
    host: String,
    port: u16,
}

impl Place {
    fn of(uri: &Uri) -> Option<Place> {
        let (scheme, port) = match uri.scheme_str()? {
            "https" | "wss" => ("https", 443),
            "http" | "ws" => ("http", 80),
            _ => return None,
        };
        Some(Place {
            scheme,
            host: uri.host()?.to_ascii_lowercase(),
            port: uri.port_u16().unwrap_or(port),
        })
    }
}

/// The site a host belongs to: its last two labels. Without the public
/// suffix list this is wrong for hosts under such as `co.uk`, but right for
/// every host Yapfast asks: discord.com, discordapp.com, discord.gg,
/// discord.media, spotify.com, github.com and the like.
fn registrable(host: &str) -> &str {
    if host.starts_with('[') || host.parse::<IpAddr>().is_ok() {
        return host;
    }
    let mut dots = host.rmatch_indices('.').map(|(at, _)| at);
    dots.next();
    match dots.next() {
        Some(at) => &host[at + 1..],
        None => host,
    }
}

fn is_secure(uri: &Uri) -> bool {
    matches!(uri.scheme_str(), Some("https" | "wss"))
}

/// Whether Firefox sends Sec-Fetch-* to `uri`: a potentially trustworthy
/// origin, HTTPS or this machine.
fn trustworthy(uri: &Uri) -> bool {
    if is_secure(uri) {
        return true;
    }
    let host = uri.host().unwrap_or_default();
    let host = host.trim_start_matches('[').trim_end_matches(']');
    host.eq_ignore_ascii_case("localhost")
        || host.to_ascii_lowercase().ends_with(".localhost")
        || host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
}

/// An origin as Origin and a cross-origin Referer give it.
fn origin_of(uri: &Uri) -> Option<String> {
    Some(format!("{}://{}", uri.scheme_str()?, uri.authority()?))
}

/// The headers Firefox gives a request of `kind` to `url`, in Firefox's
/// order; the request adds its own, such as Content-Type or Range, and wreq
/// adds Host, Content-Length and Cookie. Referer follows Firefox's default
/// policy, `strict-origin-when-cross-origin`.
pub fn headers(kind: Kind, method: &Method, url: &str) -> HeaderMap {
    let target = url.parse::<Uri>().ok();
    let secure = target.as_ref().is_some_and(is_secure);
    let page = match kind {
        Kind::Navigate => {
            let mut headers = browser().navigation.clone();
            if !secure {
                headers.insert(ACCEPT_ENCODING, HeaderValue::from_static(PLAIN_ENCODINGS));
                headers.remove(TE);
            }
            return headers;
        }
        Kind::Script(page) | Kind::Image(page) | Kind::Video(page) | Kind::WebSocket(page) => page,
    };
    let page_uri = page.parse::<Uri>().ok();
    let site = Site::of(page_uri.as_ref(), target.as_ref());
    let websocket = matches!(kind, Kind::WebSocket(_));
    let mut headers = HeaderMap::with_capacity(16);
    headers.insert(USER_AGENT, profile_header(USER_AGENT).clone());
    let accept = match kind {
        Kind::Image(_) => IMAGES,
        Kind::Video(_) => VIDEOS,
        _ => ANY,
    };
    headers.insert(ACCEPT, HeaderValue::from_static(accept));
    headers.insert(ACCEPT_LANGUAGE, profile_header(ACCEPT_LANGUAGE).clone());
    if !matches!(kind, Kind::Video(_)) {
        let encodings = if secure {
            profile_header(ACCEPT_ENCODING).clone()
        } else {
            HeaderValue::from_static(PLAIN_ENCODINGS)
        };
        headers.insert(ACCEPT_ENCODING, encodings);
    }
    if websocket {
        headers.insert(SEC_WEBSOCKET_VERSION, HeaderValue::from_static("13"));
    }
    let origin = page_uri.as_ref().and_then(origin_of);
    // A CORS request names its page's origin, and so does a same-origin one
    // that isn't GET or HEAD; a WebSocket always does, and a picture or a
    // video, which go without CORS, never.
    let names_origin = match kind {
        Kind::WebSocket(_) => true,
        Kind::Script(_) => {
            site != Site::SameOrigin || !matches!(*method, Method::GET | Method::HEAD)
        }
        _ => false,
    };
    if names_origin
        && let Some(value) = origin
            .as_deref()
            .and_then(|o| o.parse::<HeaderValue>().ok())
    {
        headers.insert(ORIGIN, value);
    }
    if websocket {
        headers.insert(CONNECTION, HeaderValue::from_static("keep-alive, Upgrade"));
    } else {
        // A WebSocket has no referrer. Nothing goes from an HTTPS page to
        // plain HTTP; the whole address to the page's own origin; its origin
        // anywhere else.
        let downgrade = page_uri.as_ref().is_some_and(is_secure) && !secure;
        let referer = match site {
            _ if downgrade => None,
            Site::SameOrigin => Some(page.to_owned()),
            _ => origin.map(|origin| origin + "/"),
        };
        if let Some(value) = referer.and_then(|referer| referer.parse::<HeaderValue>().ok()) {
            headers.insert(REFERER, value);
        }
    }
    if target.as_ref().is_some_and(trustworthy) {
        let (dest, mode) = match kind {
            Kind::Image(_) => ("image", "no-cors"),
            Kind::Video(_) => ("video", "no-cors"),
            Kind::WebSocket(_) => ("empty", "websocket"),
            Kind::Script(_) | Kind::Navigate => ("empty", "cors"),
        };
        headers.insert(&SEC_FETCH_DEST, HeaderValue::from_static(dest));
        headers.insert(&SEC_FETCH_MODE, HeaderValue::from_static(mode));
        headers.insert(&SEC_FETCH_SITE, HeaderValue::from_static(site.name()));
    }
    match kind {
        Kind::WebSocket(_) => {
            // Its cache mode is no-store, which Firefox sends as these.
            headers.insert(PRAGMA, HeaderValue::from_static("no-cache"));
            headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
            headers.insert(UPGRADE, HeaderValue::from_static("websocket"));
        }
        Kind::Image(_) => {
            headers.insert(&PRIORITY, HeaderValue::from_static(IMAGE_PRIORITY));
        }
        Kind::Video(_) => {
            headers.insert(&PRIORITY, HeaderValue::from_static(VIDEO_PRIORITY));
        }
        Kind::Script(_) | Kind::Navigate => {}
    }
    // Firefox adds it on HTTP/2, which an HTTPS address almost always is, and
    // wreq keeps it there, as HTTP/2 allows `te: trailers`.
    if secure && !websocket {
        headers.insert(TE, profile_header(TE).clone());
    }
    headers
}

/// A request of `kind` through `client`, with [`headers`]. A WebSocket's goes
/// over HTTP/1.1, with an ALPN of `http/1.1` alone: wreq offers the protocol
/// the request asks for.
pub fn request(client: &Client, kind: Kind, method: Method, url: &str) -> RequestBuilder {
    let headers = headers(kind, &method, url);
    let request = client.request(method, url).headers(headers);
    match kind {
        // Without its decoders wreq offers no encodings of its own, where it
        // would offer `identity` for a Range.
        Kind::Video(_) => request.gzip(false).brotli(false).deflate(false).zstd(false),
        // A browser's WebSocket fails on a redirect rather than following it.
        Kind::WebSocket(_) => request
            .version(Version::HTTP_11)
            .redirect(wreq::redirect::Policy::none()),
        Kind::Navigate | Kind::Script(_) | Kind::Image(_) => request,
    }
}

/// The certificates the system trusts, read once, as every connection
/// trusted them before: a CA added for a reverse proxy works. When the
/// system gives none, BoringSSL looks where `SSL_CERT_FILE` and
/// `SSL_CERT_DIR` say, or in its own default places.
fn trust() -> Option<&'static CertStore> {
    static STORE: OnceLock<Option<CertStore>> = OnceLock::new();
    STORE
        .get_or_init(|| {
            let found = rustls_native_certs::load_native_certs();
            for error in &found.errors {
                tracing::warn!(%error, "could not read some of the system's certificates");
            }
            if found.certs.is_empty() {
                tracing::warn!("the system named no certificates to trust");
                return None;
            }
            CertStore::from_der_certs(found.certs.iter())
                .inspect_err(
                    |error| tracing::warn!(%error, "could not take the system's certificates"),
                )
                .ok()
        })
        .as_ref()
}

/// A client that presents as the browser: the profile's TLS and HTTP/2, the
/// headers in [`ORDER`], the system's certificates, up to ten redirects, and
/// proxies from the environment. The caller adds a cookie store, timeouts
/// and the like, and each request takes its headers from [`request`].
pub fn builder() -> ClientBuilder {
    let browser = browser();
    let mut order = OrigHeaderMap::with_capacity(ORDER.len());
    for name in ORDER {
        order.insert(*name);
    }
    let builder = Client::builder()
        .tls_options(browser.tls.clone())
        .http1_options(browser.http1.clone())
        .http2_options(browser.http2.clone())
        .orig_headers(order)
        .redirect(wreq::redirect::Policy::default());
    match trust() {
        Some(store) => builder.tls_cert_store(store.clone()),
        None => builder,
    }
}

/// The client for requests to anyone but Discord, shared: no cookies, so
/// Discord's never reach another site and another site's aren't kept.
pub fn web() -> anyhow::Result<&'static Client> {
    static WEB: OnceLock<Result<Client, String>> = OnceLock::new();
    WEB.get_or_init(|| {
        builder()
            .connect_timeout(CONNECT)
            .build()
            .map_err(|error| error.to_string())
    })
    .as_ref()
    .map_err(|error| anyhow::anyhow!("could not set up HTTPS: {error}"))
}

/// The next part of a response's body as it arrives, or none at its end.
pub async fn chunk(response: &mut Response) -> wreq::Result<Option<Bytes>> {
    while let Some(frame) = response.frame().await {
        if let Ok(data) = frame?.into_data()
            && !data.is_empty()
        {
            return Ok(Some(data));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "https://discord.com/channels/@me";

    fn get(headers: &HeaderMap, name: &str) -> Option<String> {
        headers
            .get(name)
            .map(|value| value.to_str().unwrap().to_owned())
    }

    /// The names of `headers`, as they'd go out: in [`ORDER`].
    fn sent(headers: &HeaderMap) -> Vec<String> {
        ORDER
            .iter()
            .filter(|name| headers.contains_key(**name))
            .map(|name| name.to_ascii_lowercase())
            .collect()
    }

    #[test]
    fn the_profile_names_the_version_and_platform() {
        let major = version().split('.').next().unwrap();
        assert_eq!(format!("{PROFILE:?}"), format!("Firefox{major}"));
        assert!(
            user_agent().contains(&format!("rv:{}", version())),
            "{}",
            user_agent()
        );
        assert!(
            user_agent().ends_with(&format!("Firefox/{}", version())),
            "{}",
            user_agent()
        );
        #[cfg(windows)]
        assert!(user_agent().contains("Windows NT"), "{}", user_agent());
        #[cfg(not(windows))]
        assert!(user_agent().contains("Linux"), "{}", user_agent());
        assert_eq!(accept_language(), "en-US,en;q=0.5");
        assert_eq!(
            firefox_version("Mozilla/5.0 Gecko/20100101 Firefox/151.0"),
            Some("151.0")
        );
        assert_eq!(firefox_version("Mozilla/5.0 Chrome/140.0.0.0"), None);
    }

    /// Every header any kind carries has its place in [`ORDER`] and is there
    /// once, and the browser's own are the profile's, word for word.
    #[test]
    fn every_header_has_its_place_and_the_profiles_words() {
        let kinds = [
            Kind::Navigate,
            Kind::Script(PAGE),
            Kind::Image(PAGE),
            Kind::Video(PAGE),
            Kind::WebSocket(PAGE),
        ];
        let lower: Vec<String> = ORDER.iter().map(|name| name.to_ascii_lowercase()).collect();
        let mut unique = lower.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), lower.len(), "a header is in ORDER twice");
        for kind in kinds {
            for url in [
                "https://discord.com/api/v9/x",
                "https://cdn.discordapp.com/a.png",
            ] {
                for method in [Method::GET, Method::POST] {
                    let headers = headers(kind, &method, url);
                    assert_eq!(
                        headers.len(),
                        headers.keys_len(),
                        "{kind:?} repeats a header"
                    );
                    for name in headers.keys() {
                        assert!(
                            lower.contains(&name.as_str().to_owned()),
                            "{name} has no place"
                        );
                    }
                    assert_eq!(get(&headers, "user-agent").as_deref(), Some(user_agent()));
                    if !matches!(kind, Kind::Navigate | Kind::Video(_)) {
                        assert_eq!(
                            headers.get(ACCEPT_ENCODING),
                            browser().navigation.get(ACCEPT_ENCODING),
                        );
                    }
                }
            }
        }
        // A typed address sends the profile's headers as they are.
        let typed = headers(Kind::Navigate, &Method::GET, "https://discord.com/app");
        let profile: Vec<_> = browser().navigation.iter().collect();
        assert_eq!(typed.iter().collect::<Vec<_>>(), profile);
        assert_eq!(
            sent(&typed),
            [
                "user-agent",
                "accept",
                "accept-language",
                "accept-encoding",
                "upgrade-insecure-requests",
                "sec-fetch-dest",
                "sec-fetch-mode",
                "sec-fetch-site",
                "sec-fetch-user",
                "priority",
                "te",
            ]
        );
        assert_eq!(
            get(&typed, "accept-encoding").unwrap(),
            "gzip, deflate, br, zstd"
        );
    }

    #[test]
    fn a_script_names_its_origin_as_firefox_does() {
        let api = "https://discord.com/api/v9/users/@me";
        let read = headers(Kind::Script(PAGE), &Method::GET, api);
        assert_eq!(get(&read, "accept").unwrap(), "*/*");
        assert_eq!(get(&read, "origin"), None, "a same-origin GET");
        assert_eq!(get(&read, "referer").unwrap(), PAGE);
        assert_eq!(get(&read, "sec-fetch-dest").unwrap(), "empty");
        assert_eq!(get(&read, "sec-fetch-mode").unwrap(), "cors");
        assert_eq!(get(&read, "sec-fetch-site").unwrap(), "same-origin");
        assert_eq!(get(&read, "priority"), None);
        assert_eq!(get(&read, "te").unwrap(), "trailers");
        let write = headers(Kind::Script(PAGE), &Method::POST, api);
        assert_eq!(get(&write, "origin").unwrap(), "https://discord.com");
        assert_eq!(
            sent(&write),
            [
                "user-agent",
                "accept",
                "accept-language",
                "accept-encoding",
                "origin",
                "referer",
                "sec-fetch-dest",
                "sec-fetch-mode",
                "sec-fetch-site",
                "te",
            ]
        );
        // Another site's API: CORS, with the origin alone as the referrer.
        let spotify = headers(
            Kind::Script(PAGE),
            &Method::GET,
            "https://api.spotify.com/v1/me/player",
        );
        assert_eq!(get(&spotify, "origin").unwrap(), "https://discord.com");
        assert_eq!(get(&spotify, "referer").unwrap(), "https://discord.com/");
        assert_eq!(get(&spotify, "sec-fetch-site").unwrap(), "cross-site");
        let sibling = headers(
            Kind::Script(PAGE),
            &Method::GET,
            "https://status.discord.com/x",
        );
        assert_eq!(get(&sibling, "sec-fetch-site").unwrap(), "same-site");
    }

    #[test]
    fn pictures_videos_and_websockets_carry_their_own() {
        let picture = headers(
            Kind::Image(PAGE),
            &Method::GET,
            "https://cdn.discordapp.com/avatars/1/a.png",
        );
        assert!(
            get(&picture, "accept")
                .unwrap()
                .starts_with("image/avif,image/webp")
        );
        assert_eq!(get(&picture, "origin"), None);
        assert_eq!(get(&picture, "referer").unwrap(), "https://discord.com/");
        assert_eq!(get(&picture, "sec-fetch-dest").unwrap(), "image");
        assert_eq!(get(&picture, "sec-fetch-mode").unwrap(), "no-cors");
        assert_eq!(get(&picture, "sec-fetch-site").unwrap(), "cross-site");
        assert_eq!(get(&picture, "priority").unwrap(), "u=5, i");

        let video = headers(
            Kind::Video(PAGE),
            &Method::GET,
            "https://cdn.discordapp.com/attachments/1/2/a.mp4",
        );
        assert!(get(&video, "accept").unwrap().starts_with("video/webm"));
        assert_eq!(get(&video, "accept-encoding"), None);
        assert_eq!(get(&video, "sec-fetch-dest").unwrap(), "video");

        let socket = headers(
            Kind::WebSocket(PAGE),
            &Method::GET,
            "https://gateway.discord.gg/?v=9",
        );
        assert_eq!(get(&socket, "origin").unwrap(), "https://discord.com");
        assert_eq!(get(&socket, "referer"), None);
        assert_eq!(get(&socket, "connection").unwrap(), "keep-alive, Upgrade");
        assert_eq!(get(&socket, "upgrade").unwrap(), "websocket");
        assert_eq!(get(&socket, "sec-fetch-mode").unwrap(), "websocket");
        assert_eq!(get(&socket, "te"), None);
        assert_eq!(
            sent(&socket),
            [
                "user-agent",
                "accept",
                "accept-language",
                "accept-encoding",
                "sec-websocket-version",
                "origin",
                "connection",
                "sec-fetch-dest",
                "sec-fetch-mode",
                "sec-fetch-site",
                "pragma",
                "cache-control",
                "upgrade",
            ]
        );
    }

    #[test]
    fn plain_http_offers_less_and_gets_no_referrer_from_https() {
        let local = headers(
            Kind::Script(PAGE),
            &Method::GET,
            "http://127.0.0.1:18400/api/v9/x",
        );
        assert_eq!(get(&local, "accept-encoding").unwrap(), "gzip, deflate");
        assert_eq!(get(&local, "referer"), None);
        assert_eq!(get(&local, "te"), None);
        // This machine is trustworthy, so it still gets Sec-Fetch-*.
        assert_eq!(get(&local, "sec-fetch-site").unwrap(), "cross-site");
        let elsewhere = headers(Kind::Image(PAGE), &Method::GET, "http://example.com/a.png");
        assert_eq!(get(&elsewhere, "sec-fetch-dest"), None);
        let typed = headers(Kind::Navigate, &Method::GET, "http://127.0.0.1:18400/app");
        assert_eq!(get(&typed, "accept-encoding").unwrap(), "gzip, deflate");
        assert_eq!(get(&typed, "te"), None);
    }

    #[test]
    fn sites_by_their_last_two_labels() {
        assert_eq!(registrable("gateway.discord.gg"), "discord.gg");
        assert_eq!(registrable("discord.com"), "discord.com");
        assert_eq!(registrable("localhost"), "localhost");
        assert_eq!(registrable("127.0.0.1"), "127.0.0.1");
    }

    /// What wreq writes on the wire, over HTTP/1.1 where the order and the
    /// spelling both show: Firefox's, whatever order the request added them in.
    #[tokio::test]
    async fn a_request_goes_out_in_firefoxs_order() {
        use tokio::{
            io::{AsyncReadExt, AsyncWriteExt},
            net::TcpListener,
        };
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let head = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut read = Vec::new();
            let head = loop {
                let mut chunk = [0; 4096];
                let got = socket.read(&mut chunk).await.unwrap();
                read.extend_from_slice(&chunk[..got]);
                let text = String::from_utf8_lossy(&read).into_owned();
                if let Some((head, _)) = text.split_once("\r\n\r\n") {
                    break head.to_owned();
                }
                assert_ne!(got, 0, "the request ended early");
            };
            socket
                .write_all(b"HTTP/1.1 204 No Content\r\nconnection: close\r\n\r\n")
                .await
                .unwrap();
            head
        });
        let page: &'static str = Box::leak(format!("http://{address}/app").into_boxed_str());
        let client = builder().build().unwrap();
        let response = request(
            &client,
            Kind::Script(page),
            Method::POST,
            &format!("http://{address}/api"),
        )
        .header("X-Super-Properties", "e30=")
        .header("content-type", "application/json")
        .header("authorization", "token")
        .body("{}")
        .send()
        .await
        .unwrap();
        assert_eq!(response.status(), 204);
        let head = head.await.unwrap();
        let names: Vec<&str> = head
            .lines()
            .skip(1)
            .map(|line| line.split_once(':').unwrap().0)
            .collect();
        assert_eq!(
            names,
            [
                "Host",
                "User-Agent",
                "Accept",
                "Accept-Language",
                "Accept-Encoding",
                "Content-Type",
                "Authorization",
                "X-Super-Properties",
                "Content-Length",
                "Origin",
                "Referer",
                "Sec-Fetch-Dest",
                "Sec-Fetch-Mode",
                "Sec-Fetch-Site",
            ],
            "{head}"
        );
    }
}
