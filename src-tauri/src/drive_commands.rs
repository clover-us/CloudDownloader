use crate::{
    model::*,
    providers::{baidu_drive::remote_path, cloud},
    resolve_tasks_in_background, session, trusted,
    upload::{self, UploadRecord, UploadView},
    AppState,
};
use std::{collections::HashSet, path::PathBuf};
use tauri::{State, WebviewWindow};

#[tauri::command]
pub async fn drive_list(
    window: WebviewWindow,
    app: tauri::AppHandle,
    path: String,
    page: u32,
    keyword: String,
) -> Result<DriveListing, String> {
    trusted(&window)?;
    session::connect(&app, Provider::Baidu)
        .await?
        .drive_list(&path, page, &keyword)
        .await
}

#[tauri::command]
pub async fn drive_mkdir(
    window: WebviewWindow,
    app: tauri::AppHandle,
    account_id: String,
    parent: String,
    name: String,
) -> Result<DriveFile, String> {
    trusted(&window)?;
    let path = child_path(&parent, &name)?;
    let session = session::connect(&app, Provider::Baidu).await?;
    let identity = session.baidu_identity(Some(&account_id)).await?;
    session.drive_mkdir(&identity, &path).await
}

/// 在任意受支持网盘的目标路径下新建一级文件夹（百度按路径名，迅雷按父目录 id）。
#[tauri::command]
pub async fn drive_mkdir_at(
    window: WebviewWindow,
    app: tauri::AppHandle,
    provider: Provider,
    account_id: String,
    parent: String,
    name: String,
) -> Result<DriveFile, String> {
    trusted(&window)?;
    child_path(&parent, &name)?;
    let session = session::connect(&app, provider).await?;
    match provider {
        Provider::Baidu => {
            let path = child_path(&parent, &name)?;
            let identity = session.baidu_identity(Some(&account_id)).await?;
            session.drive_mkdir(&identity, &path).await
        }
        Provider::Xunlei => {
            let identity = session.cloud_identity().await?;
            if identity.0 != account_id {
                return Err("网盘账号已切换，请重新选择目录".into());
            }
            remote_path(&parent)?;
            let parent_id = if parent.trim_end_matches('/').is_empty() {
                "0".to_string()
            } else {
                cloud::parent_id(&parent, provider)?
            };
            let mut file = session.xunlei_mkdir(&parent_id, &name).await?;
            file.path = format!("{}/{}", parent.trim_end_matches('/'), file.id);
            Ok(file)
        }
        Provider::Quark => {
            let identity = session.cloud_identity().await?;
            if identity.0 != account_id {
                return Err("网盘账号已切换，请重新选择目录".into());
            }
            session.cloud_mkdir(&parent, &name).await
        }
        _ => Err("该网盘暂不支持新建文件夹".into()),
    }
}

fn child_path(parent: &str, name: &str) -> Result<String, String> {
    remote_path(parent)?;
    if name.trim().is_empty()
        || name != name.trim()
        || name.contains(['/', '\\'])
        || name.chars().any(char::is_control)
        || matches!(name, "." | "..")
    {
        return Err("文件夹或文件名无效".into());
    }
    let path = format!("{}/{}", parent.trim_end_matches('/'), name);
    remote_path(&path)?;
    Ok(path)
}

/// 拆分为 (主名, 扩展名)，用于同名文件自动追加序号。
fn split_name(name: &str) -> (String, String) {
    match name.rfind('.') {
        Some(idx) if idx > 0 && idx < name.len() - 1 => {
            (name[..idx].to_string(), name[idx..].to_string())
        }
        _ => (name.to_string(), String::new()),
    }
}

/// 取路径的父目录（"a/b/c" -> "a/b"，"c" 归一化为 "/"）。
fn parent_dir(path: &str) -> String {
    match path.rsplit_once('/') {
        Some((head, _)) if !head.is_empty() => head.to_string(),
        _ => "/".to_string(),
    }
}

/// 判断任务路径与给定目录是否属于同一目录（忽略首尾多余斜杠）。
fn same_directory(path: &str, directory: &str) -> bool {
    let normalize = |value: &str| {
        let trimmed = value.trim_end_matches('/');
        if trimmed.is_empty() {
            "/".to_string()
        } else {
            trimmed.to_string()
        }
    };
    normalize(&parent_dir(path)) == normalize(directory)
}

fn validate_entry_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty()
        || name != name.trim()
        || name.contains(['/', '\\'])
        || name.chars().any(char::is_control)
        || matches!(name, "." | "..")
    {
        return Err("文件或文件夹名称无效".into());
    }
    Ok(())
}

/// 重命名任意受支持网盘中的文件或文件夹（百度按路径，夸克/迅雷按文件 id）。
#[tauri::command]
pub async fn drive_rename_at(
    window: WebviewWindow,
    app: tauri::AppHandle,
    provider: Provider,
    account_id: String,
    file: DriveFile,
    new_name: String,
) -> Result<DriveFile, String> {
    trusted(&window)?;
    validate_entry_name(&new_name)?;
    let session = session::connect(&app, provider).await?;
    match provider {
        Provider::Baidu => {
            let identity = session.baidu_identity(Some(&account_id)).await?;
            session.drive_rename(&identity, &file, &new_name).await
        }
        Provider::Xunlei => {
            let identity = session.cloud_identity().await?;
            if identity.0 != account_id {
                return Err("网盘账号已切换，请重新选择文件".into());
            }
            session.xunlei_rename(&file, &new_name).await
        }
        Provider::Quark => {
            let identity = session.cloud_identity().await?;
            if identity.0 != account_id {
                return Err("网盘账号已切换，请重新选择文件".into());
            }
            session.cloud_rename(&file, &new_name).await
        }
        _ => Err("该网盘暂不支持重命名".into()),
    }
}

/// 删除任意受支持网盘中的一批文件或文件夹。
#[tauri::command]
pub async fn drive_delete_at(
    window: WebviewWindow,
    app: tauri::AppHandle,
    provider: Provider,
    account_id: String,
    files: Vec<DriveFile>,
) -> Result<(), String> {
    trusted(&window)?;
    if files.is_empty() {
        return Err("请选择要删除的文件".into());
    }
    let session = session::connect(&app, provider).await?;
    match provider {
        Provider::Baidu => {
            let identity = session.baidu_identity(Some(&account_id)).await?;
            session.drive_delete(&identity, &files).await
        }
        Provider::Xunlei => {
            let identity = session.cloud_identity().await?;
            if identity.0 != account_id {
                return Err("网盘账号已切换，请重新选择文件".into());
            }
            session.xunlei_delete(&files).await
        }
        Provider::Quark => {
            let identity = session.cloud_identity().await?;
            if identity.0 != account_id {
                return Err("网盘账号已切换，请重新选择文件".into());
            }
            session.cloud_delete(&files).await
        }
        _ => Err("该网盘暂不支持删除文件".into()),
    }
}

#[tauri::command]
pub async fn drive_enqueue(
    window: WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    account_id: String,
    files: Vec<DriveFile>,
) -> Result<Vec<Task>, String> {
    trusted(&window)?;
    let _guard = state.resolving.lock().await;
    let session = session::connect(&app, Provider::Baidu).await?;
    session.baidu_identity(Some(&account_id)).await?;
    let fresh = session
        .drive_meta(&files.iter().map(|f| f.path.clone()).collect::<Vec<_>>())
        .await?;
    if fresh.len() != files.len()
        || files
            .iter()
            .any(|f| !fresh.iter().any(|g| f.id == g.id && f.path == g.path))
    {
        return Err("文件列表已变化，请刷新后重新选择".into());
    }
    let directory = state
        .store
        .lock()
        .unwrap()
        .setting("download_dir")
        .ok_or("请先选择下载目录")?;
    let mut pending = fresh
        .into_iter()
        .map(|f| (f, PathBuf::from(&directory)))
        .collect::<Vec<_>>();
    let mut visited = HashSet::new();
    let mut downloads = Vec::new();
    while let Some((file, dir)) = pending.pop() {
        if !visited.insert(file.id.clone()) {
            continue;
        }
        if visited.len() > 10000 {
            return Err("所选文件超过 10000 项，请分批下载".into());
        }
        if file.is_dir {
            for page in 1..=10000 {
                let listing = session.drive_list(&file.path, page, "").await?;
                if listing.account_id != account_id {
                    return Err("百度账号已切换，请重新选择文件".into());
                }
                let subdir = dir.join(safe_name(&file.name));
                pending.extend(listing.files.into_iter().map(|f| (f, subdir.clone())));
                if pending.len() + visited.len() > 10000 {
                    return Err("所选文件过多，请分批下载".into());
                }
                if !listing.has_more {
                    break;
                }
            }
        } else {
            downloads.push((file, dir));
        }
    }
    let mut created = Vec::new();
    for (file, dir) in downloads {
        let existing = state.store.lock().unwrap().tasks()?;
        if existing.iter().any(|t| {
            t.own
                .as_ref()
                .is_some_and(|s| s.account_id == account_id && s.file.id == file.id)
                && t.directory == dir.to_string_lossy()
                && !matches!(t.status.as_str(), "error" | "cancelled")
        }) {
            continue;
        }
        let base = safe_name(&file.name);
        let mut name = base.clone();
        let mut n = 1;
        while dir.join(&name).exists()
            || existing.iter().any(|t| {
                t.directory.eq_ignore_ascii_case(&dir.to_string_lossy())
                    && t.name.eq_ignore_ascii_case(&name)
            })
        {
            let p = PathBuf::from(&base);
            name = format!(
                "{} ({n}){}",
                p.file_stem().unwrap_or_default().to_string_lossy(),
                p.extension()
                    .map(|e| format!(".{}", e.to_string_lossy()))
                    .unwrap_or_default()
            );
            n += 1;
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
            share: ShareLink {
                provider: Provider::Baidu,
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
        created.push(task);
    }
    if created.is_empty() {
        return Err("所选文件已在任务列表中，或文件夹为空".into());
    }
    resolve_tasks_in_background(app.clone(), created.clone());
    Ok(created)
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn drive_share(
    window: WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    provider: Provider,
    account_id: String,
    files: Vec<DriveFile>,
    period: u32,
    password: String,
) -> Result<CreatedShare, String> {
    trusted(&window)?;
    let session = session::connect(&app, provider).await?;
    let share = match provider {
        Provider::Baidu => {
            session
                .drive_share(&account_id, &files, period, &password)
                .await?
        }
        Provider::Quark => {
            if session.cloud_identity().await?.0 != account_id {
                return Err("网盘账号已切换，请重新选择文件".into());
            }
            session.cloud_share(&files, period, &password).await?
        }
        Provider::Xunlei => {
            if session.cloud_identity().await?.0 != account_id {
                return Err("网盘账号已切换，请重新选择文件".into());
            }
            session
                .xunlei_create_share(&files, period, &password)
                .await?
        }
        _ => return Err("该网盘暂不支持创建分享".into()),
    };
    state.store.lock().unwrap().save_share(&share)?;
    Ok(share)
}

#[tauri::command]
pub async fn drive_cancel_share(
    window: WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    trusted(&window)?;
    let mut share = state
        .store
        .lock()
        .unwrap()
        .shares()?
        .into_iter()
        .find(|s| s.id == id)
        .ok_or("分享记录不存在")?;
    if !share.cancelled {
        session::connect(&app, Provider::Baidu)
            .await?
            .drive_cancel_share(&share)
            .await?;
        share.cancelled = true;
        state.store.lock().unwrap().save_share(&share)?;
    }
    Ok(())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn upload_enqueue(
    window: WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    provider: Provider,
    account_id: String,
    directory: String,
    paths: Vec<String>,
    concurrency: usize,
) -> Result<Vec<UploadView>, String> {
    trusted(&window)?;
    if paths.is_empty() || paths.len() > 100 || !matches!(concurrency, 1 | 2 | 4 | 8) {
        return Err("请选择 1 至 100 个文件，并使用 1、2、4 或 8 路并发".into());
    }
    remote_path(&directory)?;
    let _guard = state.resolving.lock().await;
    let session = session::connect(&app, provider).await?;
    let parent_id = if provider == Provider::Baidu {
        session.baidu_identity(Some(&account_id)).await?;
        if directory != "/" {
            let meta = session.drive_meta(std::slice::from_ref(&directory)).await?;
            if !meta.iter().any(|f| f.is_dir && f.path == directory) {
                return Err("上传目标目录不存在".into());
            }
        }
        String::new()
    } else if matches!(provider, Provider::Quark | Provider::Xunlei) {
        let identity = session.cloud_identity().await?;
        if identity.0 != account_id {
            return Err("网盘账号已切换，请重新选择上传目录".into());
        }
        let parent = cloud::parent_id(&directory, provider)?;
        if parent == "0" {
            String::new()
        } else {
            parent
        }
    } else {
        return Err("该网盘暂不支持上传".into());
    };
    let existing = state.store.lock().unwrap().uploads()?;
    let mut records = Vec::new();
    for path in paths {
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err("本地文件路径必须为绝对路径".into());
        }
        let (size, modified_ns) = upload::fingerprint(&path)?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("本地文件名无效")?
            .to_string();
        let local_path = path.to_string_lossy().to_string();
        let remote_path = child_path(&directory, &name)?;
        if existing.iter().any(|t| {
            t.account_id == account_id
                && t.provider == provider
                && t.local_path == local_path
                && !matches!(t.status.as_str(), "complete" | "error")
                && same_directory(&t.remote_path, &directory)
        }) {
            return Err(format!("{name} 正在上传中，请勿重复提交"));
        }
        records.push(UploadRecord {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            local_path,
            remote_path,
            account_id: account_id.clone(),
            size,
            modified_ns,
            concurrency,
            status: "waiting".into(),
            error: String::new(),
            completed: 0,
            speed: 0,
            created_at: now(),
            hashes: Vec::new(),
            done: Vec::new(),
            upload_id: String::new(),
            remote_file: None,
            confirmed_requests: 0,
            provider,
            parent_id: parent_id.clone(),
            part_size: 0,
            part_etags: Vec::new(),
            session: serde_json::Value::Null,
        });
    }
    // 目标目录已有的文件名集合，用于同名自动追加序号。
    let mut taken: HashSet<String> = HashSet::new();
    match provider {
        Provider::Baidu => {
            for page in 1..=200 {
                let listing = session.drive_list(&directory, page, "").await?;
                for file in &listing.files {
                    taken.insert(file.name.clone());
                }
                if !listing.has_more {
                    break;
                }
                if page == 200 {
                    return Err("目标目录超过 20000 项，请选择较小的目录上传".into());
                }
            }
        }
        Provider::Quark | Provider::Xunlei => {
            let mut cursor: Option<String> = None;
            for page in 1..=200u32 {
                let batch = session
                    .cloud_page(&directory, page, cursor.as_deref())
                    .await?;
                for entry in &batch.entries {
                    taken.insert(entry.file.name.clone());
                }
                cursor = batch.next;
                if cursor.is_none() {
                    break;
                }
                if page == 200 {
                    return Err("目标目录超过 20000 项，请选择较小的目录上传".into());
                }
            }
        }
        _ => {}
    }
    // 目标目录中「已排队但尚未完成」的任务也要计入已占用名称，
    // 否则两批并发上传各自看不到对方，会算出相同的序号（(1)/(1)）。
    for task in &existing {
        if task.account_id == account_id
            && task.provider == provider
            && !matches!(task.status.as_str(), "complete" | "error")
            && same_directory(&task.remote_path, &directory)
        {
            taken.insert(task.name.clone());
        }
    }
    // 同名自动重命名：a.txt -> a (1).txt -> a (2).txt ...
    for record in records.iter_mut() {
        let original = record.name.clone();
        if taken.contains(&original) {
            let (stem, ext) = split_name(&original);
            let mut n = 1u32;
            let candidate = loop {
                let candidate = format!("{stem} ({n}){ext}");
                if !taken.contains(&candidate) {
                    break candidate;
                }
                n += 1;
                if n > 10_000 {
                    return Err(format!("{original} 同名文件过多，请修改文件名"));
                }
            };
            record.name = candidate.clone();
            record.remote_path = child_path(&directory, &candidate)?;
        }
        taken.insert(record.name.clone());
    }
    for task in &records {
        state.store.lock().unwrap().save_upload(task)?;
        state.uploads.start(app.clone(), task.clone())?;
    }
    Ok(records.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn upload_action(
    window: WebviewWindow,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
    action: String,
) -> Result<(), String> {
    trusted(&window)?;
    let _guard = state.resolving.lock().await;
    let mut task = state
        .store
        .lock()
        .unwrap()
        .uploads()?
        .into_iter()
        .find(|t| t.id == id)
        .ok_or("上传任务不存在")?;
    match action.as_str() {
        "pause" => {
            if task.status == "merging" {
                return Err("正在提交云端文件，请等待提交完成".into());
            }
            state.uploads.pause(&id);
        }
        "resume" => {
            if !matches!(task.status.as_str(), "paused" | "error") {
                return Err("任务当前不能恢复".into());
            }
            if state.uploads.is_running(&id) {
                return Err("任务正在暂停，请稍后重试".into());
            }
            task.status = "waiting".into();
            task.error.clear();
            state.store.lock().unwrap().save_upload(&task)?;
            state.uploads.start(app, task)?;
        }
        "remove" => {
            if state.uploads.is_running(&id) {
                return Err("请先暂停上传，再移除任务".into());
            }
            state.store.lock().unwrap().remove_upload(&id)?;
        }
        _ => return Err("未知上传操作".into()),
    }
    Ok(())
}

/// 递归收集本地文件夹内的全部文件，带相对子目录（保留目录结构上传用）。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalEntry {
    pub path: String,
    pub relative: String,
}

#[tauri::command]
pub fn collect_local_files(root: String) -> Result<Vec<LocalEntry>, String> {
    fn walk(
        dir: &std::path::Path,
        base: &std::path::Path,
        out: &mut Vec<LocalEntry>,
    ) -> Result<(), String> {
        for entry in std::fs::read_dir(dir).map_err(|_| "无法读取所选文件夹")? {
            let entry = entry.map_err(|_| "无法读取所选文件夹")?;
            let path = entry.path();
            if path.is_dir() {
                walk(&path, base, out)?;
            } else if path.is_file() {
                let relative = path
                    .parent()
                    .and_then(|parent| parent.strip_prefix(base).ok())
                    .map(|parent| parent.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();
                out.push(LocalEntry {
                    path: path.to_string_lossy().to_string(),
                    relative,
                });
            }
        }
        Ok(())
    }
    let base = std::path::Path::new(&root);
    if !base.is_dir() {
        return Err("请选择有效的文件夹".into());
    }
    let mut out = Vec::new();
    walk(base, base, &mut out)?;
    out.sort_by(|a, b| a.relative.cmp(&b.relative).then(a.path.cmp(&b.path)));
    Ok(out)
}
