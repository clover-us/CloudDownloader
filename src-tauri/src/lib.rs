mod bt;
mod cloud_commands;
mod direct;
mod direct_commands;
mod download_commands;
mod download_errors;
mod drive_commands;
mod engine;
mod migration;
mod model;
mod pan115_session;
mod providers;
mod quark;
mod session;
mod settings;
mod store;
mod task_removal;
mod tray;
mod upload;
#[cfg(debug_assertions)]
mod verification;

use engine::{Engine, Rpc};
use model::*;
use serde_json::json;
use settings::Settings;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::Duration,
};
use tauri::{Emitter, Manager, State, WebviewWindow};

struct AppState {
    store: Mutex<store::Store>,
    engine: Mutex<Option<Engine>>,
    engine_error: Mutex<String>,
    accounts: Mutex<Vec<Account>>,
    resolving: tokio::sync::Mutex<()>,
    uploads: upload::UploadManager,
    session_cache_error: Mutex<String>,
    exiting: AtomicBool,
    bt_jobs: download_commands::Jobs,
}

fn trusted(window: &WebviewWindow) -> Result<(), String> {
    if window.label() != "main" {
        return Err("此窗口无权执行本地操作".into());
    }
    Ok(())
}

fn rpc(state: &AppState) -> Result<Rpc, String> {
    state
        .engine
        .lock()
        .unwrap()
        .as_ref()
        .map(|e| e.rpc.clone())
        .ok_or_else(|| state.engine_error.lock().unwrap().clone())
}

#[tauri::command]
async fn open_login(
    window: WebviewWindow,
    app: tauri::AppHandle,
    provider: Provider,
) -> Result<(), String> {
    trusted(&window)?;
    session::window(&app, provider, true)?;
    Ok(())
}

#[tauri::command]
async fn refresh_account(
    provider: Provider,
    window: WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Account, String> {
    trusted(&window)?;
    let result = match session::connect(&app, provider).await {
        Ok(session) => session.account().await,
        Err(error) => Err(error),
    };
    let account = match result {
        Ok(name) => Account {
            provider,
            logged_in: true,
            name,
            message: if provider == Provider::Pan115 {
                let warning = state.session_cache_error.lock().unwrap().clone();
                if warning.is_empty() {
                    "账号已连接".into()
                } else {
                    format!("账号已连接；{warning}")
                }
            } else {
                "账号已连接".into()
            },
        },
        Err(message) => Account {
            provider,
            message,
            ..Account::default()
        },
    };
    if let Some(current) = state
        .accounts
        .lock()
        .unwrap()
        .iter_mut()
        .find(|a| a.provider == provider)
    {
        *current = account.clone();
    }
    Ok(account)
}

#[tauri::command]
async fn logout_account(
    window: WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    provider: Provider,
) -> Result<Account, String> {
    trusted(&window)?;
    session::forget(&app, provider)?;
    let account = Account {
        provider,
        ..Account::default()
    };
    if let Some(current) = state
        .accounts
        .lock()
        .unwrap()
        .iter_mut()
        .find(|a| a.provider == provider)
    {
        *current = account.clone();
    }
    Ok(account)
}

#[tauri::command]
async fn parse_link(
    window: WebviewWindow,
    app: tauri::AppHandle,
    text: String,
    code: String,
    parent_id: Option<String>,
) -> Result<ShareListing, String> {
    trusted(&window)?;
    let share = providers::parse_share(&text, &code)?;
    session::connect(&app, share.provider)
        .await?
        .list(&share, parent_id.as_deref().unwrap_or("0"))
        .await
}

#[tauri::command]
fn snapshot(window: WebviewWindow, state: State<'_, AppState>) -> Result<Snapshot, String> {
    trusted(&window)?;
    let store = state.store.lock().unwrap();
    let engine_running = state
        .engine
        .lock()
        .unwrap()
        .as_mut()
        .is_some_and(Engine::alive);
    let settings = Settings::load(&store);
    Ok(Snapshot {
        accounts: state.accounts.lock().unwrap().clone(),
        tasks: store.tasks()?,
        download_dir: settings.download_dir.clone(),
        engine_running,
        engine_error: state.engine_error.lock().unwrap().clone(),
        uploads: store.uploads()?.into_iter().map(Into::into).collect(),
        shares: store.shares()?,
        settings,
    })
}

#[tauri::command]
fn set_directory(
    window: WebviewWindow,
    state: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    trusted(&window)?;
    let directory = PathBuf::from(&path);
    if !directory.is_absolute() || !directory.is_dir() {
        return Err("请选择已存在的本地文件夹".into());
    }
    state
        .store
        .lock()
        .unwrap()
        .set_setting("download_dir", &path)
}

#[tauri::command]
fn settings_get(window: WebviewWindow, state: State<'_, AppState>) -> Result<Settings, String> {
    trusted(&window)?;
    Ok(Settings::load(&state.store.lock().unwrap()))
}

/// 保存偏好设置，并立即把限速、同时下载数同步给下载引擎。
#[tauri::command]
async fn settings_set(
    window: WebviewWindow,
    state: State<'_, AppState>,
    settings: Settings,
) -> Result<Settings, String> {
    trusted(&window)?;
    let saved = {
        let store = state.store.lock().unwrap();
        crate::settings::save(&store, &settings)?
    };
    if let Ok(rpc) = rpc(&state) {
        let _ = rpc.change_global_options(&saved.aria2_options()).await;
    }
    // 上传并发变更后立即生效（进行中的任务不受影响）。
    state.uploads.set_limits(
        saved.upload_concurrency as usize,
        (saved.upload_concurrency as usize * saved.upload_part_concurrency as usize).max(1),
    );
    Ok(saved)
}

async fn start_task(
    app: &tauri::AppHandle,
    state: &AppState,
    mut task: Task,
    prepared_direct: Option<direct::Metadata>,
    prepared_session: Option<&crate::providers::Session>,
) -> Result<Task, String> {
    task.status = "resolving".into();
    task.error.clear();
    task.speed = 0;
    task.upload_speed = 0;
    let previous_gid = task.gid.clone();
    state.store.lock().unwrap().save(&task)?;
    let (cloud_connections, cloud_min_split) = {
        let store = state.store.lock().unwrap();
        let settings = Settings::load(&store);
        (settings.default_connections, settings.min_split_mib)
    };
    let result = async {
        std::fs::create_dir_all(&task.directory)
            .map_err(|_| "无法创建下载目录，请选择可写的文件夹")?;
        if let Some(source) = &task.bt {
            let base = app.path().app_data_dir().map_err(|_| "无法定位种子缓存")?;
            let (torrent, data) = bt::load(&base, &source.info_hash)?;
            if torrent.name != task.name
                || torrent.files != source.files
                || torrent.piece_length != source.piece_length
            {
                return Err("种子缓存与任务文件清单不一致，请重新导入种子".into());
            }
            return rpc(state)?.add_bt(&data, &task).await;
        }
        if let Some(source) = &mut task.direct {
            let metadata = match prepared_direct {
                Some(metadata) => metadata,
                None => direct::probe(&source.url).await?,
            };
            let partial = task.completed > 0
                || PathBuf::from(&task.directory)
                    .join(&task.name)
                    .metadata()
                    .is_ok_and(|m| m.len() > 0);
            direct::update_source(source, task.size, partial, &metadata)?;
            task.size = metadata.size;
            task.file.size = metadata.size;
            return rpc(state)?
                .add_direct(source, &task.directory, &task.name)
                .await;
        }
        let owned_session;
        let session = match prepared_session {
            Some(session) => session,
            None => {
                owned_session = session::connect(app, task.share.provider).await?;
                &owned_session
            }
        };
        if task.own.is_some() || !task.share.provider.anonymous_share() {
            session.account().await?;
        }
        let download = if let Some(source) = &task.own {
            if task.share.provider == Provider::Baidu {
                session.drive_download(source).await?
            } else {
                session.cloud_download(source).await?
            }
        } else {
            let listing = session.list(&task.share, &task.file.parent_id).await?;
            let fresh = listing
                .files
                .into_iter()
                .find(|f| f.id == task.file.id)
                .ok_or("分享中的文件已经被移除")?;
            if task.completed > 0 && fresh.size != task.file.size {
                return Err("源文件大小已变化，请重新解析分享并创建新任务".into());
            }
            session
                .download(&task.share, &fresh, &listing.stoken)
                .await?
        };
        if let Some(size) = download.size {
            if task.completed > 0 && task.size > 0 && size != task.size {
                return Err("源文件大小已变化，请重新创建下载任务".into());
            }
            task.size = size;
        }
        let gid = rpc(state)?
            .add_cloud(
                &download.url,
                &task.directory,
                &task.name,
                &download.headers,
                &download.user_agent,
                cloud_connections,
                cloud_min_split,
            )
            .await?;
        Ok::<String, String>(gid)
    }
    .await;
    // A pause, a removal, or a second resolve attempt can land while the URL is
    // being fetched. Commit under the store lock so the newest user intent wins
    // instead of being overwritten by this late result (pause flips the record
    // under the same lock before it talks to aria2).
    let outcome = {
        let store = state.store.lock().unwrap();
        let stored = store
            .tasks()
            .ok()
            .and_then(|all| all.into_iter().find(|t| t.id == task.id));
        let superseded = stored
            .as_ref()
            .is_some_and(|current| current.gid != previous_gid);
        let paused = stored
            .as_ref()
            .is_some_and(|current| current.status == "paused");
        if stored.is_none() || superseded {
            None
        } else {
            match &result {
                Ok(gid) => {
                    task.gid = Some(gid.clone());
                    if paused {
                        task.status = "paused".into();
                        task.speed = 0;
                        task.upload_speed = 0;
                        task.connections = 0;
                    } else {
                        task.status = "waiting".into();
                    }
                }
                Err(error) => {
                    if paused {
                        task.status = "paused".into();
                    } else {
                        task.status = "error".into();
                        task.error = error.clone();
                    }
                }
            }
            store.save(&task)?;
            Some(paused)
        }
    };
    let Some(paused) = outcome else {
        if let Ok(gid) = &result {
            if let Ok(rpc) = rpc(state) {
                let _ = rpc.stop_for_removal(gid).await;
            }
        }
        return Ok(task);
    };
    if paused {
        // The user paused while the URL was resolving: keep the engine job (so
        // resume can continue it) but park it immediately.
        if let Ok(gid) = &result {
            if let Ok(rpc) = rpc(state) {
                let _ = rpc.call("aria2.forcePause", vec![json!(gid.clone())]).await;
            }
        }
    }
    let _ = app.emit_to("main", "tasks-changed", ());
    Ok(task)
}

/// Resolves freshly queued tasks off the UI thread. The tasks are already
/// persisted as "resolving" before this runs, so the list shows them
/// immediately instead of blocking the download button for every folder item.
///
/// Tasks of the same provider share one hidden WebView session, and a few files
/// resolve at a time: creating a session per file left a 148-file folder sitting
/// in "获取地址" for minutes before the first byte moved.
const RESOLVE_CONCURRENCY: usize = 3;

fn resolve_tasks_in_background(app: tauri::AppHandle, tasks: Vec<Task>) {
    if tasks.is_empty() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let mut groups: Vec<(Provider, Vec<Task>)> = Vec::new();
        for task in tasks {
            match groups
                .iter_mut()
                .find(|(provider, _)| *provider == task.share.provider)
            {
                Some((_, list)) => list.push(task),
                None => groups.push((task.share.provider, vec![task])),
            }
        }
        for (provider, list) in groups {
            let state = app.state::<AppState>();
            let session = match session::connect(&app, provider).await {
                Ok(session) => session,
                Err(error) => {
                    for mut task in list {
                        task.status = "error".into();
                        task.error = error.clone();
                        let _ = state.store.lock().unwrap().save(&task);
                    }
                    let _ = app.emit_to("main", "tasks-changed", ());
                    continue;
                }
            };
            let session = &session;
            for chunk in list.chunks(RESOLVE_CONCURRENCY) {
                let resolving = chunk.iter().map(|queued| {
                    let app = app.clone();
                    let id = queued.id.clone();
                    async move {
                        let state = app.state::<AppState>();
                        // Reload: the task may have been paused or removed while
                        // it waited in the queue.
                        let current = {
                            let store = state.store.lock().unwrap();
                            store
                                .tasks()
                                .ok()
                                .and_then(|all| all.into_iter().find(|t| t.id == id))
                        };
                        let Some(current) = current else { return };
                        if current.status != "resolving" {
                            return;
                        }
                        let _ = start_task(&app, &state, current, None, Some(session)).await;
                    }
                });
                futures_util::future::join_all(resolving).await;
            }
        }
    });
}

#[tauri::command]
async fn enqueue(
    window: WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    share: ShareLink,
    parent_id: String,
    ids: Vec<String>,
) -> Result<Vec<Task>, String> {
    trusted(&window)?;
    let _guard = state.resolving.lock().await;
    let session = session::connect(&app, share.provider).await?;
    if !share.provider.anonymous_share() {
        let name = session.account().await?;
        if let Some(current) = state
            .accounts
            .lock()
            .unwrap()
            .iter_mut()
            .find(|a| a.provider == share.provider)
        {
            *current = Account {
                provider: share.provider,
                logged_in: true,
                name,
                message: "账号已连接".into(),
            };
        }
    }
    if ids.is_empty() {
        return Err("请先选择文件".into());
    }
    let directory = state
        .store
        .lock()
        .unwrap()
        .setting("download_dir")
        .ok_or("请先选择下载目录")?;
    let listing = session.list(&share, &parent_id).await?;
    let selected: Vec<_> = listing
        .files
        .into_iter()
        .filter(|f| ids.contains(&f.id))
        .collect();
    if selected.len() != ids.len() {
        return Err("文件列表已变化，请重新解析分享".into());
    }
    let scope = format!("share:{}:{}", share.provider.key(), share.id);
    let mut pending: Vec<(ShareFile, PathBuf, Option<TaskBatch>)> = selected
        .into_iter()
        .map(|file| {
            let group = file.is_dir.then(|| {
                folder_batch(&scope, &file.id, &file.name, "", &PathBuf::from(&directory))
            });
            (file, PathBuf::from(&directory), group)
        })
        .collect();
    let mut files = Vec::new();
    let mut visited = HashSet::new();
    while let Some((file, dir, group)) = pending.pop() {
        if !visited.insert(file.id.clone()) {
            continue;
        }
        if visited.len() > 10000 {
            return Err("所选文件过多，请分批下载".into());
        }
        if file.is_dir {
            let listing = session.list(&share, &file.id).await?;
            let subdir = dir.join(safe_name(&file.name));
            pending.extend(
                listing
                    .files
                    .into_iter()
                    .map(|f| (f, subdir.clone(), group.clone())),
            );
        } else {
            files.push((file, dir, group));
        }
    }
    let mut created = Vec::new();
    for (file, dir, group) in files {
        let existing = state.store.lock().unwrap().tasks()?;
        let duplicate = existing.iter().position(|t| {
            t.share.provider == share.provider
                && t.share.id == share.id
                && t.file.id == file.id
                && t.directory == dir.to_string_lossy()
                && !matches!(t.status.as_str(), "error" | "cancelled")
        });
        if let Some(index) = duplicate {
            if let Some(group) = &group {
                let mut task = existing[index].clone();
                if task.batch.is_none() {
                    task.batch = Some(group.clone());
                    state.store.lock().unwrap().save(&task)?;
                }
            }
            continue;
        }
        let base_name = safe_name(&file.name);
        let mut name = base_name.clone();
        let mut number = 1;
        while dir.join(&name).exists()
            || existing.iter().any(|t| {
                t.directory.eq_ignore_ascii_case(&dir.to_string_lossy())
                    && t.name.eq_ignore_ascii_case(&name)
            })
        {
            let p = PathBuf::from(&base_name);
            let stem = p.file_stem().unwrap_or_default().to_string_lossy();
            let ext = p
                .extension()
                .map(|v| format!(".{}", v.to_string_lossy()))
                .unwrap_or_default();
            name = format!("{stem} ({number}){ext}");
            number += 1;
        }
        let task = Task {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            size: file.size,
            completed: 0,
            speed: 0,
            status: "resolving".into(),
            error: String::new(),
            directory: dir.to_string_lossy().into(),
            created_at: now(),
            gid: None,
            share: share.clone(),
            file,
            own: None,
            direct: None,
            connections: 0,
            bt: None,
            upload_speed: 0,
            batch: group,
        };
        created.push(start_task(&app, &state, task, None, Some(&session)).await?);
    }
    if created.is_empty() {
        return Err("所选文件已在下载列表中，或文件夹为空".into());
    }
    Ok(created)
}

/// Applies one action to a single task. Callers hold `state.resolving` so the
/// action cannot interleave with URL resolution.
async fn apply_task_action(
    app: &tauri::AppHandle,
    state: &AppState,
    mut task: Task,
    action: &str,
    delete_files: bool,
    prepared_session: Option<&crate::providers::Session>,
) -> Result<(), String> {
    match action {
        "pause" => {
            // Flip the record first, under the store lock and without awaiting:
            // a resolve that is finishing right now then sees "paused" and parks
            // its own engine job instead of overwriting this state.
            let gid = {
                let store = state.store.lock().unwrap();
                let mut current = store
                    .tasks()?
                    .into_iter()
                    .find(|t| t.id == task.id)
                    .unwrap_or_else(|| task.clone());
                current.status = "paused".into();
                current.speed = 0;
                current.upload_speed = 0;
                current.connections = 0;
                let gid = current.gid.clone();
                store.save(&current)?;
                gid
            };
            if let Some(gid) = gid {
                // aria2 answers "cannot be paused now" when the job is already
                // paused or just finished; a repeated click must not surface an
                // error banner.
                if let Err(error) = rpc(state)?.call("aria2.forcePause", vec![json!(gid)]).await {
                    if !error.contains("cannot be paused") {
                        return Err(error);
                    }
                }
            }
        }
        "resume" => {
            if !matches!(task.status.as_str(), "paused" | "error" | "cancelled") {
                return Err("任务当前不能恢复".into());
            }
            // A paused download is still registered in aria2. Unpausing keeps
            // its partial data and reported progress, so the bar does not flash
            // back to zero while the engine re-reads the control file. Tasks the
            // engine no longer knows (app restart, expired URL, failed task)
            // fall through to a fresh resolve below.
            if let Some(gid) = task.gid.clone() {
                let rpc = rpc(state)?;
                if rpc.call("aria2.unpause", vec![json!(gid)]).await.is_ok() {
                    task.status = "waiting".into();
                    task.speed = 0;
                    task.upload_speed = 0;
                    state.store.lock().unwrap().save(&task)?;
                    let _ = app.emit_to("main", "tasks-changed", ());
                    return Ok(());
                }
                let _ = rpc.stop_for_removal(&gid).await;
            }
            task.gid = None;
            start_task(app, state, task, None, prepared_session).await?;
        }
        "remove" => {
            if let Some(gid) = &task.gid {
                rpc(state)?.stop_for_removal(gid).await?;
            }
            task.gid = None;
            task.speed = 0;
            task.upload_speed = 0;
            task.connections = 0;
            if task.status != "complete" {
                task.status = "paused".into();
            }
            let store = state.store.lock().unwrap();
            // Keep a stopped, retryable record if filesystem cleanup fails.
            store.save(&task)?;
            task_removal::remove_record(&store, &task, delete_files)?;
        }
        _ => return Err("未知任务操作".into()),
    }
    Ok(())
}

#[tauri::command]
async fn task_action(
    window: WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
    action: String,
    delete_files: Option<bool>,
) -> Result<(), String> {
    trusted(&window)?;
    let _guard = state.resolving.lock().await;
    let task = state
        .store
        .lock()
        .unwrap()
        .tasks()?
        .into_iter()
        .find(|t| t.id == id)
        .ok_or("下载任务不存在")?;
    apply_task_action(
        &app,
        &state,
        task,
        &action,
        delete_files.unwrap_or(false),
        None,
    )
    .await
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchActionReport {
    done: usize,
    failed: Vec<BatchActionFailure>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BatchActionFailure {
    id: String,
    message: String,
}

/// Runs one action across a selection or a whole folder group. Actions that do
/// not apply to a task (for example pausing an already finished file) are
/// skipped instead of failing the batch.
#[tauri::command]
async fn task_batch_action(
    window: WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    ids: Vec<String>,
    action: String,
    delete_files: Option<bool>,
) -> Result<BatchActionReport, String> {
    trusted(&window)?;
    if ids.is_empty() {
        return Err("请先选择下载任务".into());
    }
    if ids.len() > 10000 {
        return Err("一次最多操作 10000 个任务".into());
    }
    if !matches!(action.as_str(), "pause" | "resume" | "remove") {
        return Err("未知任务操作".into());
    }
    let _guard = state.resolving.lock().await;
    let all = state.store.lock().unwrap().tasks()?;
    let mut report = BatchActionReport {
        done: 0,
        failed: Vec::new(),
    };
    let mut seen = HashSet::new();
    // Resuming a whole folder re-resolves every file; one session per provider
    // keeps that from opening a hidden WebView per task.
    let mut sessions: HashMap<Provider, crate::providers::Session> = HashMap::new();
    for id in ids {
        if !seen.insert(id.clone()) {
            continue;
        }
        let Some(task) = all.iter().find(|t| t.id == id).cloned() else {
            report.failed.push(BatchActionFailure {
                id,
                message: "下载任务不存在".into(),
            });
            continue;
        };
        let applies = match action.as_str() {
            "pause" => matches!(
                task.status.as_str(),
                "active" | "waiting" | "seeding" | "resolving"
            ),
            "resume" => matches!(task.status.as_str(), "paused" | "error" | "cancelled"),
            _ => true,
        };
        if !applies {
            continue;
        }
        if action == "resume" && !sessions.contains_key(&task.share.provider) {
            match session::connect(&app, task.share.provider).await {
                Ok(session) => {
                    sessions.insert(task.share.provider, session);
                }
                Err(error) => {
                    report
                        .failed
                        .push(BatchActionFailure { id, message: error });
                    continue;
                }
            }
        }
        let prepared = sessions.get(&task.share.provider);
        match apply_task_action(
            &app,
            &state,
            task,
            &action,
            delete_files.unwrap_or(false),
            prepared,
        )
        .await
        {
            Ok(()) => report.done += 1,
            Err(message) => report.failed.push(BatchActionFailure { id, message }),
        }
    }
    Ok(report)
}

#[tauri::command]
fn open_directory(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: Option<String>,
) -> Result<(), String> {
    trusted(&window)?;
    let store = state.store.lock().unwrap();
    let path = if let Some(id) = id {
        store
            .tasks()?
            .into_iter()
            .find(|t| t.id == id)
            .map(|t| t.directory)
            .ok_or("任务不存在")?
    } else {
        store.setting("download_dir").ok_or("未设置下载目录")?
    };
    tauri_plugin_opener::open_path(path, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn open_data_directory(window: WebviewWindow, app: tauri::AppHandle) -> Result<(), String> {
    trusted(&window)?;
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "无法定位程序数据目录")?;
    tauri_plugin_opener::open_path(dir, None::<&str>).map_err(|e| e.to_string())
}

async fn poll_engine(app: tauri::AppHandle) {
    loop {
        tokio::time::sleep(Duration::from_millis(1000)).await;
        let state = app.state::<AppState>();
        let Ok(rpc) = rpc(&state) else {
            continue;
        };
        download_commands::expire_jobs(&state, &rpc).await;
        let tasks = state.store.lock().unwrap().tasks().unwrap_or_default();
        for mut task in tasks {
            if !matches!(
                task.status.as_str(),
                "active" | "waiting" | "paused" | "seeding"
            ) {
                continue;
            }
            let Some(gid) = &task.gid else {
                continue;
            };
            let before_status = task.status.clone();
            let Ok(value) = rpc
                .call(
                    "aria2.tellStatus",
                    vec![
                        json!(gid),
                        json!([
                            "status",
                            "totalLength",
                            "completedLength",
                            "downloadSpeed",
                            "connections",
                            "errorCode",
                            "errorMessage",
                            "files",
                            "numPeers",
                            "uploadSpeed",
                            "seeder",
                            "bitfield"
                        ]),
                    ],
                )
                .await
            else {
                continue;
            };
            let number = |key: &str| {
                value[key]
                    .as_str()
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0)
            };
            task.status = value["status"].as_str().unwrap_or("waiting").into();
            let reported = number("completedLength");
            // aria2 answers 0 for a moment when a download is (re)added before
            // it has loaded the .aria2 control file. Keeping the stored value
            // avoids an empty progress bar on resume; real progress takes over
            // as soon as aria2 reports a non-zero length.
            if reported > 0 || task.completed == 0 {
                task.completed = reported;
            }
            task.speed = if task.status == "active" {
                number("downloadSpeed")
            } else {
                0
            };
            task.connections = number("connections") as u32;
            if number("totalLength") > 0 {
                task.size = number("totalLength");
            }
            if let Some(source) = &task.bt {
                let (size, completed) = bt_progress(source, &value);
                task.size = size;
                task.completed = completed;
                task.connections = number("numPeers") as u32;
                task.upload_speed = if task.status == "active" {
                    number("uploadSpeed")
                } else {
                    0
                };
                if task.status == "active" && source.seed_minutes > 0 && task.completed == task.size
                {
                    task.status = "seeding".into();
                    task.speed = 0;
                }
            }
            if task.status == "error" {
                let code = value["errorCode"].as_str().unwrap_or("未知");
                task.error = download_errors::message(
                    code,
                    value["errorMessage"].as_str().unwrap_or_default(),
                    task.direct.is_some(),
                );
            }
            let store = state.store.lock().unwrap();
            if store
                .tasks()
                .unwrap_or_default()
                .iter()
                .any(|t| t.id == task.id && t.gid == task.gid && t.status == before_status)
            {
                let _ = store.save(&task);
            }
        }
        let _ = app.emit_to("main", "tasks-changed", ());
    }
}

fn bt_progress(source: &BtSource, value: &serde_json::Value) -> (u64, u64) {
    let selected: HashSet<_> = source.selected.iter().copied().collect();
    let size = source
        .files
        .iter()
        .filter(|f| selected.contains(&f.index))
        .map(|f| f.size)
        .sum();
    // aria2 1.37 undercounts per-file completedLength for selected multi-file
    // torrents. Intersect verified pieces with each selected byte range instead.
    let bitfield = value["bitfield"].as_str().unwrap_or_default();
    let verified = |piece: u64| -> bool {
        let Some(hex) = bitfield.as_bytes().get((piece / 4) as usize) else {
            return false;
        };
        let Some(nibble) = (*hex as char).to_digit(16) else {
            return false;
        };
        nibble & (8 >> (piece % 4)) != 0
    };
    let mut completed = 0;
    let mut offset = 0;
    for file in &source.files {
        let end = offset + file.size;
        // piece_length 为 0 表示种子缺少分片信息，此时无法按分片统计。
        let Some(first) = offset.checked_div(source.piece_length) else {
            break;
        };
        if file.size > 0 && selected.contains(&file.index) {
            for piece in first..end.div_ceil(source.piece_length) {
                if verified(piece) {
                    completed += end.min((piece + 1) * source.piece_length)
                        - offset.max(piece * source.piece_length);
                }
            }
        }
        offset = end;
    }
    (size, completed)
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            tray::restore(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            tray::setup(app)?;
            let dir = app.path().app_data_dir()?;
            migration::migrate(&dir)?;
            std::fs::create_dir_all(&dir)?;
            let store = store::Store::open(&dir.join("tasks.sqlite3"))?;
            if store.setting("download_dir").is_none() {
                let downloads = app
                    .path()
                    .download_dir()
                    .unwrap_or(dir.clone())
                    .join("CloudDownloader");
                std::fs::create_dir_all(&downloads)?;
                store.set_setting("download_dir", &downloads.to_string_lossy())?;
            }
            for mut task in store.tasks()? {
                if matches!(
                    task.status.as_str(),
                    "active" | "waiting" | "resolving" | "seeding"
                ) {
                    task.status = "paused".into();
                }
                task.gid = None;
                task.speed = 0;
                task.upload_speed = 0;
                task.connections = 0;
                store.save(&task)?;
            }
            for mut upload in store.uploads()? {
                if !matches!(upload.status.as_str(), "complete" | "error" | "paused") {
                    upload.status = "paused".into();
                }
                upload.speed = 0;
                store.save_upload(&upload)?;
            }
            let preferences = Settings::load(&store);
            let (engine, engine_error) = match Engine::start_with(&preferences) {
                Ok(v) => (Some(v), String::new()),
                Err(e) => (None, e),
            };
            app.manage(AppState {
                store: Mutex::new(store),
                engine: Mutex::new(engine),
                engine_error: Mutex::new(engine_error),
                accounts: Mutex::new(
                    Provider::ALL
                        .into_iter()
                        .map(|provider| Account {
                            provider,
                            ..Account::default()
                        })
                        .collect(),
                ),
                resolving: tokio::sync::Mutex::new(()),
                uploads: upload::UploadManager::new(
                    preferences.upload_concurrency as usize,
                    (preferences.upload_concurrency as usize
                        * preferences.upload_part_concurrency as usize)
                        .max(1),
                ),
                session_cache_error: Mutex::new(String::new()),
                exiting: AtomicBool::new(false),
                bt_jobs: Default::default(),
            });
            if let Some(main) = app.get_webview_window("main") {
                let handle = app.handle().clone();
                main.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let exit = handle
                            .state::<AppState>()
                            .store
                            .lock()
                            .unwrap()
                            .setting("close_action")
                            .as_deref()
                            == Some("exit");
                        if exit {
                            handle.exit(0);
                        } else if let Some(window) = handle.get_webview_window("main") {
                            let _ = window.hide();
                        }
                    }
                });
            }
            tauri::async_runtime::spawn(poll_engine(app.handle().clone()));
            #[cfg(debug_assertions)]
            if let Some(mode) = std::env::args().find(|a| {
                a.starts_with("--verify-baidu-")
                    || a == "--verify-cloud-diagnostics"
                    || matches!(
                        a.as_str(),
                        "--verify-pan115-cache-seed"
                            | "--verify-pan115-cache-restore"
                            | "--verify-pan115-session"
                    )
            }) {
                verification::start(app.handle().clone(), mode);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_login,
            refresh_account,
            logout_account,
            parse_link,
            snapshot,
            set_directory,
            settings_get,
            settings_set,
            enqueue,
            direct_commands::enqueue_direct,
            download_commands::inspect_download,
            download_commands::poll_bt_inspection,
            download_commands::cancel_bt_inspection,
            download_commands::enqueue_bt,
            task_action,
            task_batch_action,
            open_directory,
            open_data_directory,
            drive_commands::drive_list,
            cloud_commands::cloud_list,
            cloud_commands::dir_list_all,
            cloud_commands::cloud_search,
            cloud_commands::cloud_enqueue,
            drive_commands::drive_enqueue,
            drive_commands::drive_mkdir,
            drive_commands::drive_mkdir_at,
            drive_commands::drive_rename_at,
            drive_commands::drive_delete_at,
            drive_commands::drive_share,
            drive_commands::drive_cancel_share,
            drive_commands::upload_enqueue,
            drive_commands::upload_action,
            drive_commands::upload_batch_action,
            drive_commands::collect_local_files
        ])
        .build(tauri::generate_context!())
        .expect("程序初始化失败");
    app.run(|app, event| {
        if let tauri::RunEvent::ExitRequested { api, code, .. } = &event {
            if let Some(state) = app.try_state::<AppState>() {
                if !state.exiting.swap(true, Ordering::SeqCst) {
                    api.prevent_exit();
                    let app = app.clone();
                    let code = code.unwrap_or(0);
                    tauri::async_runtime::spawn(async move {
                        let save_app = app.clone();
                        let save = tauri::async_runtime::spawn_blocking(move || {
                            session::save_before_exit(&save_app)
                        });
                        let _ = tokio::time::timeout(Duration::from_secs(3), save).await;
                        app.exit(code);
                    });
                }
            }
        }
        if let tauri::RunEvent::Exit = event {
            if let Some(state) = app.try_state::<AppState>() {
                state.engine.lock().unwrap().take();
            }
        }
    });
}
