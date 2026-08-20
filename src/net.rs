//! A small HTTPS client, for the one thing Zervo fetches on its own account.
//!
//! Every other request in this browser is the engine's: Servo fetches for a
//! page, with that page's cookies and its own cache. A wallpaper has no page
//! behind it, so it cannot go through any of that. Rather than take on a
//! general-purpose client crate for one GET, this is HTTP/1.1 written against
//! the rustls Zervo already links. It knows GET, it follows redirects, it
//! reads a counted or a chunked body, and it gives up loudly on the rest.
//!
//! It is deliberately not a browser: no cookies, no cache, no compression, no
//! connection reuse, and a hard ceiling on how many bytes a host can hand back.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs as _};
use std::sync::Arc;
use std::time::Duration;

/// How Zervo introduces itself to a wallpaper host. Wikimedia asks that
/// automated clients say who they are and where to complain, and refuses
/// anonymous ones; the others do not mind being told.
const USER_AGENT: &str = concat!(
    "Zervo/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/goddv/zervo) rustls"
);

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// Redirects to follow before deciding a host is playing games.
const MAX_HOPS: usize = 5;
/// Headers alone, before the body starts. Anything past this is not a header
/// block, it is an attack.
const MAX_HEADERS: usize = 64 * 1024;

pub struct Response {
    /// Where the bytes finally came from, after any redirects.
    pub url: String,
    pub content_type: String,
    pub body: Vec<u8>,
}

/// Fetch `url`, following redirects, reading at most `limit` bytes of body.
///
/// `accept` is sent as the `Accept` header: the JSON endpoints and the image
/// hosts want different answers, and asking for the wrong one gets an HTML
/// error page that decodes as neither.
pub fn get(url: &str, accept: &str, limit: usize) -> Result<Response, String> {
    let mut target = url::Url::parse(url).map_err(|error| format!("{url}: {error}"))?;
    for _ in 0..MAX_HOPS {
        let response = fetch_once(&target, accept, limit)?;
        match response {
            Hop::Done(mut done) => {
                done.url = target.to_string();
                return Ok(done);
            },
            Hop::Redirect(location) => {
                // Relative redirects are legal and common: resolve against the
                // URL we asked for, not against the original.
                target = target
                    .join(&location)
                    .map_err(|error| format!("bad redirect to {location}: {error}"))?;
                if target.scheme() != "https" {
                    return Err(format!("refusing a redirect to {}", target.scheme()));
                }
            },
        }
    }
    Err(format!("{url}: too many redirects"))
}

enum Hop {
    Done(Response),
    Redirect(String),
}

fn fetch_once(url: &url::Url, accept: &str, limit: usize) -> Result<Hop, String> {
    if url.scheme() != "https" {
        return Err(format!("{url}: only https is fetched"));
    }
    let host = url
        .host_str()
        .ok_or_else(|| format!("{url}: no host"))?
        .to_owned();
    let port = url.port_or_known_default().unwrap_or(443);

    // Resolution can block for a while on a bad network, so it happens under
    // the same deadline as everything else.
    let addresses: Vec<_> = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|error| format!("{host}: {error}"))?
        .collect();
    if addresses.is_empty() {
        return Err(format!("{host}: no address"));
    }
    // Every address, not just the first. A host whose first record is an IPv6
    // one, on a machine with no route to it, is otherwise unreachable — and
    // that is a common enough shape to be worth four lines.
    let mut refused = format!("{host}: unreachable");
    let mut connected = None;
    for address in addresses {
        match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
            Ok(socket) => {
                connected = Some(socket);
                break;
            },
            Err(error) => refused = format!("{host}: {error}"),
        }
    }
    let Some(socket) = connected else {
        return Err(refused);
    };
    socket
        .set_read_timeout(Some(READ_TIMEOUT))
        .and_then(|()| socket.set_write_timeout(Some(READ_TIMEOUT)))
        .map_err(|error| format!("{host}: {error}"))?;

    // The process-wide crypto provider is installed at startup for the engine;
    // this rides on it rather than choosing a second one.
    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let server_name = rustls::pki_types::ServerName::try_from(host.clone())
        .map_err(|error| format!("{host}: {error}"))?;
    let connection = rustls::ClientConnection::new(Arc::new(config), server_name)
        .map_err(|error| format!("{host}: {error}"))?;
    let mut tls = rustls::StreamOwned::new(connection, socket);

    let path = match url.query() {
        Some(query) => format!("{}?{}", url.path(), query),
        None => url.path().to_owned(),
    };
    // `identity` because nothing here can decompress, and `close` because
    // there is no second request to keep the connection for.
    let request = format!(
        "GET {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         User-Agent: {USER_AGENT}\r\n\
         Accept: {accept}\r\n\
         Accept-Encoding: identity\r\n\
         Connection: close\r\n\r\n"
    );
    tls.write_all(request.as_bytes())
        .and_then(|()| tls.flush())
        .map_err(|error| format!("{host}: {error}"))?;

    let raw =
        read_capped(&mut tls, limit + MAX_HEADERS).map_err(|error| format!("{host}: {error}"))?;
    parse(&raw, limit)
}

/// Read until the peer hangs up, or until `limit` bytes have arrived.
///
/// A `Content-Length` cannot be trusted to bound this: it is the sender's
/// claim, and the sender is the party being defended against.
fn read_capped(source: &mut impl Read, limit: usize) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        match source.read(&mut buffer) {
            Ok(0) => return Ok(out),
            Ok(read) => {
                out.extend_from_slice(&buffer[..read]);
                if out.len() > limit {
                    return Err(std::io::Error::other("response too large"));
                }
            },
            // A clean close arrives as a TLS "close notify" or, from hosts
            // that do not send one, as an abrupt end. Both mean the body is
            // whatever has already been read.
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(out),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
}

fn parse(raw: &[u8], limit: usize) -> Result<Hop, String> {
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| "no header block".to_owned())?;
    if split > MAX_HEADERS {
        return Err("header block too large".to_owned());
    }
    let head = String::from_utf8_lossy(&raw[..split]);
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or_default();
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .ok_or_else(|| format!("unreadable status line: {status_line}"))?;

    let mut location = None;
    let mut content_type = String::new();
    let mut chunked = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "location" => location = Some(value.to_owned()),
            "content-type" => content_type = value.to_owned(),
            "transfer-encoding" => chunked = value.to_ascii_lowercase().contains("chunked"),
            _ => {},
        }
    }

    if (300..400).contains(&status) {
        return match location {
            Some(location) => Ok(Hop::Redirect(location)),
            None => Err(format!("{status} with nowhere to go")),
        };
    }
    if !(200..300).contains(&status) {
        return Err(format!("{status} {}", status_line.trim()));
    }

    let body = &raw[split + 4..];
    let body = if chunked {
        dechunk(body, limit)?
    } else {
        body.to_vec()
    };
    if body.len() > limit {
        return Err("response too large".to_owned());
    }
    Ok(Hop::Done(Response {
        url: String::new(),
        content_type,
        body,
    }))
}

/// Reassemble a chunked body. Sizes are hex, each chunk is followed by CRLF,
/// and a zero-length chunk ends it.
fn dechunk(mut body: &[u8], limit: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let line_end = body
            .windows(2)
            .position(|window| window == b"\r\n")
            .ok_or_else(|| "truncated chunk header".to_owned())?;
        let header = String::from_utf8_lossy(&body[..line_end]);
        // A chunk size may carry extensions after a semicolon; ignore them.
        let size = usize::from_str_radix(header.split(';').next().unwrap_or("").trim(), 16)
            .map_err(|_| format!("unreadable chunk size: {header}"))?;
        body = &body[line_end + 2..];
        if size == 0 {
            return Ok(out);
        }
        if size > body.len() {
            return Err("truncated chunk".to_owned());
        }
        out.extend_from_slice(&body[..size]);
        if out.len() > limit {
            return Err("response too large".to_owned());
        }
        // The chunk's own trailing CRLF.
        body = &body[(size + 2).min(body.len())..];
    }
}
