#![allow(dead_code)]

//! Minimal HTTP/1.1 client.
//!
//! - Uses DNS (A queries) and a minimal TCP client.
//! - Supports basic redirects (Location) and chunked transfer decoding.
//!
//! HTTPS:
//! - Native TLS is not implemented in-kernel yet.
//! - For now, `https://` URLs are fetched via the optional host-side HTTPS proxy
//!   at 10.0.2.2:8080 (QEMU user networking default). See tools/https_proxy.py.

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use super::{dns, tcp};

fn parse_ipv4(host: &str) -> Option<[u8; 4]> {
    let mut out = [0u8; 4];
    let mut i = 0usize;
    for part in host.split('.') {
        if i >= 4 { return None; }
        let n: u16 = part.parse().ok()?;
        if n > 255 { return None; }
        out[i] = n as u8;
        i += 1;
    }
    if i == 4 { Some(out) } else { None }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HttpError {
    Dns,
    Tcp(super::tcp::TcpError),
    Parse,
    RedirectLoop,
    UnsupportedScheme,
}

#[derive(Clone, Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub content_type: Option<String>,
    pub location: Option<String>,
    pub body: Vec<u8>,
}

const HTTPS_PROXY_IP: [u8; 4] = [10, 0, 2, 2];
const HTTPS_PROXY_PORT: u16 = 8080;

#[derive(Clone, Debug)]
struct UrlParts {
    scheme: String,
    host: String,
    port: u16,
    path: String,
    original: String,
}

fn parse_url(url: &str) -> Result<UrlParts, HttpError> {
    let original = url.to_string();
    let mut rest = url;

    let mut scheme = "http".to_string();
    if let Some(i) = url.find("://") {
        scheme = url[..i].to_string();
        rest = &url[i + 3..];
    }

    let mut host_port = rest;
    let mut path = "/";
    if let Some(slash) = rest.find('/') {
        host_port = &rest[..slash];
        path = &rest[slash..];
    }

    let mut host = host_port;
    let mut port: u16 = match scheme.as_str() {
        "http" => 80,
        "https" => 443,
        _ => return Err(HttpError::UnsupportedScheme),
    };

    if let Some(col) = host_port.rfind(':') {
        if let Ok(p) = host_port[col + 1..].parse::<u16>() {
            host = &host_port[..col];
            port = p;
        }
    }
    if host.is_empty() { return Err(HttpError::Parse); }

    Ok(UrlParts {
        scheme,
        host: host.to_string(),
        port,
        path: path.to_string(),
        original,
    })
}

fn find_header<'a>(headers: &'a str, name: &str) -> Option<&'a str> {
    let needle = name.to_ascii_lowercase();
    for line in headers.split("\r\n") {
        if let Some(col) = line.find(':') {
            let (k, v) = line.split_at(col);
            if k.trim().to_ascii_lowercase() == needle {
                return Some(v[1..].trim());
            }
        }
    }
    None
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => {
                out.push('%');
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                out.push(HEX[(b >> 4) as usize] as char);
                out.push(HEX[(b & 0xF) as usize] as char);
            }
        }
    }
    out
}

fn decode_chunked(body: &[u8]) -> Result<Vec<u8>, HttpError> {
    let mut out = Vec::new();
    let mut i = 0usize;

    while i < body.len() {
        let mut j = i;
        while j + 1 < body.len() && !(body[j] == b'\r' && body[j + 1] == b'\n') {
            j += 1;
        }
        if j + 1 >= body.len() { return Err(HttpError::Parse); }

        let line = &body[i..j];
        let line_str = core::str::from_utf8(line).map_err(|_| HttpError::Parse)?;
        let size_str = line_str.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_str, 16).map_err(|_| HttpError::Parse)?;

        i = j + 2;
        if size == 0 { break; }
        if i + size > body.len() { return Err(HttpError::Parse); }

        out.extend_from_slice(&body[i..i + size]);
        i += size;

        if i + 1 >= body.len() { return Err(HttpError::Parse); }
        if body[i] != b'\r' || body[i + 1] != b'\n' { return Err(HttpError::Parse); }
        i += 2;
    }

    Ok(out)
}

fn parse_http_response(raw: &[u8]) -> Result<HttpResponse, HttpError> {
    // Prefer CRLF delimiter per RFC, but tolerate LF-only servers (or buggy stacks).
    let mut hdr_end: Option<(usize, usize)> = None; // (index, delimiter_len)
    for i in 0..raw.len().saturating_sub(3) {
        if raw[i] == b'\r' && raw[i + 1] == b'\n' && raw[i + 2] == b'\r' && raw[i + 3] == b'\n' {
            hdr_end = Some((i, 4));
            break;
        }
    }
    if hdr_end.is_none() {
        for i in 0..raw.len().saturating_sub(1) {
            if raw[i] == b'\n' && raw[i + 1] == b'\n' {
                hdr_end = Some((i, 2));
                break;
            }
        }
    }

    let Some((hdr_end, delim_len)) = hdr_end else { return Err(HttpError::Parse); };
    let hdrs = &raw[..hdr_end];
    let body = &raw[hdr_end + delim_len..];

    let hdr_str = core::str::from_utf8(hdrs).map_err(|_| HttpError::Parse)?;

    // Split lines on either CRLF or LF; trim trailing CR to be safe.
    let mut lines_iter = hdr_str
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l));

    let status_line = lines_iter.next().ok_or(HttpError::Parse)?;
    let mut status_parts = status_line.split_whitespace();
    let _http = status_parts.next().ok_or(HttpError::Parse)?;
    let status = status_parts.next().ok_or(HttpError::Parse)?.parse::<u16>().map_err(|_| HttpError::Parse)?;

    let content_type = find_header(hdr_str, "content-type").map(|s| s.to_string());
    let location = find_header(hdr_str, "location").map(|s| s.to_string());
    let transfer = find_header(hdr_str, "transfer-encoding").unwrap_or("");
    let cl = find_header(hdr_str, "content-length").and_then(|s| s.parse::<usize>().ok());

    let mut out_body = if transfer.to_ascii_lowercase().contains("chunked") {
        decode_chunked(body)?
    } else if let Some(n) = cl {
        body.get(..n).unwrap_or(body).to_vec()
    } else {
        body.to_vec()
    };

    if out_body.len() > 2_000_000 {
        out_body.truncate(2_000_000);
    }

    Ok(HttpResponse {
        status,
        content_type,
        location,
        body: out_body,
    })
}

fn http_get_direct(parts: &UrlParts, max_bytes: usize) -> Result<HttpResponse, HttpError> {
    // If the host is already an IPv4 literal (e.g. 10.0.2.2), skip DNS.
    let ip = if let Some(ip) = parse_ipv4(&parts.host) {
        ip
    } else {
        dns::resolve_a(&parts.host).map_err(|_| HttpError::Dns)?
    };
    let mut s = tcp::TcpStream::connect(ip, parts.port, 10_000_000).map_err(HttpError::Tcp)?;

    let req = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: OthelloBrowser/0.1\r\nAccept: text/html, text/plain, */*\r\nConnection: close\r\n\r\n",
        parts.path,
        parts.host
    );
    s.write_all(req.as_bytes()).map_err(HttpError::Tcp)?;
    let raw = s.read_to_end(max_bytes, 10_000_000).map_err(HttpError::Tcp)?;
    let _ = s.close();

    parse_http_response(&raw)
}

fn http_get_via_https_proxy(url: &str, max_bytes: usize) -> Result<HttpResponse, HttpError> {
    let ip = HTTPS_PROXY_IP;
    let mut s = tcp::TcpStream::connect(ip, HTTPS_PROXY_PORT, 10_000_000).map_err(HttpError::Tcp)?;

    let q = url_encode(url);
    let path = format!("/fetch?url={}", q);

    let host = format!("{}.{}.{}.{}", ip[0], ip[1], ip[2], ip[3]);
    let req = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: OthelloBrowser/0.1\r\nAccept: */*\r\nConnection: close\r\n\r\n",
        path,
        host
    );

    s.write_all(req.as_bytes()).map_err(HttpError::Tcp)?;
    let raw = s.read_to_end(max_bytes, 10_000_000).map_err(HttpError::Tcp)?;
    let _ = s.close();

    parse_http_response(&raw)
}

pub fn get(url: &str, max_bytes: usize) -> Result<HttpResponse, HttpError> {
    let mut cur = parse_url(url)?;
    let mut redirects = 0usize;

    loop {
        let resp = match cur.scheme.as_str() {
            "http" => http_get_direct(&cur, max_bytes),
            "https" => http_get_via_https_proxy(&cur.original, max_bytes),
            _ => Err(HttpError::UnsupportedScheme),
        }?;

        if matches!(resp.status, 301 | 302 | 303 | 307 | 308) {
            if redirects >= 5 { return Err(HttpError::RedirectLoop); }
            if let Some(loc) = resp.location.clone() {
                redirects += 1;
                let next = if loc.starts_with("http://") || loc.starts_with("https://") {
                    loc
                } else if loc.starts_with('/') {
                    format!("{}://{}{}", cur.scheme, cur.host, loc)
                } else {
                    // relative
                    let base = cur.path.rsplitn(2, '/').nth(1).unwrap_or("");
                    format!("{}://{}/{}/{}", cur.scheme, cur.host, base, loc)
                };
                cur = parse_url(&next)?;
                continue;
            }
        }

        return Ok(resp);
    }
}
