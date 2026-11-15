//! Local HTTP bridge for the browser extension.
//!
//! A page download (`Content-Disposition: attachment`) cannot be handed to
//! another program through the OS, so the extension cancels it in the browser
//! and posts the URL here instead. The server binds `127.0.0.1` only and accepts
//! a request only when its `Origin` is a Chrome/Edge extension: a web page cannot
//! forge that header, and a cross-site page would be blocked before reaching us.

use crate::handoff::{self, Handoff};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicU16, Ordering},
        Mutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// Default port, matching the one the extension asks for. The user can change
/// it in settings, so the listener watches [`PORT`] instead of a constant.
pub const DEFAULT_PORT: u16 = 17890;
const MAX_HEAD: usize = 8 * 1024;
const MAX_BODY: usize = 64 * 1024;

/// The port the socket should be on. The listener rebinds when it changes.
static PORT: AtomicU16 = AtomicU16::new(DEFAULT_PORT);
/// Whether the socket is open, so the settings page can say so.
static LISTENING: AtomicBool = AtomicBool::new(false);
/// The last request that arrived, so a hand-off that did not show up can be
/// explained without reading logs.
static LAST: Mutex<Option<LastHandoff>> = Mutex::new(None);
/// The last time the extension said hello, so the title bar can tell an
/// installed extension from a missing one without reading logs.
static SEEN: Mutex<Option<Seen>> = Mutex::new(None);
/// Links the app itself opened in the browser. Without this the extension would
/// hand the same URL back the moment the browser starts downloading it.
static IGNORED: Mutex<Vec<(String, u64)>> = Mutex::new(Vec::new());
/// How long an ignored link stays ignored; long enough for the browser to start.
const IGNORE_SECS: u64 = 60;
/// A ping older than this is stale: browsers stop background timers when they
/// are asleep, so the title bar only calls it missing after a generous gap.
const SEEN_FRESH_SECS: u64 = 180;

/// The last hello from the extension.
#[derive(Clone, Debug, Default)]
pub struct Seen {
    /// Unix seconds.
    pub at: u64,
    /// Version the extension reports, empty when it did not say.
    pub version: String,
}

/// One bridge request, kept for the settings page.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastHandoff {
    /// Host of the page that started the download; empty when unknown.
    pub site: String,
    pub ok: bool,
    /// Why it was refused, when it was.
    pub message: String,
    /// Unix seconds.
    pub at: u64,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default()
}

fn record(site: String, ok: bool, message: String) {
    if let Ok(mut last) = LAST.lock() {
        *last = Some(LastHandoff {
            site,
            ok,
            message,
            at: now(),
        });
    }
}

/// 记下扩展刚联系过本机，供标题栏的插件按钮判断它是否装好并在运行。
pub fn mark_seen(version: Option<&str>) {
    let mut version = version.unwrap_or_default().trim().to_string();
    version.truncate(32);
    if let Ok(mut seen) = SEEN.lock() {
        let now = now();
        // Keep the version we already know when a heartbeat omits it.
        if version.is_empty() {
            if let Some(previous) = seen.as_ref() {
                version = previous.version.clone();
            }
        }
        *seen = Some(Seen { at: now, version });
    }
}

/// What the settings page shows about the browser hand-off.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeStatus {
    /// The socket is open and accepting requests.
    pub listening: bool,
    pub port: u16,
    /// The user has not turned the bridge off in settings.
    pub enabled: bool,
    pub last: Option<LastHandoff>,
    /// Unix seconds of the last hello from the extension.
    pub seen_at: Option<u64>,
    /// Version the extension reported.
    pub version: String,
}

pub fn status(enabled: bool) -> BridgeStatus {
    let seen = SEEN.lock().ok().and_then(|seen| seen.clone());
    let fresh = seen
        .as_ref()
        .filter(|seen| now().saturating_sub(seen.at) <= SEEN_FRESH_SECS);
    BridgeStatus {
        listening: LISTENING.load(Ordering::Relaxed),
        port: PORT.load(Ordering::Relaxed),
        enabled,
        last: LAST.lock().ok().and_then(|last| last.clone()),
        seen_at: fresh
            .map(|seen| seen.at)
            .or_else(|| seen.as_ref().map(|seen| seen.at)),
        version: seen.map(|seen| seen.version).unwrap_or_default(),
    }
}

/// Asks the listener to move to another port; it rebinds within a moment.
/// Called after the settings are saved, so the extension keeps working when the
/// user picks a different port.
pub fn set_port(port: u16) {
    PORT.store(port, Ordering::Relaxed);
}

/// Starts the bridge on a background thread. A failed bind only costs the
/// extension path, so the failure is reported and startup continues.
pub fn start(app: tauri::AppHandle) {
    std::thread::spawn(move || serve(app));
}

fn serve(app: tauri::AppHandle) {
    let mut bound: Option<(u16, TcpListener)> = None;
    loop {
        let wanted = PORT.load(Ordering::Relaxed);
        if bound.as_ref().map(|(port, _)| *port) != Some(wanted) {
            bound = None;
            LISTENING.store(false, Ordering::Relaxed);
            match TcpListener::bind(("127.0.0.1", wanted)) {
                Ok(listener) => {
                    let _ = listener.set_nonblocking(true);
                    LISTENING.store(true, Ordering::Relaxed);
                    bound = Some((wanted, listener));
                }
                Err(error) => {
                    eprintln!("clouddownloader: 浏览器扩展桥未启动（127.0.0.1:{wanted}）：{error}");
                }
            }
        }
        match bound.as_ref() {
            Some((_, listener)) => match listener.accept() {
                Ok((stream, _)) => {
                    let app = app.clone();
                    std::thread::spawn(move || {
                        let _ = answer(stream, &app);
                    });
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(_) => std::thread::sleep(Duration::from_millis(500)),
            },
            // Nothing to accept: wait, then try the port again.
            None => std::thread::sleep(Duration::from_secs(2)),
        }
    }
}

fn answer(mut stream: TcpStream, app: &tauri::AppHandle) -> std::io::Result<()> {
    // Streams accepted from a non-blocking listener start non-blocking too.
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    let (head, body) = match read_request(&mut stream) {
        Ok(request) => request,
        Err((status, message)) => return reply(&mut stream, status, None, &message),
    };
    let origin = extension_origin(header_value(&head, "origin").as_deref());
    // Chrome's extension updater fetches these two without an Origin header, so
    // they live before the extension check: they only hand out the packaged
    // files, and the policy entry that points here is what authorises the
    // install, not the request itself.
    if let Some((method, target)) = request_line(&head) {
        if method == "GET" && matches!(target.as_str(), "/update.xml" | "/extension.crx") {
            return serve_extension(&mut stream, app, &target);
        }
    }
    // Any request from a real extension origin proves it is installed and
    // talking to us; heartbeats also carry the version for the title bar.
    if origin.is_some() {
        mark_seen(extension_version(&body).as_deref());
    }
    // A link the user asked us to open in the browser would come straight back
    // through /add, so tell the extension to leave that one alone.
    if origin.is_some() {
        if let Some(url) = payload_url(&body) {
            if ignored(&url) {
                return reply(
                    &mut stream,
                    200,
                    origin.as_deref(),
                    "{\"ok\":true,\"taken\":false}",
                );
            }
        }
    }
    let enabled = bridge_enabled(app);
    let adds = request_line(&head).is_some_and(|(_, target)| target == "/add");
    match route(&head, &body, origin.as_deref(), enabled) {
        Ok(Routed::Preflight) => reply(&mut stream, 204, origin.as_deref(), ""),
        Ok(Routed::Ping) => reply(
            &mut stream,
            200,
            origin.as_deref(),
            "{\"ok\":true,\"app\":\"clouddownloader\"}",
        ),
        Ok(Routed::Add(item)) => {
            // Queue before answering so the extension only removes the browser
            // download once the app really has the link.
            record(
                crate::direct::referring_site(&item.headers).unwrap_or_default(),
                true,
                String::new(),
            );
            handoff::push(app, item);
            reply(&mut stream, 200, origin.as_deref(), "{\"ok\":true}")
        }
        Err((status, message)) => {
            if adds {
                record(String::new(), false, message.clone());
            }
            reply(&mut stream, status, origin.as_deref(), &message)
        }
    }
}

/// Reads the user preference once per request; the setting changes rarely enough
/// that caching it would only make the toggle feel broken.
fn bridge_enabled(app: &tauri::AppHandle) -> bool {
    use tauri::Manager;
    let state = app.state::<crate::AppState>();
    let store = state.store.lock().unwrap();
    crate::settings::Settings::load(&store).browser_bridge
}

enum Routed {
    Add(Handoff),
    Ping,
    Preflight,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Payload {
    url: String,
    #[serde(default)]
    filename: Option<String>,
    #[serde(default)]
    referrer: Option<String>,
    #[serde(default)]
    cookie: Option<String>,
    #[serde(default)]
    user_agent: Option<String>,
}

/// What a request is allowed to ask for, with no I/O, so it can be tested.
fn route(
    head: &str,
    body: &[u8],
    origin: Option<&str>,
    enabled: bool,
) -> Result<Routed, (u16, String)> {
    let (method, target) = request_line(head).ok_or((400, "请求格式无效".into()))?;
    if method == "OPTIONS" {
        // Chrome preflights a JSON POST from an extension context.
        return match origin {
            Some(_) => Ok(Routed::Preflight),
            None => Err((403, "仅接受浏览器扩展的请求".into())),
        };
    }
    if method != "POST" {
        return Err((405, "只接受 POST 请求".into()));
    }
    if origin.is_none() {
        return Err((403, "仅接受浏览器扩展的请求".into()));
    }
    // The options page checks that the app is running without queueing anything.
    if target == "/ping" {
        return Ok(Routed::Ping);
    }
    if target != "/add" {
        return Err((404, "未知接口".into()));
    }
    // Turning the bridge off must beat a queued page download, so the check sits
    // before the payload is even parsed.
    if !enabled {
        return Err((503, "浏览器接力已在设置中关闭".into()));
    }
    let content_type = header_value(head, "content-type").unwrap_or_default();
    let content_type = content_type.to_ascii_lowercase();
    if !content_type.starts_with("application/json") && !content_type.starts_with("text/plain") {
        return Err((415, "请使用 JSON 请求体".into()));
    }
    let payload: Payload =
        serde_json::from_slice(body).map_err(|_| (400, "请求内容无效".into()))?;
    let item = handoff::browser(
        &payload.url,
        payload.filename.as_deref(),
        payload.referrer.as_deref(),
        payload.cookie.as_deref(),
        payload.user_agent.as_deref(),
    )
    .map_err(|message| (400, message))?;
    Ok(Routed::Add(item))
}

/// The version a heartbeat reports, if it sent one.
fn extension_version(body: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    value
        .get("version")
        .and_then(|version| version.as_str())
        .map(str::to_string)
}

/// Serves the extension package and its update manifest for the browser's
/// extension updater. Only reads files that ship with the app.
fn serve_extension(
    stream: &mut TcpStream,
    app: &tauri::AppHandle,
    target: &str,
) -> std::io::Result<()> {
    let Some(directory) = crate::extension_dir(app) else {
        return write_body(
            stream,
            404,
            "text/plain; charset=utf-8",
            b"extension folder missing",
        );
    };
    if target == "/update.xml" {
        let Some(id) = crate::extension_id(&directory) else {
            return write_body(
                stream,
                404,
                "text/plain; charset=utf-8",
                b"extension id unavailable",
            );
        };
        let manifest = std::fs::read_to_string(directory.join("manifest.json")).unwrap_or_default();
        let version = serde_json::from_str::<serde_json::Value>(&manifest)
            .ok()
            .and_then(|value| {
                value
                    .get("version")
                    .and_then(|version| version.as_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "0.0.0".into());
        let body = update_manifest(&id, PORT.load(Ordering::Relaxed), &version);
        return write_body(
            stream,
            200,
            "application/xml; charset=utf-8",
            body.as_bytes(),
        );
    }
    match std::fs::read(directory.join("clouddownloader.crx")) {
        Ok(bytes) => write_body(stream, 200, "application/x-chrome-extension", &bytes),
        Err(_) => write_body(stream, 404, "text/plain; charset=utf-8", b"crx not built"),
    }
}

fn write_body(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        reason(status),
        body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

/// 让扩展放过这条链接：应用自己刚把它交回浏览器。
pub fn ignore(url: &str) {
    let at = now();
    let mut list = IGNORED.lock().unwrap_or_else(|error| error.into_inner());
    list.retain(|(_, when)| at.saturating_sub(*when) < IGNORE_SECS);
    list.push((url.to_string(), at));
}

fn ignored(url: &str) -> bool {
    let at = now();
    let mut list = IGNORED.lock().unwrap_or_else(|error| error.into_inner());
    list.retain(|(_, when)| at.saturating_sub(*when) < IGNORE_SECS);
    list.iter().any(|(known, _)| known == url)
}

/// The URL in an `/add` body, for the ignore check.
fn payload_url(body: &[u8]) -> Option<String> {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()?
        .get("url")?
        .as_str()
        .map(str::to_string)
}

/// `chrome-extension://<id>` with the 32-letter id Chrome generates.
fn extension_origin(origin: Option<&str>) -> Option<String> {
    let origin = origin?;
    let id = origin.strip_prefix("chrome-extension://")?;
    if id.len() == 32 && id.bytes().all(|b| b.is_ascii_lowercase() && b <= b'p') {
        Some(origin.to_string())
    } else {
        None
    }
}

fn request_line(head: &str) -> Option<(String, String)> {
    let mut parts = head.lines().next()?.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    parts.next()?;
    Some((method, target))
}

fn header_value(head: &str, name: &str) -> Option<String> {
    head.lines().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim()
            .eq_ignore_ascii_case(name)
            .then(|| value.trim().to_string())
    })
}

/// Reads one request, refusing anything larger than a hand-off could be.
fn read_request(stream: &mut TcpStream) -> Result<(String, Vec<u8>), (u16, String)> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            break end;
        }
        if buffer.len() > MAX_HEAD {
            return Err((431, "请求头过大".into()));
        }
        match stream.read(&mut chunk) {
            Ok(0) => return Err((400, "请求不完整".into())),
            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
            Err(_) => return Err((408, "读取请求超时".into())),
        }
    };
    let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
    let length = header_value(&head, "content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    if length > MAX_BODY {
        return Err((413, "请求体过大".into()));
    }
    let mut body = buffer[head_end + 4..].to_vec();
    while body.len() < length {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => body.extend_from_slice(&chunk[..n]),
            Err(_) => return Err((408, "读取请求超时".into())),
        }
        if body.len() > MAX_BODY {
            return Err((413, "请求体过大".into()));
        }
    }
    body.truncate(length.min(MAX_BODY));
    Ok((head, body))
}

/// The update manifest Chrome's extension updater reads. The codebase points
/// back at this bridge, so the same file works on whatever port is configured.
fn update_manifest(id: &str, port: u16, version: &str) -> String {
    format!(
        "<?xml version='1.0' encoding='UTF-8'?>\n\
         <gupdate xmlns='http://www.google.com/update2/response' protocol='2.0'>\n  \
         <app appid='{id}'>\n    \
         <updatecheck codebase='http://127.0.0.1:{port}/extension.crx' version='{version}' />\n  \
         </app>\n\
         </gupdate>\n"
    )
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        431 => "Request Header Fields Too Large",
        503 => "Service Unavailable",
        _ => "Error",
    }
}

fn reply(
    stream: &mut TcpStream,
    status: u16,
    origin: Option<&str>,
    body: &str,
) -> std::io::Result<()> {
    let reason = reason(status);
    // The body is JSON in every case, including errors, so the extension can
    // show the reason instead of a generic failure.
    let body = if status == 204 {
        String::new()
    } else if status == 200 {
        body.to_string()
    } else {
        serde_json::json!({ "error": body }).to_string()
    };
    let mut response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n",
        body.len()
    );
    if let Some(origin) = origin {
        response.push_str(&format!(
            "Access-Control-Allow-Origin: {origin}\r\nVary: Origin\r\n"
        ));
        if status == 204 {
            response.push_str(
                "Access-Control-Allow-Methods: POST\r\nAccess-Control-Allow-Headers: content-type\r\nAccess-Control-Max-Age: 600\r\nAccess-Control-Allow-Private-Network: true\r\n",
            );
        }
    }
    response.push_str("\r\n");
    response.push_str(&body);
    stream.write_all(response.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: &str = "chrome-extension://abcdefghijklmnopabcdefghijklmnop";

    fn post(body: &str) -> String {
        format!(
            "POST /add HTTP/1.1\r\nHost: 127.0.0.1:17890\r\nOrigin: {ORIGIN}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
    }

    #[test]
    fn only_extension_origins_are_trusted() {
        assert_eq!(extension_origin(Some(ORIGIN)), Some(ORIGIN.to_string()));
        for origin in [
            "https://evil.example",
            "chrome-extension://",
            "chrome-extension://ABCDEFGHIJKLMNOPABCDEFGHIJKLMNOP",
            "chrome-extension://abcdefghijklmnopabcdefghijklmno",
            "chrome-extension://abcdefghijklmnopabcdefghijklmnop/",
            "moz-extension://abcdefghijklmnopabcdefghijklmnop",
        ] {
            assert_eq!(extension_origin(Some(origin)), None, "{origin}");
        }
        assert_eq!(extension_origin(None), None);
    }

    #[test]
    fn ignored_links_are_remembered_for_a_while() {
        let url = "https://example.test/a.iso";
        assert!(!ignored(url));
        ignore(url);
        assert!(ignored(url));
        assert!(!ignored("https://example.test/b.iso"));
        // Entries older than the window drop out on the next look.
        {
            let mut list = IGNORED.lock().unwrap_or_else(|error| error.into_inner());
            for entry in list.iter_mut() {
                entry.1 = entry.1.saturating_sub(IGNORE_SECS + 1);
            }
        }
        assert!(!ignored(url));
    }

    #[test]
    fn the_update_manifest_points_back_at_the_bridge() {
        let xml = update_manifest("bgmhpiikmknhjbgpedfgpelffapcagpa", 18900, "0.2.0");
        assert!(xml.contains("appid='bgmhpiikmknhjbgpedfgpelffapcagpa'"));
        assert!(xml.contains("codebase='http://127.0.0.1:18900/extension.crx'"));
        assert!(xml.contains("version='0.2.0'"));
        assert!(xml.starts_with("<?xml"));
    }

    #[test]
    fn adds_downloads_and_refuses_everything_else() {
        let body = serde_json::json!({
            "url": "https://example.test/a.zip",
            "filename": "C:\\Users\\me\\a.zip",
            "referrer": "https://example.test/list",
            "cookie": "uid=42",
            "userAgent": "Browser/1.0",
        })
        .to_string();
        let Ok(Routed::Add(item)) = route(&post(&body), body.as_bytes(), Some(ORIGIN), true) else {
            panic!("expected an add");
        };
        assert_eq!(item.url, "https://example.test/a.zip");
        assert_eq!(item.filename.as_deref(), Some("a.zip"));
        assert_eq!(
            item.headers,
            vec![
                "Referer: https://example.test/list".to_string(),
                "Cookie: uid=42".to_string(),
                "User-Agent: Browser/1.0".to_string(),
            ]
        );

        // Preflight only for a real extension.
        let preflight = format!("OPTIONS /add HTTP/1.1\r\nOrigin: {ORIGIN}\r\n\r\n");
        assert!(matches!(
            route(&preflight, b"", Some(ORIGIN), true),
            Ok(Routed::Preflight)
        ));
        assert_eq!(route(&preflight, b"", None, true).err().unwrap().0, 403);

        // The options page uses /ping to prove the app is running.
        let ping = format!("POST /ping HTTP/1.1\r\nOrigin: {ORIGIN}\r\n\r\n");
        assert!(matches!(
            route(&ping, b"", Some(ORIGIN), true),
            Ok(Routed::Ping)
        ));
        assert_eq!(route(&ping, b"", None, true).err().unwrap().0, 403);

        let json = format!(
            "POST /add HTTP/1.1\r\nOrigin: {ORIGIN}\r\nContent-Type: application/json\r\n\r\n"
        );
        let cases: [(String, Option<&str>, u16); 6] = [
            (
                format!("GET /add HTTP/1.1\r\nOrigin: {ORIGIN}\r\n\r\n"),
                Some(ORIGIN),
                405,
            ),
            (
                format!("POST /other HTTP/1.1\r\nOrigin: {ORIGIN}\r\n\r\n"),
                Some(ORIGIN),
                404,
            ),
            ("POST /add HTTP/1.1\r\n\r\n".to_string(), None, 403),
            (
                "POST /add HTTP/1.1\r\nOrigin: https://evil.example\r\n\r\n".to_string(),
                None,
                403,
            ),
            (
                format!(
                    "POST /add HTTP/1.1\r\nOrigin: {ORIGIN}\r\nContent-Type: text/html\r\n\r\n"
                ),
                Some(ORIGIN),
                415,
            ),
            (json.clone(), Some(ORIGIN), 400),
        ];
        for (head, origin, status) in cases {
            assert_eq!(
                route(&head, b"{}", origin, true).err().unwrap().0,
                status,
                "{head}"
            );
        }
        // Valid JSON is not enough: the link itself has to be routable.
        let local = r#"{"url":"file:///C:/Windows/System32/cmd.exe"}"#;
        assert_eq!(
            route(&json, local.as_bytes(), Some(ORIGIN), true)
                .err()
                .unwrap()
                .0,
            400
        );
        // Turning the bridge off refuses the add, and says why.
        let (status, message) = route(&post(&body), body.as_bytes(), Some(ORIGIN), false)
            .err()
            .unwrap();
        assert_eq!(status, 503);
        assert!(message.contains("设置"), "{message}");
    }

    #[test]
    fn reads_a_whole_request_and_bounds_its_size() {
        fn roundtrip(request: &str) -> Result<(String, Vec<u8>), (u16, String)> {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let payload = request.as_bytes().to_vec();
            std::thread::spawn(move || {
                if let Ok(mut client) = TcpStream::connect(address) {
                    let _ = client.write_all(&payload);
                }
            });
            let (mut stream, _) = listener.accept().unwrap();
            read_request(&mut stream)
        }

        let body = r#"{"url":"https://example.test/a.zip"}"#;
        let request = format!(
            "POST /add HTTP/1.1\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let (head, read) = roundtrip(&request).unwrap();
        assert!(head.starts_with("POST /add HTTP/1.1"), "{head}");
        assert_eq!(read, body.as_bytes());

        let huge = format!(
            "POST /add HTTP/1.1\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY + 1
        );
        assert_eq!(roundtrip(&huge).err().unwrap().0, 413);
        // A head with no terminator is refused instead of grown without limit.
        assert_eq!(roundtrip(&"x".repeat(MAX_HEAD + 1)).err().unwrap().0, 431);
    }
}
