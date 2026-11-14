use crate::{
    model::*, providers::cloud::Listing, resolve_tasks_in_background, session, trusted, AppState,
};
use std::{collections::HashSet, path::PathBuf};
use tauri::{State, WebviewWindow};

#[tauri::command]
pub async fn cloud_list(
    window: WebviewWindow,
    app: tauri::AppHandle,
    provider: Provider,
    path: String,
    page: u32,
    cursor: Option<String>,
) -> Result<Listing, String> {
    trusted(&window)?;
    let session = session::connect(&app, provider).await?;
    let account_id = session.cloud_identity().await?.0;
    let batch = session.cloud_page(&path, page, cursor.as_deref()).await?;
    Ok(Listing {
        account_id,
        path,
        files: batch.entries.into_iter().map(|entry| entry.file).collect(),
        next_cursor: batch.next,
    })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirListing {
    pub account_id: String,
    pub path: String,
    pub files: Vec<DriveFile>,
    pub total: usize,
    pub truncated: bool,
}

fn file_category(name: &str) -> &'static str {
    let ext = name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    const VIDEO: &[&str] = &[
        "mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "ts", "m4v", "rmvb", "mpeg", "mpg", "3gp",
    ];
    const AUDIO: &[&str] = &[
        "mp3", "flac", "wav", "aac", "ogg", "m4a", "wma", "ape", "opus",
    ];
    const IMAGE: &[&str] = &[
        "jpg", "jpeg", "png", "gif", "bmp", "webp", "svg", "ico", "tif", "tiff", "heic", "raw",
    ];
    const TEXT: &[&str] = &[
        "txt", "md", "log", "doc", "docx", "pdf", "xls", "xlsx", "ppt", "pptx", "csv", "rtf",
        "epub",
    ];
    const FONT: &[&str] = &["ttf", "otf", "woff", "woff2"];
    const SUBTITLE: &[&str] = &["srt", "ass", "ssa", "vtt", "sub"];
    const PACKAGE: &[&str] = &["exe", "msi", "apk", "dmg", "pkg", "deb", "appx", "ipa"];
    const ARCHIVE: &[&str] = &["zip", "rar", "7z", "tar", "gz", "bz2", "xz", "iso", "cab"];
    const TORRENT: &[&str] = &["torrent"];
    match ext.as_str() {
        e if VIDEO.contains(&e) => "video",
        e if AUDIO.contains(&e) => "audio",
        e if IMAGE.contains(&e) => "image",
        e if TEXT.contains(&e) => "text",
        e if FONT.contains(&e) => "font",
        e if SUBTITLE.contains(&e) => "subtitle",
        e if PACKAGE.contains(&e) => "package",
        e if ARCHIVE.contains(&e) => "archive",
        e if TORRENT.contains(&e) => "torrent",
        _ => "other",
    }
}

/// Fetches an entire directory (all pages) so sorting and filtering can cover
/// the whole listing instead of just the first loaded page.
#[tauri::command]
pub async fn dir_list_all(
    window: WebviewWindow,
    app: tauri::AppHandle,
    provider: Provider,
    path: String,
    sort_key: Option<String>,
    sort_desc: Option<bool>,
    file_types: Option<Vec<String>>,
) -> Result<DirListing, String> {
    trusted(&window)?;
    let session = session::connect(&app, provider).await?;
    let mut files: Vec<DriveFile> = Vec::new();
    let mut truncated = false;
    let account_id;
    if provider == Provider::Baidu {
        let mut id = String::new();
        for page in 1..=200u32 {
            let listing = session.drive_list(&path, page, "").await?;
            id = listing.account_id;
            let more = listing.has_more;
            files.extend(listing.files);
            if !more {
                break;
            }
            if page == 200 {
                truncated = true;
            }
        }
        account_id = id;
    } else {
        account_id = session.cloud_identity().await?.0;
        let mut cursor: Option<String> = None;
        for page in 1..=200u32 {
            let batch = session.cloud_page(&path, page, cursor.as_deref()).await?;
            cursor = batch.next.clone();
            files.extend(batch.entries.into_iter().map(|entry| entry.file));
            if cursor.is_none() {
                break;
            }
            if page == 200 {
                truncated = true;
            }
        }
    }
    if let Some(types) = file_types {
        let wanted: HashSet<String> = types.into_iter().filter(|t| t != "all").collect();
        if !wanted.is_empty() {
            files.retain(|file| !file.is_dir && wanted.contains(file_category(&file.name)));
        }
    }
    let key = sort_key.as_deref().unwrap_or("name");
    let desc = sort_desc.unwrap_or(false);
    files.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            return if a.is_dir {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        let ord = match key {
            "size" => a.size.cmp(&b.size),
            "modified" => a.modified.cmp(&b.modified),
            "created" => a.created.cmp(&b.created),
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        };
        if desc {
            ord.reverse()
        } else {
            ord
        }
    });
    let total = files.len();
    Ok(DirListing {
        account_id,
        path,
        files,
        total,
        truncated,
    })
}

#[tauri::command]
pub async fn cloud_enqueue(
    window: WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    provider: Provider,
    account_id: String,
    files: Vec<DriveFile>,
) -> Result<Vec<Task>, String> {
    trusted(&window)?;
    if files.is_empty() || files.len() > 100 {
        return Err("一次请选择 1 至 100 项下载".into());
    }
    let _guard = state.resolving.lock().await;
    let session = session::connect(&app, provider).await?;
    if session.cloud_identity().await?.0 != account_id {
        return Err("网盘账号已切换，请刷新后重新选择".into());
    }
    let directory = PathBuf::from(
        state
            .store
            .lock()
            .unwrap()
            .setting("download_dir")
            .ok_or("请先选择下载目录")?,
    );
    let mut pending = Vec::new();
    for selected in files {
        let fresh = session.cloud_find(&selected).await?.file;
        if fresh.size != selected.size
            || fresh.is_dir != selected.is_dir
            || fresh.name != selected.name
        {
            return Err("文件列表已变化，请刷新后重新选择".into());
        }
        pending.push((fresh, directory.clone()));
    }
    let mut visited = HashSet::new();
    let mut downloads = Vec::new();
    while let Some((file, directory)) = pending.pop() {
        if !visited.insert(file.id.clone()) {
            continue;
        }
        if visited.len() + pending.len() > 10000 {
            return Err("所选文件超过 10000 项，请分批下载".into());
        }
        if file.is_dir {
            let mut cursor = None;
            for page in 1..=200 {
                let batch = session
                    .cloud_page(&file.path, page, cursor.as_deref())
                    .await?;
                pending.extend(
                    batch
                        .entries
                        .into_iter()
                        .map(|entry| (entry.file, directory.join(safe_name(&file.name)))),
                );
                if visited.len() + pending.len() > 10000 {
                    return Err("所选文件超过 10000 项，请分批下载".into());
                }
                cursor = batch.next;
                if cursor.is_none() {
                    break;
                }
                if page == 200 {
                    return Err("所选目录超过分页上限，请分批下载".into());
                }
            }
        } else {
            downloads.push((file, directory))
        }
    }
    let mut tasks = Vec::new();
    for (file, directory) in downloads {
        let existing = state.store.lock().unwrap().tasks()?;
        if existing.iter().any(|task| {
            task.share.provider == provider
                && task.own.as_ref().is_some_and(|source| {
                    source.account_id == account_id && source.file.id == file.id
                })
                && task.directory == directory.to_string_lossy()
                && !matches!(task.status.as_str(), "error" | "cancelled")
        }) {
            continue;
        }
        let name = crate::direct::available_name(&directory, &file.name, &existing);
        let task = Task {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            size: file.size,
            completed: 0,
            speed: 0,
            status: "resolving".into(),
            error: String::new(),
            directory: directory.to_string_lossy().into(),
            created_at: now(),
            gid: None,
            share: ShareLink {
                provider,
                id: String::new(),
                passcode: String::new(),
            },
            file: ShareFile {
                id: file.id.clone(),
                name: file.name.clone(),
                size: file.size,
                is_dir: false,
                parent_id: String::new(),
                token: String::new(),
            },
            own: Some(OwnSource {
                account_id: account_id.clone(),
                file,
            }),
            direct: None,
            connections: 0,
            bt: None,
            upload_speed: 0,
        };
        state.store.lock().unwrap().save(&task)?;
        tasks.push(task);
    }
    if tasks.is_empty() {
        return Err("文件夹为空，或所选文件已在下载列表中".into());
    }
    resolve_tasks_in_background(app.clone(), tasks.clone());
    Ok(tasks)
}
