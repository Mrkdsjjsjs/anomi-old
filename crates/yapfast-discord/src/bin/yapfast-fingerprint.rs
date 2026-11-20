//! Developer check of what Yapfast's connections look like, run by hand. It
//! makes one GET, through the client Discord's requests go through, to a
//! fingerprint echo service named by its address, such as
//! `https://tls.peet.ws/api/all`, and prints the TLS fingerprints (JA3, JA4),
//! the HTTP/2 one (Akamai's format) and the headers in the order the service
//! saw them. It sends no login, no super properties and no cookie of
//! Discord's. Open the same address in the Firefox it names, on the same
//! system, and the two should match. It is not part of Yapfast; see
//! docs/discord/README.md#how-yapfast-presents-itself.

use anyhow::{Context, Result, bail};
use serde_json::Value;
use yapfast_discord::browser::{self, Echo};

const USAGE: &str = "usage: yapfast-fingerprint <echo address> [page|script|image|http1]
  page    as an address typed into a tab (the default): compare with opening
          the address in Firefox
  script  as Discord's web client's script asks, from https://discord.com
  image   as a picture on Discord's web client's page
  http1   as page, over HTTP/1.1: the ALPN the WebSockets' connections offer";

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(url) = args.first() else {
        bail!("{USAGE}");
    };
    let how = match args.get(1).map(String::as_str) {
        None | Some("page") => Echo::Page,
        Some("script") => Echo::Script,
        Some("image") => Echo::Image,
        Some("http1") => Echo::Http1,
        Some(_) => bail!("{USAGE}"),
    };
    println!(
        "Yapfast presents as {:?} on {:?}: {}",
        yapfast_http::PROFILE,
        yapfast_http::PLATFORM,
        yapfast_http::user_agent()
    );
    let (status, body) = browser::echo(url, how).await?;
    println!("{url} answered {status}");
    let Ok(seen) = serde_json::from_str::<Value>(&body) else {
        println!("{body}");
        return Ok(());
    };
    report(&seen).context("could not read the echo service's answer")?;
    Ok(())
}

/// Prints what tls.peet.ws's `/api/all` gives, and the whole answer of a
/// service that answers otherwise.
fn report(seen: &Value) -> Result<()> {
    let text = |value: &Value| value.as_str().map(str::to_owned);
    let fields = [
        ("HTTP version", &seen["http_version"]),
        ("JA3", &seen["tls"]["ja3"]),
        ("JA3 hash", &seen["tls"]["ja3_hash"]),
        ("JA4", &seen["tls"]["ja4"]),
        ("JA4_r", &seen["tls"]["ja4_r"]),
        ("PeetPrint hash", &seen["tls"]["peetprint_hash"]),
        ("Akamai HTTP/2", &seen["http2"]["akamai_fingerprint"]),
        ("Akamai hash", &seen["http2"]["akamai_fingerprint_hash"]),
        ("User-Agent seen", &seen["user_agent"]),
    ];
    let mut known = false;
    for (name, value) in fields {
        if let Some(value) = text(value) {
            known = true;
            println!("{name}: {value}");
        }
    }
    // The headers as sent: an HTTP/2 HEADERS frame's, or HTTP/1.1's.
    let frames = seen["http2"]["sent_frames"]
        .as_array()
        .into_iter()
        .flatten();
    let headers = frames
        .filter(|frame| frame["frame_type"] == "HEADERS")
        .filter_map(|frame| frame["headers"].as_array())
        .next()
        .or_else(|| seen["http1"]["headers"].as_array());
    if let Some(headers) = headers {
        known = true;
        println!("Headers, in order:");
        for header in headers {
            println!("  {}", text(header).unwrap_or_else(|| header.to_string()));
        }
    }
    if !known {
        println!("{}", serde_json::to_string_pretty(seen)?);
    }
    Ok(())
}
