//! Browser hand-off through the `clouddownloader://` protocol.
//!
//! Windows starts a new process with the URI as the only argument, so the scheme
//! registration in [`SCHEME`] plus `single-instance` are what let a click in the
//! browser reach the running window. The link is queued here instead of being
//! emitted straight to the front end: the webview can still be loading (cold
//! start) or reloaded (dev), and a queued link survives both. The front end asks
//! for it with the `take_handoff` command, and the emitted event is only a wake-up
//! signal.

use std::sync::Mutex;
use tauri::{Emitter, Manager};

/// Scheme declared in `tauri.conf.json > plugins.deep-link > desktop > schemes`.
/// Windows hands the URI over as a single argument; both the `//` form used by
/// browsers and the bare `scheme:` form are accepted.
const URI_PREFIX: &str = "clouddownloader://";
const URI_PREFIX_BARE: &str = "clouddownloader:";

const MAX_LINK: usize = 16384;
const MAX_QUEUE: usize = 8;

/// Links received while the window was not ready to show them yet.
#[derive(Default)]
pub struct Inbox(Mutex<Vec<String>>);

impl Inbox {
    fn push(&self, link: String) {
        let mut queue = self.0.lock().unwrap();
        if queue.len() < MAX_QUEUE && !queue.contains(&link) {
            queue.push(link);
        }
    }

    /// Takes every queued link, oldest first.
    pub fn drain(&self) -> Vec<String> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

/// Extracts the download link from `clouddownloader://add?url=<encoded>`.
///
/// Only links the download box already routes are accepted, and anything the
/// user could paste into a shell or a second protocol handler is rejected.
pub fn parse(input: &str) -> Option<String> {
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
        .map(|(_, value)| value.trim().to_string())?;
    if link.is_empty() || link.len() > MAX_LINK || link.chars().any(char::is_control) {
        return None;
    }
    let lower = link.to_ascii_lowercase();
    if ["http://", "https://", "magnet:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
    {
        Some(link)
    } else {
        None
    }
}

/// Queues a link without touching the window, for a cold start where the webview
/// is still loading and showing the frame early would flash an empty window.
pub fn enqueue(app: &tauri::AppHandle, link: String) {
    if let Some(inbox) = app.try_state::<Inbox>() {
        inbox.push(link);
    }
}

/// Queues a link, brings the window back and wakes the front end.
pub fn push(app: &tauri::AppHandle, link: String) {
    enqueue(app, link);
    crate::tray::restore(app);
    let _ = app.emit_to("main", "handoff", ());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_encoded_http_links() {
        assert_eq!(
            parse("clouddownloader://add?url=https%3A%2F%2Fexample.com%2Fa.bin").as_deref(),
            Some("https://example.com/a.bin")
        );
        // A browser may hand the query over without percent encoding.
        assert_eq!(
            parse("clouddownloader://add?url=https://example.com/a.bin").as_deref(),
            Some("https://example.com/a.bin")
        );
        assert_eq!(
            parse("clouddownloader:add?url=https://example.com/a.bin").as_deref(),
            Some("https://example.com/a.bin")
        );
        assert_eq!(
            parse("clouddownloader://ADD/?url=https://example.com/a.bin").as_deref(),
            Some("https://example.com/a.bin")
        );
    }

    #[test]
    fn accepts_magnet_links() {
        let link = "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=a";
        let uri = format!(
            "clouddownloader://add?url={}",
            url::form_urlencoded::byte_serialize(link.as_bytes()).collect::<String>()
        );
        assert_eq!(parse(&uri).as_deref(), Some(link));
    }

    #[test]
    fn keeps_extra_parameters_out_of_the_link() {
        assert_eq!(
            parse("clouddownloader://add?from=chrome&url=https%3A%2F%2Fexample.com%2Fa.bin")
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
    fn queue_drains_once_and_ignores_duplicates() {
        let inbox = Inbox::default();
        inbox.push("https://example.com/a".into());
        inbox.push("https://example.com/a".into());
        inbox.push("https://example.com/b".into());
        assert_eq!(
            inbox.drain(),
            vec![
                "https://example.com/a".to_string(),
                "https://example.com/b".to_string()
            ]
        );
        assert!(inbox.drain().is_empty());
    }
}
