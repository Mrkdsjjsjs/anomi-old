//! Cookies Discord's servers set, sent back on later requests as any HTTP
//! client that keeps cookies would. Discord sets device cookies on its first
//! answer, and Cloudflare, in front of it, sets its own for rate limiting and
//! bot management. Dropping them makes every request look like a new device.
//!
//! One jar serves every Discord request in the process, as a browser's does.
//! Persistent cookies are saved in Yapfast's data directory and load on the next
//! start; session cookies end with the process. With a test server the jar
//! stays in memory. Requests to other sites go through a client without it
//! ([`yapfast_http::web`]), so they neither get Discord's cookies nor leave
//! their own here.

use std::{
    io::Write,
    path::PathBuf,
    sync::{Arc, LazyLock, Mutex},
};

use cookie_store::{CookieStore, RawCookie};
use url::Url;
use yapfast_http::wreq::{
    Uri, Version,
    cookie::{CookieStore as Store, Cookies},
    header::HeaderValue,
};

use crate::client;

/// The cookies, and the file they're saved in.
#[derive(Default)]
pub struct Jar {
    store: Mutex<CookieStore>,
    file: Mutex<Option<PathBuf>>,
}

static JAR: LazyLock<Arc<Jar>> = LazyLock::new(Arc::default);

/// The process's jar, for an HTTP client.
pub(crate) fn jar() -> Arc<Jar> {
    JAR.clone()
}

/// Loads cookies saved in `file`, and saves them there from now on.
pub fn keep_in(file: PathBuf) {
    if client::test_server().is_some() {
        return;
    }
    match std::fs::File::open(&file) {
        Ok(saved) => match cookie_store::serde::json::load(std::io::BufReader::new(saved)) {
            Ok(store) => *JAR.store.lock().unwrap() = store,
            Err(error) => tracing::warn!(%error, "could not read the saved Discord cookies"),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!(%error, "could not open the saved Discord cookies"),
    }
    *JAR.file.lock().unwrap() = Some(file);
}

impl Jar {
    fn save(&self, store: &CookieStore) {
        let Some(file) = self.file.lock().unwrap().clone() else {
            return;
        };
        if let Err(error) = write(&file, store) {
            tracing::warn!(%error, "could not save the Discord cookies");
        }
    }
}

/// Writes the jar beside `file`, readable only by this user, then swaps it in.
fn write(file: &std::path::Path, store: &CookieStore) -> anyhow::Result<()> {
    let partial = file.with_extension("partial");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut writer = std::io::BufWriter::new(crate::cache::open_in_folder(&options, &partial)?);
    cookie_store::serde::json::save(store, &mut writer)
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    writer.flush()?;
    drop(writer);
    std::fs::rename(&partial, file)?;
    Ok(())
}

/// The request's address as the cookie store takes it.
fn url_of(uri: &Uri) -> Option<Url> {
    Url::parse(&uri.to_string()).ok()
}

impl Store for Jar {
    fn set_cookies(&self, headers: &mut dyn Iterator<Item = &HeaderValue>, uri: &Uri) {
        let Some(url) = url_of(uri) else {
            return;
        };
        let cookies: Vec<RawCookie<'static>> = headers
            .filter_map(|header| header.to_str().ok())
            .filter_map(|header| RawCookie::parse(header.to_owned()).ok())
            .collect();
        if cookies.is_empty() {
            return;
        }
        let mut store = self.store.lock().unwrap();
        store.store_response_cookies(cookies.into_iter(), &url);
        self.save(&store);
    }

    /// The cookies for `uri` as Firefox sends them: one `Cookie` header with
    /// them all over HTTP/1.1, and one field a cookie over HTTP/2, where
    /// Firefox "crumbles" the header (`Http2Compression.cpp`).
    fn cookies(&self, uri: &Uri, version: Version) -> Cookies {
        let Some(url) = url_of(uri) else {
            return Cookies::Empty;
        };
        let store = self.store.lock().unwrap();
        let pairs: Vec<String> = store
            .get_request_values(&url)
            .map(|(name, value)| format!("{name}={value}"))
            .collect();
        if pairs.is_empty() {
            return Cookies::Empty;
        }
        if version == Version::HTTP_2 {
            let crumbs: Vec<HeaderValue> = pairs
                .iter()
                .filter_map(|pair| HeaderValue::from_str(pair).ok())
                .collect();
            return Cookies::Uncompressed(crumbs);
        }
        HeaderValue::from_str(&pairs.join("; ")).map_or(Cookies::Empty, Cookies::Compressed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri(address: &str) -> Uri {
        address.parse().unwrap()
    }

    /// The cookies as one HTTP/1.1 header, or none.
    fn sent(jar: &Jar, address: &str) -> Option<String> {
        match jar.cookies(&uri(address), Version::HTTP_11) {
            Cookies::Compressed(value) => Some(value.to_str().unwrap().to_owned()),
            Cookies::Empty => None,
            _ => panic!("HTTP/1.1 takes one header"),
        }
    }

    #[test]
    fn sends_back_what_the_server_set_where_it_applies() {
        let jar = Jar::default();
        let api = "https://discord.com/api/v9/users/@me";
        jar.set_cookies(
            &mut [
                HeaderValue::from_static(
                    "__dcfduid=device; Max-Age=31536000; Path=/; Secure; HttpOnly",
                ),
                HeaderValue::from_static("__cf_bm=bot; Path=/; Domain=discord.com; Secure"),
            ]
            .iter(),
            &uri(api),
        );
        let sent_here = sent(&jar, api).unwrap();
        assert!(sent_here.contains("__dcfduid=device"), "{sent_here}");
        assert!(sent_here.contains("__cf_bm=bot"), "{sent_here}");
        // Over HTTP/2, a field each.
        let Cookies::Uncompressed(crumbs) = jar.cookies(&uri(api), Version::HTTP_2) else {
            panic!("HTTP/2 crumbles them");
        };
        assert_eq!(crumbs.len(), 2);
        // Not to other sites, nor over plain HTTP.
        assert_eq!(sent(&jar, "https://example.com/"), None);
        assert_eq!(sent(&jar, "http://discord.com/api/v9"), None);
    }

    #[test]
    fn saves_persistent_cookies_and_loads_them_again() {
        let file =
            std::env::temp_dir().join(format!("yapfast-cookies-{}.json", std::process::id()));
        let jar = Jar::default();
        *jar.file.lock().unwrap() = Some(file.clone());
        let api = Url::parse("https://discord.com/api/v9/users/@me").unwrap();
        jar.set_cookies(
            &mut [
                HeaderValue::from_static("__dcfduid=device; Max-Age=31536000; Path=/; Secure"),
                HeaderValue::from_static("session=gone; Path=/; Secure"),
            ]
            .iter(),
            &uri(api.as_str()),
        );
        let saved = std::fs::read_to_string(&file).unwrap();
        let store =
            cookie_store::serde::json::load(std::io::BufReader::new(saved.as_bytes())).unwrap();
        std::fs::remove_file(&file).unwrap();
        let names: Vec<_> = store
            .get_request_values(&api)
            .map(|(name, _)| name)
            .collect();
        assert_eq!(names, ["__dcfduid"]);
    }
}
