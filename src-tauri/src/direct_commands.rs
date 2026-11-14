use crate::{direct, model::*, start_task, trusted, AppState};
use std::path::PathBuf;
use tauri::{State, WebviewWindow};

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn enqueue_direct(
    window: WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    url: String,
    filename: String,
    connections: u8,
    headers: Option<Vec<String>>,
    from_browser: Option<bool>,
) -> Result<Task, String> {
    trusted(&window)?;
    direct::validate_connections(connections)?;
    // Only the headers a browser hand-off needs survive; a link pasted by hand
    // carries none and keeps working exactly as before.
    let headers = direct::validate_headers(&headers.unwrap_or_default())?;
    let url = direct::validate_url(&url)?.to_string();
    let _guard = state.resolving.lock().await;
    let directory = state
        .store
        .lock()
        .unwrap()
        .setting("download_dir")
        .ok_or("请先选择下载目录")?;
    let existing = state.store.lock().unwrap().tasks()?;
    if existing.iter().any(|t| {
        t.direct.as_ref().is_some_and(|s| s.url == url)
            && t.directory.eq_ignore_ascii_case(&directory)
            && !matches!(t.status.as_str(), "complete" | "error" | "cancelled")
    }) {
        return Err("该链接已在下载列表中，可继续现有任务".into());
    }
    let metadata = direct::probe(&url, &headers).await?;
    // A link a browser handed over keeps that fact, so the list can say where
    // it came from; a pasted link stays an ordinary direct download.
    let origin = from_browser
        .filter(|handed_over| *handed_over)
        .map(|_| TaskOrigin {
            site: direct::referring_site(&headers).unwrap_or_default(),
        });
    let name = direct::available_name(
        &PathBuf::from(&directory),
        if filename.trim().is_empty() {
            &metadata.name
        } else {
            filename.trim()
        },
        &existing,
    );
    let task = Task {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.clone(),
        size: metadata.size,
        completed: 0,
        speed: 0,
        status: "resolving".into(),
        error: String::new(),
        directory,
        created_at: now(),
        gid: None,
        // Kept for compatibility with earlier task records; direct sources are resolved first.
        share: ShareLink {
            provider: Provider::Quark,
            id: String::new(),
            passcode: String::new(),
        },
        file: ShareFile {
            id: String::new(),
            name,
            size: metadata.size,
            is_dir: false,
            parent_id: String::new(),
            token: String::new(),
        },
        own: None,
        direct: Some(DirectSource {
            url,
            connections,
            range_supported: metadata.range_supported,
            etag: metadata.etag.clone(),
            last_modified: metadata.last_modified.clone(),
            headers,
        }),
        connections: 0,
        bt: None,
        upload_speed: 0,
        batch: None,
        origin,
    };
    start_task(&app, &state, task, Some(metadata), None).await
}
