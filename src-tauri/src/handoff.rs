//! Browser hand-off through the `clouddownloader://` protocol and the local
//! bridge in [`crate::bridge`].
//!
//! Windows starts a new process with the URI as the only argument, so the scheme
//! registration plus `single-instance` are what let a click in the browser reach
//! the running window. The link is queued here instead of being emitted straight
//! to the front end: the webview can still be loading (cold start) or reloaded
//! (dev), and a queued link survives both. The front end asks for it with the
//! `take_handoff` command, and the emitted event is only a wake-up signal.
//!
//! A hand-off may also carry the request headers the browser had for the same
//! URL (Cookie, Referer, User-Agent). The extension sends them through the local
//! bridge so a link that needs the site login still works after it is moved out
//! of the browser.

use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use tauri::{Emitter, Manager};

/// Scheme declared in `tauri.conf.json > plugins.deep-link > desktop > schemes`.
/// Windows hands the URI over as a single argument; both the `//` form used by
/// browsers and the bare `scheme:` form are accepted.
const URI_PREFIX: &str = "clouddownloader://";
const URI_PREFIX_BARE: &str = "clouddownloader:";

/// Longest link accepted from a browser. Windows caps a protocol argument well
/// below this, but the bridge accepts arbitrary bodies, so keep one bound.
pub const MAX_LINK: usize = 16384;
const MAX_QUEUE: usize = 8;

/// One link handed over by the browser, with what the browser knew about it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Handoff {
    pub url: String,
    /// File name the browser had already chosen, so the download box can show
    /// the same name before the server answers.
    #[serde(default)]
    pub filename: Option<String>,
    /// `Name: value` request headers (Cookie, Referer, User-Agent). Already
    /// validated by [`crate::direct::validate_headers`]; the magnet form has none.
    #[serde(default)]
    pub headers: Vec<String>,
}

/// Links received while the window was not ready to show them yet.
#[derive(Default)]
pub struct Inbox(Mutex<Vec<Handoff>>);

impl Inbox {
    fn push(&self, item: Handoff) {
        let mut queue = self.0.lock().unwrap();
        // The browser can hand the same URL over twice (a re-download); the
        // newer headers are the ones that match the current session.
        if let Some(existing) = queue.iter_mut().find(|queued| queued.url == item.url) {
            *existing = item;
            return;
        }
        if queue.len() < MAX_QUEUE {
            queue.push(item);
        }
    }

    /// Takes every queued link, oldest first.
    pub fn drain(&self) -> Vec<Handoff> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

/// Accepts only what the download box routes today, so nothing handed over from
/// a browser or a shell can point the engine at a local file or a script.
pub fn checked_link(input: &str) -> Option<String> {
    let link = input.trim();
    if link.is_empty() || link.len() > MAX_LINK || link.chars().any(char::is_control) {
        return None;
    }
    let lower = link.to_ascii_lowercase();
    if ["http://", "https://", "magnet:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
    {
        Some(link.to_string())
    } else {
        None
    }
}

fn is_magnet(link: &str) -> bool {
    link.get(..7)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("magnet:"))
}

/// Extracts the download link from `clouddownloader://add?url=<encoded>`.
///
/// Only links the download box already routes are accepted, and anything the
/// user could paste into a shell or a second protocol handler is rejected.
pub fn parse(input: &str) -> Option<Handoff> {
    let input = input.trim();
    if input.len() > MAX_LINK {
        return None;
    }
    let rest = input
        .strip_prefix(URI_PREFIX)
        .or_else(|| input.strip_prefix(URI_PREFIX_BARE))?;
    let (action, query) = rest.split_once('?')?;
    if !action.trim_end_matches('/').eq_ignore_ascii_case("add") {
        return None;
    }
    let link = url::form_urlencoded::parse(query.as_bytes())
        .find(|(key, _)| key == "url")
        .map(|(_, value)| value.into_owned())?;
    let link = checked_link(&link)?;
    if !is_magnet(&link) {
        // A protocol URL can be opened by any local program; keep it from
        // pointing the engine at this machine or the intranet.
        let target = url::Url::parse(&link).ok()?;
        if crate::direct::is_local_target(&target) {
            return None;
        }
    }
    Some(Handoff {
        url: link,
        filename: None,
        headers: Vec::new(),
    })
}

/// Builds a hand-off from what the extension saw for one browser download.
///
/// Absurd header values are dropped rather than rejected: a missing cookie only
/// costs the login of one download, while failing the whole request would leave
/// the browser download cancelled with nothing to replace it.
pub fn browser(
    url: &str,
    filename: Option<&str>,
    referrer: Option<&str>,
    cookie: Option<&str>,
    agent: Option<&str>,
) -> Result<Handoff, String> {
    let url = checked_link(url).ok_or("只接受 HTTP/HTTPS 直链或磁力链接")?;
    if !is_magnet(&url) {
        // Anything that can POST to the bridge would otherwise be able to walk
        // the intranet with the browser's cookies attached.
        let target = url::Url::parse(&url).map_err(|_| "下载链接格式无效")?;
        if crate::direct::is_local_target(&target) {
            return Err("浏览器接力不支持本机或内网地址，请改用「用浏览器下载」".into());
        }
    }
    let mut headers = Vec::new();
    if !is_magnet(&url) {
        for (name, value) in [
            ("Referer", referrer),
            ("Cookie", cookie),
            ("User-Agent", agent),
        ] {
            if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
                if value.len() <= MAX_LINK {
                    headers.push(format!("{name}: {value}"));
                }
            }
        }
        headers = crate::direct::validate_headers(&headers).unwrap_or_default();
    }
    let filename = filename
        .map(str::trim)
        .filter(|name| !name.is_empty())
        // Browsers send the suggested name, but a path separator showing up here
        // must not become part of the file name.
        .map(|name| name.rsplit(['\\', '/']).next().unwrap_or(name))
        .map(crate::model::safe_name)
        .filter(|name| !name.is_empty());
    Ok(Handoff {
        url,
        filename,
        headers,
    })
}

/// Queues a link without touching the window, for a cold start where the webview
/// is still loading and showing the frame early would flash an empty window.
pub fn enqueue(app: &tauri::AppHandle, item: Handoff) {
    if let Some(inbox) = app.try_state::<Inbox>() {
        inbox.push(item);
    }
}

/// Queues a link, brings the window back and wakes the front end.
pub fn push(app: &tauri::AppHandle, item: Handoff) {
    enqueue(app, item);
    crate::tray::restore(app);
    let _ = app.emit_to("main", "handoff", ());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link(parsed: Option<Handoff>) -> Option<String> {
        parsed.map(|item| item.url)
    }

    #[test]
    fn accepts_encoded_http_links() {
        assert_eq!(
            link(parse(
                "clouddownloader://add?url=https%3A%2F%2Fexample.com%2Fa.bin"
            ))
            .as_deref(),
            Some("https://example.com/a.bin")
        );
        // A browser may hand the query over without percent encoding.
        assert_eq!(
            link(parse("clouddownloader://add?url=https://example.com/a.bin")).as_deref(),
            Some("https://example.com/a.bin")
        );
        assert_eq!(
            link(parse("clouddownloader:add?url=https://example.com/a.bin")).as_deref(),
            Some("https://example.com/a.bin")
        );
        assert_eq!(
            link(parse(
                "clouddownloader://ADD/?url=https://example.com/a.bin"
            ))
            .as_deref(),
            Some("https://example.com/a.bin")
        );
    }

    #[test]
    fn accepts_magnet_links() {
        let magnet = "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=a";
        let uri = format!(
            "clouddownloader://add?url={}",
            url::form_urlencoded::byte_serialize(magnet.as_bytes()).collect::<String>()
        );
        assert_eq!(link(parse(&uri)).as_deref(), Some(magnet));
    }

    #[test]
    fn keeps_extra_parameters_out_of_the_link() {
        assert_eq!(
            link(parse(
                "clouddownloader://add?from=chrome&url=https%3A%2F%2Fexample.com%2Fa.bin"
            ))
            .as_deref(),
            Some("https://example.com/a.bin")
        );
    }

    #[test]
    fn rejects_other_inputs() {
        for input in [
            "",
            "https://example.com/a.bin",
            "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567",
            "clouddownloader://add",
            "clouddownloader://add?url=",
            "clouddownloader://add?url=file%3A%2F%2F%2FC%3A%2FWindows%2Fnotepad.exe",
            "clouddownloader://add?url=javascript%3Aalert(1)",
            "clouddownloader://open?url=https%3A%2F%2Fexample.com%2Fa.bin",
            "other://add?url=https%3A%2F%2Fexample.com%2Fa.bin",
            // A newline would smuggle a second request into anything downstream.
            "clouddownloader://add?url=https%3A%2F%2Fexample.com%2Fa.bin%0Ahttp%3A%2F%2Fb",
        ] {
            assert_eq!(parse(input), None, "{input}");
        }
    }

    #[test]
    fn rejects_oversized_links() {
        let uri = format!(
            "clouddownloader://add?url=https%3A%2F%2Fexample.com%2F{}",
            "a".repeat(MAX_LINK)
        );
        assert_eq!(parse(&uri), None);
    }

    #[test]
    fn queue_drains_once_and_keeps_the_newest_headers() {
        let inbox = Inbox::default();
        let first = Handoff {
            url: "https://example.com/a".into(),
            ..Handoff::default()
        };
        let mut second = first.clone();
        second.headers = vec!["Cookie: session=1".into()];
        inbox.push(first);
        inbox.push(second.clone());
        inbox.push(Handoff {
            url: "https://example.com/b".into(),
            ..Handoff::default()
        });
        assert_eq!(
            inbox.drain(),
            vec![
                second,
                Handoff {
                    url: "https://example.com/b".into(),
                    ..Handoff::default()
                }
            ]
        );
        assert!(inbox.drain().is_empty());
    }

    #[test]
    fn browser_handoff_keeps_only_download_headers() {
        let item = browser(
            "https://example.com/a.bin",
            Some("C:\\Users\\me\\a.bin"),
            Some("https://example.com/list"),
            Some("session=abc; theme=dark"),
            Some("Mozilla/5.0"),
        )
        .unwrap();
        assert_eq!(item.filename.as_deref(), Some("a.bin"));
        assert_eq!(
            item.headers,
            vec![
                "Referer: https://example.com/list".to_string(),
                "Cookie: session=abc; theme=dark".to_string(),
                "User-Agent: Mozilla/5.0".to_string(),
            ]
        );
        // A magnet link has no browser session to carry over.
        let magnet = browser(
            "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567",
            None,
            Some("https://example.com/"),
            Some("session=abc"),
            None,
        )
        .unwrap();
        assert!(magnet.headers.is_empty());
        // A header value cannot smuggle a second header or a request line.
        let smuggled = browser(
            "https://example.com/a.bin",
            None,
            None,
            Some("session=abc\r\nX-Evil: 1"),
            None,
        )
        .unwrap();
        assert!(smuggled.headers.is_empty());
        assert!(browser("file:///C:/Windows", None, None, None, None).is_err());
    }
}
