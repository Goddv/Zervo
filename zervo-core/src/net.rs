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
use std::net::{IpAddr, TcpStream, ToSocketAddrs as _};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

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
/// The whole fetch, redirects included.
///
/// `READ_TIMEOUT` bounds a single `read`, which is not the same thing at all: a
/// host sending one byte every twenty-nine seconds satisfies it indefinitely,
/// and the transfer then ends only when the byte ceiling does. Because the
/// fetch runs on a detached thread whose `loading` flag is cleared only when it
/// returns, that did not merely waste a thread — it stopped the wallpaper ever
/// refreshing again for the rest of the session.
const TOTAL_TIMEOUT: Duration = Duration::from_secs(60);
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
    // One deadline for the whole thing, not one per hop: five redirects each
    // taking their own sweet time is the same wait as one that never ends.
    let deadline = Instant::now() + TOTAL_TIMEOUT;
    for _ in 0..MAX_HOPS {
        if Instant::now() >= deadline {
            return Err(format!("{url}: took too long"));
        }
        let response = fetch_once(&target, accept, limit, deadline)?;
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

/// The TLS setup, built once.
///
/// This used to be rebuilt per request *and* per redirect hop, which meant
/// cloning the whole webpki root store — a few hundred certificates — to fetch
/// one picture.
fn tls_config() -> Arc<rustls::ClientConfig> {
    static CONFIG: OnceLock<Arc<rustls::ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let roots = rustls::RootCertStore {
                roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
            };
            // The process-wide crypto provider is installed at startup for the
            // engine; this rides on it rather than choosing a second one.
            Arc::new(
                rustls::ClientConfig::builder()
                    .with_root_certificates(roots)
                    .with_no_client_auth(),
            )
        })
        .clone()
}

/// Whether an address is out on the internet, rather than on this machine or
/// this network.
///
/// Wallpapers come from public APIs, but Openverse indexes third-party
/// providers, so the image URL inside a result is not first-party data — and
/// after a redirect the host is not even the one that was asked. Names that
/// resolve to private addresses and still hold a valid public certificate are
/// ordinary and easy to get (`localtest.me`, `*.nip.io`). Without this check a
/// stranger's search result can make Zervo knock on whatever is listening
/// inside the network and report back, through Settings, what answered.
fn is_public(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.is_unspecified()
                // 100.64.0.0/10, carrier-grade NAT.
                || (a == 100 && (64..128).contains(&b))
                // 192.0.0.0/24, IETF protocol assignments.
                || v4.octets()[..3] == [192, 0, 0]
                // 240.0.0.0/4, reserved — including 255.255.255.255.
                || a >= 240)
        },
        IpAddr::V6(v6) => {
            // An IPv4 address in a v6 coat would otherwise walk past every one
            // of the rules above.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public(IpAddr::V4(v4));
            }
            if v6.is_loopback() || v6.is_unspecified() || v6.is_multicast() {
                return false;
            }
            // fc00::/7 unique-local and fe80::/10 link-local. std knows both,
            // but only behind unstable methods, so they are spelled out.
            let first = v6.segments()[0];
            first & 0xfe00 != 0xfc00 && first & 0xffc0 != 0xfe80
        },
    }
}

fn fetch_once(
    url: &url::Url,
    accept: &str,
    limit: usize,
    deadline: Instant,
) -> Result<Hop, String> {
    if url.scheme() != "https" {
        return Err(format!("{url}: only https is fetched"));
    }
    let host = url
        .host_str()
        .ok_or_else(|| format!("{url}: no host"))?
        .to_owned();
    let port = url.port_or_known_default().unwrap_or(443);

    // `to_socket_addrs` is a blocking `getaddrinfo` with no deadline of its own
    // — there is no way to give it one without a thread — so on a network that
    // is down rather than slow this call is the one part of a fetch that can
    // outlast `TOTAL_TIMEOUT`. The resolver's own timeout is the only bound.
    let addresses: Vec<_> = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|error| format!("{host}: {error}"))?
        .collect();
    if addresses.is_empty() {
        return Err(format!("{host}: no address"));
    }
    // Checked after resolution rather than on the URL, because the URL is a
    // name and the name is not what decides where the connection goes.
    let addresses: Vec<_> = addresses
        .into_iter()
        .filter(|address| is_public(address.ip()))
        .collect();
    if addresses.is_empty() {
        return Err(format!("{host}: refusing to fetch from a private address"));
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
    // Never longer than what is left of the overall deadline, so a stalled
    // read cannot overshoot it by a whole `READ_TIMEOUT`.
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .unwrap_or_default()
        .min(READ_TIMEOUT)
        .max(Duration::from_millis(1));
    socket
        .set_read_timeout(Some(remaining))
        .and_then(|()| socket.set_write_timeout(Some(remaining)))
        .map_err(|error| format!("{host}: {error}"))?;

    let server_name = rustls::pki_types::ServerName::try_from(host.clone())
        .map_err(|error| format!("{host}: {error}"))?;
    let connection = rustls::ClientConnection::new(tls_config(), server_name)
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

    let raw = read_capped(&mut tls, limit + MAX_HEADERS, deadline)
        .map_err(|error| format!("{host}: {error}"))?;
    parse(&raw, limit)
}

/// Read until the peer hangs up, or until `limit` bytes have arrived.
///
/// A `Content-Length` cannot be trusted to bound this: it is the sender's
/// claim, and the sender is the party being defended against.
fn read_capped(
    source: &mut impl Read,
    limit: usize,
    deadline: Instant,
) -> std::io::Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        // A sender that keeps each individual read inside `READ_TIMEOUT` can
        // otherwise hold this loop open until the byte ceiling is reached, one
        // byte at a time.
        if Instant::now() >= deadline {
            return Err(std::io::Error::other("took too long"));
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn done(raw: &[u8], limit: usize) -> Response {
        match parse(raw, limit) {
            Ok(Hop::Done(response)) => response,
            other => panic!("expected a body, got {:?}", other.map(|_| "a redirect")),
        }
    }

    #[test]
    fn a_plain_response_comes_apart_into_headers_and_body() {
        let response = done(
            b"HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: 4\r\n\r\nbody",
            100,
        );
        assert_eq!(response.content_type, "image/jpeg");
        assert_eq!(response.body, b"body");
    }

    /// Header names are case-insensitive on the wire, and a server that sends
    /// `content-type` is not sending something else.
    #[test]
    fn header_names_are_matched_without_regard_to_case() {
        let response = done(
            b"HTTP/1.1 200 OK\r\nCoNtEnT-TyPe:  image/png \r\n\r\nx",
            100,
        );
        assert_eq!(response.content_type, "image/png");
    }

    #[test]
    fn a_redirect_is_reported_rather_than_followed_here() {
        match parse(b"HTTP/1.1 302 Found\r\nLocation: /elsewhere\r\n\r\n", 100) {
            Ok(Hop::Redirect(to)) => assert_eq!(to, "/elsewhere"),
            other => panic!("expected a redirect, got {:?}", other.is_ok()),
        }
        // A redirect with nowhere to go is not a redirect.
        assert!(parse(b"HTTP/1.1 302 Found\r\n\r\n", 100).is_err());
    }

    #[test]
    fn anything_that_is_not_a_success_is_an_error() {
        assert!(parse(b"HTTP/1.1 404 Not Found\r\n\r\nnope", 100).is_err());
        assert!(parse(b"HTTP/1.1 500 Oops\r\n\r\n", 100).is_err());
        // No header block at all.
        assert!(parse(b"garbage", 100).is_err());
        assert!(parse(b"", 100).is_err());
        // A status line with no status in it.
        assert!(parse(b"HTTP/1.1\r\n\r\n", 100).is_err());
    }

    #[test]
    fn a_chunked_body_is_reassembled() {
        let raw = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\
                    4\r\nWiki\r\n5\r\npedia\r\n0\r\n\r\n";
        assert_eq!(done(raw, 100).body, b"Wikipedia");
    }

    /// A chunk size may carry extensions after a semicolon, and a server is
    /// entitled to send them.
    #[test]
    fn a_chunk_extension_is_ignored_rather_than_choked_on() {
        assert_eq!(
            dechunk(b"3;name=value\r\nabc\r\n0\r\n\r\n", 100).unwrap(),
            b"abc"
        );
    }

    /// Everything here is server-controlled, so every malformed shape has to
    /// come back as an error rather than a panic or a hang.
    #[test]
    fn a_malformed_chunked_body_is_refused_and_does_not_panic() {
        // A size that is not hexadecimal.
        assert!(dechunk(b"zz\r\nabc\r\n0\r\n\r\n", 100).is_err());
        // A size larger than what follows it.
        assert!(dechunk(b"ff\r\nabc\r\n", 100).is_err());
        // No terminator at all.
        assert!(dechunk(b"3\r\nabc\r\n", 100).is_err());
        // A header line that never ends.
        assert!(dechunk(b"3", 100).is_err());
        assert!(dechunk(b"", 100).is_err());
        // A size that would overflow a usize parses as an error, not a wrap.
        assert!(dechunk(b"ffffffffffffffffffff\r\nx\r\n", 100).is_err());
    }

    /// The ceiling is the only thing standing between a hostile host and this
    /// process's memory, so it has to hold part-way through a body as well as
    /// at the end of one.
    #[test]
    fn the_byte_ceiling_holds_mid_body() {
        assert!(dechunk(b"a\r\n0123456789\r\n0\r\n\r\n", 4).is_err());
        let long = format!("HTTP/1.1 200 OK\r\n\r\n{}", "x".repeat(50));
        assert!(parse(long.as_bytes(), 10).is_err());
    }

    /// `Content-Length` is the sender's claim about the sender, and the sender
    /// is the party being defended against — so it must not be what bounds the
    /// read.
    #[test]
    fn a_lying_content_length_changes_nothing() {
        let response = done(
            b"HTTP/1.1 200 OK\r\nContent-Length: 99999999\r\n\r\nshort",
            100,
        );
        assert_eq!(response.body, b"short");
    }

    #[test]
    fn reading_stops_at_the_ceiling() {
        let mut source = std::io::Cursor::new(vec![b'x'; 64 * 1024]);
        let deadline = Instant::now() + Duration::from_secs(5);
        assert!(read_capped(&mut source, 1024, deadline).is_err());

        let mut small = std::io::Cursor::new(b"hello".to_vec());
        assert_eq!(
            read_capped(&mut small, 1024, deadline).unwrap(),
            b"hello".to_vec()
        );
    }

    /// The check that keeps a stranger's search result from pointing this
    /// browser at something inside the network it is running on.
    #[test]
    fn only_addresses_out_on_the_internet_are_fetched_from() {
        let public = ["1.1.1.1", "93.184.216.34", "2606:4700:4700::1111"];
        for address in public {
            assert!(
                is_public(address.parse().unwrap()),
                "{address} should be reachable"
            );
        }
        let private = [
            "127.0.0.1",        // loopback
            "0.0.0.0",          // unspecified
            "10.0.0.1",         // private
            "192.168.1.1",      // private
            "172.16.0.1",       // private
            "169.254.169.254",  // link-local, and the cloud metadata service
            "100.64.0.1",       // carrier-grade NAT
            "192.0.0.1",        // IETF protocol assignments
            "255.255.255.255",  // broadcast
            "240.0.0.1",        // reserved
            "::1",              // v6 loopback
            "fc00::1",          // v6 unique-local
            "fe80::1",          // v6 link-local
            "::ffff:127.0.0.1", // v4 loopback wearing a v6 coat
            "::ffff:10.0.0.1",
        ];
        for address in private {
            assert!(
                !is_public(address.parse().unwrap()),
                "{address} should be refused"
            );
        }
    }
}
