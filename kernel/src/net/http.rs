//! A small HTTP/1.0 client (plain `http://` only).

use super::{NetError, Received};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Debug)]
pub struct Url {
    pub host: String,
    pub port: u16,
    pub path: String,
}

impl Url {
    pub fn parse(text: &str) -> Result<Url, String> {
        let text = text.trim();
        let rest = if let Some(r) = text.strip_prefix("http://") {
            r
        } else if text.starts_with("https://") {
            return Err(String::from("https:// pages are encrypted, and zero cannot do that yet: try an http:// page"));
        } else if text.contains("://") {
            return Err(String::from("only http:// addresses work"));
        } else {
            text
        };
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        let (host, port) = match authority.rsplit_once(':') {
            Some((h, p)) => (h, p.parse::<u16>().map_err(|_| String::from("bad port number"))?),
            None => (authority, 80),
        };
        if host.is_empty() {
            return Err(String::from("empty address"));
        }
        // fragments are not sent to the server
        let path = path.split('#').next().unwrap_or("/");
        Ok(Url { host: String::from(host), port, path: String::from(if path.is_empty() { "/" } else { path }) })
    }

    pub fn to_string(&self) -> String {
        if self.port == 80 {
            format!("http://{}{}", self.host, self.path)
        } else {
            format!("http://{}:{}{}", self.host, self.port, self.path)
        }
    }

    /// Resolves a link found on this page.
    pub fn join(&self, link: &str) -> String {
        let link = link.trim();
        if link.starts_with("http://") || link.starts_with("https://") {
            return String::from(link);
        }
        if let Some(rest) = link.strip_prefix("//") {
            return format!("http://{}", rest);
        }
        let base = if self.port == 80 { format!("http://{}", self.host) } else { format!("http://{}:{}", self.host, self.port) };
        if link.starts_with('/') {
            return format!("{}{}", base, link);
        }
        if link.starts_with('#') {
            return format!("{}{}", self.to_string().split('#').next().unwrap_or(""), link);
        }
        let dir = match self.path.rfind('/') {
            Some(i) => &self.path[..=i],
            None => "/",
        };
        // collapse ./ and ../
        let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
        let mut tail = link;
        loop {
            if let Some(t) = tail.strip_prefix("./") {
                tail = t;
            } else if let Some(t) = tail.strip_prefix("../") {
                parts.pop();
                tail = t;
            } else {
                break;
            }
        }
        let mut path = String::from("/");
        for p in parts {
            path.push_str(p);
            path.push('/');
        }
        path.push_str(tail);
        format!("{}{}", base, path)
    }
}

pub struct Response {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
    pub url: Url,
}

/// Fetches a page, following redirects. `progress` is told how many bytes
/// have arrived and may return false to cancel.
pub fn get(url: &str, progress: &mut dyn FnMut(&str, usize) -> bool) -> Result<Response, String> {
    let mut url = Url::parse(url)?;
    for _hop in 0..6 {
        progress("looking up the address", 0);
        let ip = super::resolve(&url.host).map_err(|e| format!("{}", e))?;
        progress("connecting", 0);
        let sock = super::tcp_connect(ip, url.port, 8000).map_err(|e| format!("{}", e))?;
        let request = format!(
            "GET {} HTTP/1.0\r\nHost: {}\r\nUser-Agent: zero-browser/0.1\r\nAccept: text/html, text/plain\r\nConnection: close\r\n\r\n",
            url.path, url.host
        );
        super::tcp_send(sock, request.as_bytes()).map_err(|e| format!("{}", e))?;
        let mut data: Vec<u8> = Vec::new();
        let mut idle = 0;
        loop {
            match super::tcp_recv(sock, &mut data, 500) {
                Received::Data => {
                    idle = 0;
                    if !progress("downloading", data.len()) {
                        super::tcp_close(sock);
                        return Err(String::from("stopped"));
                    }
                    if data.len() > 4 * 1024 * 1024 {
                        break;
                    }
                }
                Received::Closed => break,
                Received::Timeout => {
                    idle += 1;
                    if idle > 20 {
                        break;
                    }
                    if !progress("waiting for the server", data.len()) {
                        super::tcp_close(sock);
                        return Err(String::from("stopped"));
                    }
                }
            }
        }
        super::tcp_close(sock);
        let (status, headers, body) = split_response(&data).ok_or_else(|| String::from("the server sent something that is not HTTP"))?;
        if matches!(status, 301 | 302 | 303 | 307 | 308) {
            if let Some(location) = header(&headers, "location") {
                let next = url.join(&location);
                url = Url::parse(&next)?;
                continue;
            }
        }
        let body = if header(&headers, "transfer-encoding").map(|v| v.to_ascii_lowercase().contains("chunked")).unwrap_or(false) {
            dechunk(&body)
        } else {
            body
        };
        let content_type = header(&headers, "content-type").unwrap_or_default();
        return Ok(Response { status, content_type, body, url });
    }
    Err(String::from("too many redirects"))
}

fn header(headers: &[(String, String)], name: &str) -> Option<String> {
    headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone())
}

fn split_response(data: &[u8]) -> Option<(u16, Vec<(String, String)>, Vec<u8>)> {
    let end = data.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = String::from_utf8_lossy(&data[..end]).into_owned();
    let mut lines = head.lines();
    let status_line = lines.next()?;
    if !status_line.starts_with("HTTP/") {
        return None;
    }
    let status: u16 = status_line.split_whitespace().nth(1)?.parse().ok()?;
    let headers = lines.filter_map(|l| l.split_once(':')).map(|(k, v)| (String::from(k.trim()), String::from(v.trim()))).collect();
    Some((status, headers, data[end + 4..].to_vec()))
}

fn dechunk(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < body.len() {
        let Some(line_end) = body[i..].windows(2).position(|w| w == b"\r\n") else { break };
        let size_text = String::from_utf8_lossy(&body[i..i + line_end]).into_owned();
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16).unwrap_or(0);
        i += line_end + 2;
        if size == 0 || i + size > body.len() {
            out.extend_from_slice(&body[i.min(body.len())..(i + size).min(body.len())]);
            break;
        }
        out.extend_from_slice(&body[i..i + size]);
        i += size + 2;
    }
    out
}

#[allow(dead_code)]
fn unused(_: NetError) {}
