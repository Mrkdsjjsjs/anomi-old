//! Opening WebSockets: Discord's gateway, voice and Go Live servers, its
//! login service, and the Spotify socket its web client follows. Each opens
//! as the browser's WebSocket does: an HTTP/1.1 upgrade through the same
//! client, and so the same TLS and certificates, as HTTPS requests, with the
//! browser's headers in its order and discord.com as its origin
//! ([`yapfast_http::Kind::WebSocket`]). The frames then go through
//! tokio-tungstenite over the upgraded connection, with the tungstenite of
//! vendor/tungstenite, which gives back the read buffer a large frame grew.
//!
//! Firefox also offers `Sec-WebSocket-Extensions: permessage-deflate`, which
//! these don't: tungstenite reads no compressed frames, and a server that
//! took the offer would send them.

use std::{
    pin::Pin,
    task::{Context, Poll},
};

use sync_wrapper::SyncWrapper;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_tungstenite::{
    WebSocketStream,
    tungstenite::{
        handshake::{client::generate_key, derive_accept_key},
        protocol::Role,
    },
};
use yapfast_http::wreq::{
    self, Client, Method, StatusCode,
    header::{CONNECTION, HeaderName, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, UPGRADE},
};

use crate::browser;

/// An open WebSocket.
pub(crate) type Socket = WebSocketStream<Transport>;

/// The connection under a WebSocket, once upgraded. wreq's is Send but not
/// Sync; this is both, as the TLS stream under the sockets was before, by
/// only ever lending it out mutably.
pub(crate) struct Transport(SyncWrapper<wreq::Upgraded>);

impl AsyncRead for Transport {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(self.get_mut().0.get_mut()).poll_read(cx, buf)
    }
}

impl AsyncWrite for Transport {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(self.get_mut().0.get_mut()).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(self.get_mut().0.get_mut()).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(self.get_mut().0.get_mut()).poll_shutdown(cx)
    }
}

/// Why a WebSocket didn't open. Neither says the address, which may hold a
/// token, as Spotify's does.
#[derive(Debug)]
pub(crate) enum Error {
    /// The server answered the upgrade with this status rather than
    /// switching protocols.
    Refused(StatusCode),
    /// It couldn't be reached, its TLS failed, or it answered no WebSocket.
    Failed(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Refused(status) => write!(f, "the server answered {status}"),
            Error::Failed(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for Error {}

impl From<wreq::Error> for Error {
    fn from(error: wreq::Error) -> Self {
        Error::Failed(error.without_uri().to_string())
    }
}

/// Opens a WebSocket to Discord, or the test server, at a `wss://` or
/// `ws://` address, through the client with Discord's cookies.
pub(crate) async fn connect(url: &str) -> Result<Socket, Error> {
    let client = browser::discord().map_err(|error| Error::Failed(error.to_string()))?;
    connect_with(client, url).await
}

/// Opens a WebSocket to `url` through `client`.
pub(crate) async fn connect_with(client: &Client, url: &str) -> Result<Socket, Error> {
    // The upgrade is an HTTP request, to the address's HTTP form.
    let address = if let Some(rest) = url.strip_prefix("wss://") {
        format!("https://{rest}")
    } else if let Some(rest) = url.strip_prefix("ws://") {
        format!("http://{rest}")
    } else {
        return Err(Error::Failed("not a WebSocket address".to_owned()));
    };
    let key = generate_key();
    let response = yapfast_http::request(client, browser::SOCKET, Method::GET, &address)
        .header(SEC_WEBSOCKET_KEY, key.as_str())
        .send()
        .await?;
    let status = response.status();
    if status != StatusCode::SWITCHING_PROTOCOLS {
        return Err(Error::Refused(status));
    }
    let headers = response.headers();
    let says = |name: HeaderName, value: &str| {
        headers
            .get(name)
            .and_then(|header| header.to_str().ok())
            .is_some_and(|header| {
                header
                    .split(',')
                    .any(|token| token.trim().eq_ignore_ascii_case(value))
            })
    };
    let accepted = headers
        .get(SEC_WEBSOCKET_ACCEPT)
        .is_some_and(|accept| accept.as_bytes() == derive_accept_key(key.as_bytes()).as_bytes());
    if !says(UPGRADE, "websocket") || !says(CONNECTION, "upgrade") || !accepted {
        return Err(Error::Failed(
            "the server answered the upgrade with no WebSocket".to_owned(),
        ));
    }
    let upgraded = response.upgrade().await?;
    let transport = Transport(SyncWrapper::new(upgraded));
    Ok(WebSocketStream::from_raw_socket(transport, Role::Client, None).await)
}

#[cfg(test)]
mod tests {
    use futures_util::{SinkExt, StreamExt};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };
    use tokio_tungstenite::tungstenite::Message;

    use super::*;

    /// A WebSocket opens as Firefox's: its upgrade's headers go in Firefox's
    /// order and spelling, and once the server switches, frames go both ways
    /// over the upgraded connection, including what came with the 101.
    #[tokio::test]
    async fn a_websocket_opens_as_firefoxs_and_carries_frames() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut read = Vec::new();
            let head = loop {
                let mut chunk = [0; 4096];
                let got = stream.read(&mut chunk).await.unwrap();
                assert_ne!(got, 0, "the upgrade ended early");
                read.extend_from_slice(&chunk[..got]);
                let text = String::from_utf8_lossy(&read).into_owned();
                if let Some((head, _)) = text.split_once("\r\n\r\n") {
                    break head.to_owned();
                }
            };
            let key = head
                .lines()
                .find_map(|line| line.strip_prefix("Sec-WebSocket-Key: "))
                .unwrap()
                .to_owned();
            let answer = format!(
                "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
                derive_accept_key(key.as_bytes())
            );
            stream.write_all(answer.as_bytes()).await.unwrap();
            let mut socket = WebSocketStream::from_raw_socket(stream, Role::Server, None).await;
            socket.send(Message::text("hello")).await.unwrap();
            let echoed = socket.next().await.unwrap().unwrap();
            (head, echoed)
        });
        let client = yapfast_http::builder().build().unwrap();
        let mut socket = connect_with(&client, &format!("ws://{address}/?v=9"))
            .await
            .unwrap();
        let hello = socket.next().await.unwrap().unwrap();
        socket.send(hello).await.unwrap();
        let (head, echoed) = server.await.unwrap();
        assert_eq!(echoed, Message::text("hello"));
        assert!(head.starts_with("GET /?v=9 HTTP/1.1\r\n"), "{head}");
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
                "Sec-WebSocket-Version",
                "Origin",
                "Sec-WebSocket-Key",
                "Connection",
                "Sec-Fetch-Dest",
                "Sec-Fetch-Mode",
                "Sec-Fetch-Site",
                "Pragma",
                "Cache-Control",
                "Upgrade",
            ],
            "{head}"
        );
        assert!(
            head.contains("\r\nOrigin: https://discord.com\r\n"),
            "{head}"
        );
        assert!(
            head.contains("\r\nConnection: keep-alive, Upgrade\r\n"),
            "{head}"
        );
    }
}
