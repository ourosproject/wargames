//! Thin-model backend. An OpenAI-compatible (Ollama) endpoint picks ONE card per turn from the
//! legal menu. THICK ENGINE / THIN MODEL: the engine stays the authority — the model's pick is
//! validated against the legal set by the caller, and any failure (endpoint down, bad JSON,
//! illegal card, timeout) falls back to the heuristic. Air-gapped: a hand-rolled std::net
//! HTTP/1.1 client (LAN, plain HTTP, no TLS), so no new crate dependency is pulled.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde_json::{json, Value};

/// Default range inference node (VM106, qwen2.5:7b). Override via WARGAME_MODEL_URL / WARGAME_MODEL.
pub fn default_url() -> String {
    std::env::var("WARGAME_MODEL_URL")
        .unwrap_or_else(|_| "http://10.10.40.20:11434/v1/chat/completions".into())
}
pub fn default_model() -> String {
    std::env::var("WARGAME_MODEL").unwrap_or_else(|_| "qwen2.5:7b".into())
}
fn timeout() -> Duration {
    let s = std::env::var("WARGAME_MODEL_TIMEOUT").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(25);
    Duration::from_secs(s.clamp(3, 120))
}

/// Ask the model for a JSON-object reply and return `choices[0].message.content` parsed as JSON.
/// `None` on any transport/parse failure — callers fall back to the heuristic.
pub fn chat_json(system: &str, user: &str) -> Option<Value> {
    let body = json!({
        "model": default_model(),
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ],
        "response_format": {"type": "json_object"},
        "temperature": 0.2,
        "stream": false
    });
    let resp = http_post_json(&default_url(), &body, timeout())?;
    let content = resp.get("choices")?.get(0)?.get("message")?.get("content")?.as_str()?;
    serde_json::from_str::<Value>(content.trim()).ok()
}

/// Minimal blocking HTTP/1.1 POST of a JSON body over plain TCP. Returns the parsed JSON response
/// body. Handles both Content-Length and chunked transfer-encoding; bounded by `to`.
fn http_post_json(url: &str, body: &Value, to: Duration) -> Option<Value> {
    let rest = url.strip_prefix("http://")?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse::<u16>().ok()?),
        None => (authority.to_string(), 80),
    };
    let payload = serde_json::to_vec(body).ok()?;
    // Try every resolved address, not just the first: a hostname like `localhost` resolves to
    // both ::1 and 127.0.0.1, and an IPv4-only server (Ollama binds v4 by default) is unreachable
    // over the v6 candidate. `.next()` alone would silently fail on macOS dual-stack. A raw IPv4
    // literal (the VM106 default) resolves to one addr, so this only matters for hostname overrides.
    let mut stream = format!("{host}:{port}").to_socket_addrs().ok()?
        .find_map(|addr| TcpStream::connect_timeout(&addr, to).ok())?;
    stream.set_read_timeout(Some(to)).ok()?;
    stream.set_write_timeout(Some(to)).ok()?;
    let head = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        payload.len()
    );
    stream.write_all(head.as_bytes()).ok()?;
    stream.write_all(&payload).ok()?;
    stream.flush().ok()?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).ok()?;
    let sep = find_sub(&raw, b"\r\n\r\n")?;
    let headers = String::from_utf8_lossy(&raw[..sep]).to_lowercase();
    let body_bytes = &raw[sep + 4..];
    let text = if headers.contains("transfer-encoding: chunked") {
        dechunk(body_bytes)
    } else {
        String::from_utf8_lossy(body_bytes).into_owned()
    };
    serde_json::from_str::<Value>(text.trim()).ok()
}

fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn dechunk(b: &[u8]) -> String {
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let line_end = match find_sub(&b[i..], b"\r\n") { Some(x) => i + x, None => break };
        let size_str = String::from_utf8_lossy(&b[i..line_end]);
        let size = usize::from_str_radix(size_str.trim().split(';').next().unwrap_or("0").trim(), 16).unwrap_or(0);
        if size == 0 { break; }
        let start = line_end + 2;
        let end = (start + size).min(b.len());
        out.extend_from_slice(&b[start..end]);
        i = end + 2;
    }
    String::from_utf8_lossy(&out).into_owned()
}
