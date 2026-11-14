use crate::{
    model::*,
    providers::{xunlei, Session},
    session, AppState,
};
use futures_util::{stream, StreamExt};
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io::{Read, Seek, SeekFrom},
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};
use tokio::sync::{watch, Semaphore};

pub const BLOCK_SIZE: u64 = 4 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadRecord {
    pub id: String,
    pub name: String,
    pub local_path: String,
    pub remote_path: String,
    pub account_id: String,
    pub size: u64,
    pub modified_ns: String,
    pub concurrency: usize,
    pub status: String,
    pub error: String,
    pub completed: u64,
    pub speed: u64,
    pub created_at: u64,
    #[serde(default)]
    pub hashes: Vec<String>,
    #[serde(default)]
    pub done: Vec<bool>,
    #[serde(default)]
    pub upload_id: String,
    #[serde(default)]
    pub remote_file: Option<DriveFile>,
    #[serde(default)]
    pub confirmed_requests: u64,
    #[serde(default)]
    pub provider: Provider,
    #[serde(default)]
    pub parent_id: String,
    /// 分片大小（字节）。 表示未启用分片。
    #[serde(default)]
    pub part_size: u64,
    /// 已上传分片的 ETag，与 done 一一对应。
    #[serde(default)]
    pub part_etags: Vec<String>,
    /// 分片续传所需的服务端会话信息（迅雷/夸克 OSS 会话），空表示无。
    #[serde(default)]
    pub session: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadView {
    pub id: String,
    pub name: String,
    pub local_path: String,
    pub remote_path: String,
    pub size: u64,
    pub concurrency: usize,
    pub status: String,
    pub error: String,
    pub completed: u64,
    pub speed: u64,
    pub created_at: u64,
    pub completed_parts: usize,
    pub total_parts: usize,
    pub provider: Provider,
}
impl From<UploadRecord> for UploadView {
    fn from(t: UploadRecord) -> Self {
        Self {
            completed_parts: t.done.iter().filter(|v| **v).count(),
            total_parts: t.done.len().max(t.hashes.len()),
            id: t.id,
            name: t.name,
            local_path: t.local_path,
            remote_path: t.remote_path,
            size: t.size,
            concurrency: t.concurrency,
            status: t.status,
            error: t.error,
            completed: t.completed,
            speed: t.speed,
            created_at: t.created_at,
            provider: t.provider,
        }
    }
}

pub struct UploadManager {
    running: Mutex<HashMap<String, watch::Sender<bool>>>,
    slots: Arc<Semaphore>,
    parts: Arc<Semaphore>,
}
impl Default for UploadManager {
    fn default() -> Self {
        Self {
            running: Mutex::new(HashMap::new()),
            slots: Arc::new(Semaphore::new(2)),
            parts: Arc::new(Semaphore::new(8)),
        }
    }
}

pub fn fingerprint(path: &Path) -> Result<(u64, String), String> {
    let meta = path
        .metadata()
        .map_err(|_| "无法读取本地文件，请检查文件是否存在")?;
    if !meta.is_file() {
        return Err("暂不支持上传文件夹，请选择文件夹内的文件".into());
    }
    let modified = meta
        .modified()
        .map_err(|_| "无法读取文件修改时间")?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "文件时间无效")?
        .as_nanos()
        .to_string();
    Ok((meta.len(), modified))
}
fn check_local(t: &UploadRecord) -> Result<(), String> {
    let (size, modified) = fingerprint(Path::new(&t.local_path))?;
    if size != t.size || modified != t.modified_ns {
        return Err("本地文件已修改，请移除该任务并重新选择文件".into());
    }
    Ok(())
}
pub fn hash_file(path: &Path, cancelled: &AtomicBool) -> Result<Vec<String>, String> {
    let mut file = std::fs::File::open(path).map_err(|_| "无法打开上传文件")?;
    let mut buffer = vec![0u8; BLOCK_SIZE as usize];
    let mut hashes = Vec::new();
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err("已暂停".into());
        }
        let mut length = 0;
        while length < buffer.len() {
            let n = file
                .read(&mut buffer[length..])
                .map_err(|_| "读取上传文件失败")?;
            if n == 0 {
                break;
            }
            length += n;
        }
        if length == 0 && !hashes.is_empty() {
            break;
        }
        hashes.push(format!("{:x}", Md5::digest(&buffer[..length])));
        if length < buffer.len() {
            break;
        }
    }
    Ok(hashes)
}
fn confirmed_size(t: &UploadRecord) -> u64 {
    t.done
        .iter()
        .enumerate()
        .filter(|(_, done)| **done)
        .map(|(i, _)| t.size.saturating_sub(i as u64 * BLOCK_SIZE).min(BLOCK_SIZE))
        .sum()
}

/// 增量 SHA1，用于计算分片上传的中间态（夸克 x-oss-hash-ctx）。
struct Sha1State {
    h: [u32; 5],
    buf: [u8; 64],
    buf_len: usize,
    total: u64,
}

impl Sha1State {
    fn new() -> Self {
        Self {
            h: [
                0x6745_2301,
                0xEFCD_AB89,
                0x98BA_DCFE,
                0x1032_5476,
                0xC3D2_E1F0,
            ],
            buf: [0u8; 64],
            buf_len: 0,
            total: 0,
        }
    }
    fn compress(h: &mut [u32; 5], block: &[u8]) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for (i, word) in w.iter().enumerate() {
            let (f, k) = if i < 20 {
                ((b & c) | ((!b) & d), 0x5A82_7999u32)
            } else if i < 40 {
                (b ^ c ^ d, 0x6ED9_EBA1)
            } else if i < 60 {
                ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC)
            } else {
                (b ^ c ^ d, 0xCA62_C1D6)
            };
            let tmp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = tmp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    fn update(&mut self, mut data: &[u8]) {
        self.total += data.len() as u64;
        while !data.is_empty() {
            let take = (64 - self.buf_len).min(data.len());
            self.buf[self.buf_len..self.buf_len + take].copy_from_slice(&data[..take]);
            self.buf_len += take;
            data = &data[take..];
            if self.buf_len == 64 {
                let block = self.buf;
                Self::compress(&mut self.h, &block);
                self.buf_len = 0;
            }
        }
    }
    fn midstate(&self) -> [u32; 5] {
        self.h
    }
    fn finalize(mut self) -> [u8; 20] {
        let bit_len = self.total.wrapping_mul(8);
        let mut pad = vec![0x80u8];
        while (self.buf_len + pad.len()) % 64 != 56 {
            pad.push(0);
        }
        pad.extend_from_slice(&bit_len.to_be_bytes());
        let saved = self.total;
        self.update(&pad);
        self.total = saved;
        let mut out = [0u8; 20];
        for (i, word) in self.h.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        out
    }
}

fn hex_digest(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// 计算整文件md5、sha1，以及每个分片起始位置的 SHA1 中间态（夸克分片用）。
fn hash_with_states(
    path: &Path,
    part_size: u64,
) -> Result<(String, String, Vec<[u32; 5]>), String> {
    if part_size == 0 {
        return Err("分片大小无效".into());
    }
    let mut file = std::fs::File::open(path).map_err(|_| "读取上传文件失败")?;
    let mut md5 = Md5::new();
    let mut sha1 = Sha1State::new();
    let mut states: Vec<[u32; 5]> = vec![sha1.midstate()];
    let mut buffer = vec![0u8; 1024 * 1024];
    let mut processed: u64 = 0;
    let mut next_boundary = part_size;
    loop {
        let n = file.read(&mut buffer).map_err(|_| "读取上传文件失败")?;
        if n == 0 {
            break;
        }
        md5.update(&buffer[..n]);
        let mut start = 0usize;
        while start < n {
            let remain = (next_boundary - processed) as usize;
            let take = remain.min(n - start);
            sha1.update(&buffer[start..start + take]);
            processed += take as u64;
            start += take;
            if processed == next_boundary {
                states.push(sha1.midstate());
                next_boundary += part_size;
            }
        }
    }
    let sha1_hex = hex_digest(&sha1.finalize());
    Ok((format!("{:x}", md5.finalize()), sha1_hex, states))
}

/// 把分片中间态编码为夸克 OSS 需要的 x-oss-hash-ctx（base64 JSON）。
fn encode_hash_ctx(state: &[u32; 5], processed_bytes: u64) -> String {
    use base64::Engine as _;
    let body = serde_json::json!({
        "hash_type": "sha1",
        "h0": state[0].to_string(),
        "h1": state[1].to_string(),
        "h2": state[2].to_string(),
        "h3": state[3].to_string(),
        "h4": state[4].to_string(),
        "Nl": processed_bytes.wrapping_mul(8).to_string(),
        "Nh": "0",
        "data": "",
        "num": "0",
    });
    base64::engine::general_purpose::STANDARD.encode(body.to_string().as_bytes())
}

fn save(app: &tauri::AppHandle, task: &UploadRecord) -> Result<(), String> {
    app.state::<AppState>()
        .store
        .lock()
        .unwrap()
        .save_upload(task)?;
    let _ = app.emit_to("main", "tasks-changed", ());
    Ok(())
}

impl UploadManager {
    pub fn is_running(&self, id: &str) -> bool {
        self.running.lock().unwrap().contains_key(id)
    }
    pub fn pause(&self, id: &str) {
        if let Some(cancel) = self.running.lock().unwrap().get(id) {
            let _ = cancel.send(true);
        }
    }
    pub fn start(&self, app: tauri::AppHandle, task: UploadRecord) -> Result<(), String> {
        let (cancel, mut receiver) = watch::channel(false);
        {
            let mut running = self.running.lock().unwrap();
            if running.contains_key(&task.id) {
                return Err("上传任务正在运行或暂停中".into());
            }
            running.insert(task.id.clone(), cancel);
        }
        let slots = self.slots.clone();
        let parts = self.parts.clone();
        tauri::async_runtime::spawn(async move {
            let id = task.id.clone();
            let stop = Arc::new(AtomicBool::new(false));
            let result = tokio::select! {
                result = run_upload(&app, task, slots, parts, stop.clone()) => result,
                _ = receiver.changed() => { stop.store(true, Ordering::Relaxed); Err("__paused".into()) }
            };
            let state = app.state::<AppState>();
            finish_state(&app, &id, result);
            state.uploads.running.lock().unwrap().remove(&id);
            let _ = app.emit_to("main", "tasks-changed", ());
        });
        Ok(())
    }
}

fn finish_state(app: &tauri::AppHandle, id: &str, result: Result<(), String>) {
    let state = app.state::<AppState>();
    let store = state.store.lock().unwrap();
    if let Ok(records) = store.uploads() {
        if let Some(mut latest) = records.into_iter().find(|t| t.id == id) {
            if let Err(error) = result {
                latest.status = if error == "__paused" {
                    "paused"
                } else {
                    "error"
                }
                .into();
                latest.error = if error == "__paused" {
                    String::new()
                } else {
                    error
                };
                if latest.error.contains("上传会话已失效") {
                    latest.upload_id.clear();
                    latest.done.fill(false);
                    latest.completed = 0;
                    latest.session = serde_json::Value::Null;
                }
                latest.speed = 0;
                let _ = store.save_upload(&latest);
            }
        }
    }
}

async fn run_upload(
    app: &tauri::AppHandle,
    mut t: UploadRecord,
    slots: Arc<Semaphore>,
    parts: Arc<Semaphore>,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    let _slot = slots.acquire().await.map_err(|_| "上传队列已关闭")?;
    if t.provider == Provider::Xunlei {
        return run_upload_xunlei(app, &mut t, stop).await;
    }
    if t.provider == Provider::Quark {
        return run_upload_quark(app, &mut t, stop).await;
    }
    let session = session::connect(app, Provider::Baidu).await?;
    let identity = session.baidu_identity(Some(&t.account_id)).await?;
    check_local(&t)?;
    if session.drive_find(&t.remote_path).await?.is_some() {
        return Err("目标位置已有同名文件，请修改本地文件名或选择其他目录上传".into());
    }
    t.status = "hashing".into();
    save(app, &t)?;
    let path = t.local_path.clone();
    let hashes = tokio::task::spawn_blocking(move || hash_file(Path::new(&path), &stop))
        .await
        .map_err(|_| "文件校验任务失败")??;
    check_local(&t)?;
    if !t.hashes.is_empty() && t.hashes != hashes {
        return Err("本地文件内容已改变，请重新创建上传任务".into());
    }
    t.hashes = hashes;
    let pre = session
        .upload_precreate(&identity, &t.remote_path, t.size, &t.hashes, &t.upload_id)
        .await?;
    #[cfg(debug_assertions)]
    if std::env::args().any(|a| a.starts_with("--verify-baidu-")) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.verification");
        let _ = std::fs::write(dir.join("resume-checkpoint.json"), serde_json::json!({"savedParts": t.done.iter().filter(|v| **v).count(), "totalParts": t.hashes.len(), "serverMissing": pre.missing.len(), "sameSession": t.upload_id == pre.upload_id, "confirmedRequests": t.confirmed_requests}).to_string());
    }
    t.upload_id = pre.upload_id;
    t.done = vec![true; t.hashes.len()];
    for index in &pre.missing {
        t.done[*index] = false;
    }
    t.completed = confirmed_size(&t);
    t.status = "active".into();
    save(app, &t)?;
    let baseline = t.completed;
    let started = Instant::now();
    let pending = t.clone();
    let requests = stream::iter(pre.missing)
        .map(|index| {
            let session = &session;
            let token = &identity.token;
            let pending = &pending;
            let parts = &parts;
            async move {
                let _permit = parts.acquire().await.map_err(|_| "上传已停止")?;
                upload_one(session, token, pending, index)
                    .await
                    .map(|_| index)
            }
        })
        .buffer_unordered(t.concurrency);
    tokio::pin!(requests);
    while let Some(result) = requests.next().await {
        let index = result?;
        t.done[index] = true;
        t.confirmed_requests += 1;
        t.completed = confirmed_size(&t);
        t.speed =
            ((t.completed - baseline) as f64 / started.elapsed().as_secs_f64().max(0.001)) as u64;
        save(app, &t)?;
    }
    check_local(&t)?;
    t.status = "merging".into();
    t.speed = 0;
    save(app, &t)?;
    let remote = if pre.complete {
        session
            .drive_meta(std::slice::from_ref(&t.remote_path))
            .await?
            .into_iter()
            .next()
            .ok_or("上传完成但未查到文件")?
    } else {
        session
            .upload_finish(&identity, &t.remote_path, t.size, &t.hashes, &t.upload_id)
            .await?
    };
    if remote.size != t.size || remote.path != t.remote_path {
        return Err("云端文件信息与上传任务不一致，请检查目标目录".into());
    }
    t.remote_file = Some(remote);
    t.status = "complete".into();
    t.completed = t.size;
    t.upload_id.clear();
    save(app, &t)?;
    Ok(())
}

/// 大文件分片大小（迅雷/夸克续传）。
const MULTIPART_SIZE: u64 = 8 * 1024 * 1024;

fn sum_done(total: u64, part_size: u64, done: &[bool]) -> u64 {
    (0..done.len())
        .filter(|i| done[*i])
        .map(|i| total.saturating_sub(i as u64 * part_size).min(part_size))
        .sum()
}

async fn run_upload_xunlei(
    app: &tauri::AppHandle,
    t: &mut UploadRecord,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    let session = session::connect(app, Provider::Xunlei).await?;
    if session.cloud_identity().await?.0 != t.account_id {
        return Err("迅雷账号已切换，请使用创建任务时的账号继续上传".into());
    }
    check_local(t)?;
    if session.xunlei_find(&t.remote_path).await?.is_some() {
        return Err("目标位置已有同名文件，请修改本地文件名或选择其他目录上传".into());
    }
    t.status = "hashing".into();
    save(app, t)?;
    let path = t.local_path.clone();
    let size = t.size;
    let hash_stop = stop.clone();
    let gcid =
        tokio::task::spawn_blocking(move || xunlei::gcid(Path::new(&path), size, &hash_stop))
            .await
            .map_err(|_| "文件校验任务失败")??;
    check_local(t)?;
    let total = t.size;
    let local = t.local_path.clone();
    let parent_id = t.parent_id.clone();
    let name = t.name.clone();
    let remote_path = t.remote_path.clone();

    // 小文件：整对象单 PUT。
    if total <= MULTIPART_SIZE {
        t.status = "active".into();
        t.completed = 0;
        save(app, t)?;
        let (file_id, params) = session
            .xunlei_begin_upload(&parent_id, &name, total, &gcid)
            .await?;
        let progress = Arc::new(AtomicU64::new(0));
        let started = Instant::now();
        pump_single(
            app,
            t,
            session.xunlei_put_object(&params, Path::new(&local), total, &progress),
            &progress,
            total,
            started,
        )
        .await?;
        let id = if file_id.is_empty() {
            session
                .xunlei_find(&remote_path)
                .await?
                .map(|f| f.id)
                .unwrap_or_default()
        } else {
            file_id
        };
        t.remote_file = Some(DriveFile {
            id,
            path: remote_path,
            name,
            size: total,
            is_dir: false,
            modified: now(),
            created: now(),
            md5: gcid.clone(),
        });
        t.status = "complete".into();
        t.completed = total;
        t.session = serde_json::Value::Null;
        save(app, t)?;
        return Ok(());
    }

    // 大文件：分片 + 断点续传。
    let part_size = MULTIPART_SIZE;
    if t.session.get("params").is_none() || t.session.get("file_id").is_none() {
        let (file_id, params) = session
            .xunlei_begin_upload(&parent_id, &name, total, &gcid)
            .await?;
        let parts = total.div_ceil(part_size) as usize;
        t.session = serde_json::json!({ "file_id": file_id, "params": params, "upload_id": "" });
        t.part_size = part_size;
        t.done = vec![false; parts];
        t.part_etags = vec![String::new(); parts];
        t.hashes = vec![String::new(); parts];
        t.completed = 0;
        save(app, t)?;
    }
    let params = t.session["params"].clone();
    let file_id = t.session["file_id"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    let mut upload_id = t.session["upload_id"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    if upload_id.is_empty() {
        upload_id = session.xunlei_oss_initiate(&params).await?;
        t.session["upload_id"] = serde_json::json!(upload_id);
        save(app, t)?;
    }
    let parts = t.done.len();
    t.status = "active".into();
    t.completed = sum_done(total, part_size, &t.done);
    save(app, t)?;
    let started = Instant::now();

    let mut multipart_err: Option<String> = None;
    for i in 0..parts {
        if stop.load(Ordering::Relaxed) {
            return Err("__paused".into());
        }
        if t.done[i] {
            continue;
        }
        let offset = i as u64 * part_size;
        let len = (total - offset).min(part_size);
        let progress = Arc::new(AtomicU64::new(0));
        let put = session.xunlei_oss_put_part(
            &params,
            &upload_id,
            (i + 1) as u32,
            Path::new(&local),
            offset,
            len,
            &progress,
        );
        let baseline = sum_done(total, part_size, &t.done);
        match pump_part(app, t, put, &progress, baseline, baseline + len).await {
            Ok(etag) => {
                t.part_etags[i] = etag;
                t.done[i] = true;
                t.completed = sum_done(total, part_size, &t.done);
                t.speed = (t.completed as f64 / started.elapsed().as_secs_f64().max(0.001)) as u64;
                save(app, t)?;
            }
            Err(e) => {
                multipart_err = Some(e);
                break;
            }
        }
    }

    let id = if multipart_err.is_none() {
        let pairs: Vec<(u32, String)> = (0..parts)
            .map(|i| ((i + 1) as u32, t.part_etags[i].clone()))
            .collect();
        match session
            .xunlei_oss_complete(&params, &upload_id, &pairs)
            .await
        {
            Ok(()) => {
                if file_id.is_empty() {
                    session
                        .xunlei_find(&remote_path)
                        .await?
                        .map(|f| f.id)
                        .unwrap_or_default()
                } else {
                    file_id.clone()
                }
            }
            Err(e) => {
                multipart_err = Some(e);
                String::new()
            }
        }
    } else {
        String::new()
    };

    if let Some(err) = multipart_err {
        // 分片失败：回退为整对象单PUT（覆盖已上传的分片）。
        t.status = "active".into();
        t.completed = 0;
        t.done.iter_mut().for_each(|v| *v = false);
        t.part_etags.iter_mut().for_each(|v| v.clear());
        save(app, t)?;
        let progress = Arc::new(AtomicU64::new(0));
        let started2 = Instant::now();
        pump_single(
            app,
            t,
            session.xunlei_put_object(&params, Path::new(&local), total, &progress),
            &progress,
            total,
            started2,
        )
        .await
        .map_err(|e| format!("{err}；回退整文件上传失败：{e}"))?;
        let id = if file_id.is_empty() {
            session
                .xunlei_find(&remote_path)
                .await?
                .map(|f| f.id)
                .unwrap_or_default()
        } else {
            file_id
        };
        t.remote_file = Some(DriveFile {
            id,
            path: remote_path,
            name,
            size: total,
            is_dir: false,
            modified: now(),
            created: now(),
            md5: gcid.clone(),
        });
        t.status = "complete".into();
        t.completed = total;
        t.session = serde_json::Value::Null;
        save(app, t)?;
        return Ok(());
    }

    t.remote_file = Some(DriveFile {
        id,
        path: remote_path,
        name,
        size: total,
        is_dir: false,
        modified: now(),
        created: now(),
        md5: gcid,
    });
    t.status = "complete".into();
    t.completed = total;
    t.session = serde_json::Value::Null;
    save(app, t)?;
    Ok(())
}

/// 单次整对象上传：带进度轮询地等待完成。
async fn pump_single<F>(
    app: &tauri::AppHandle,
    t: &mut UploadRecord,
    fut: F,
    progress: &Arc<AtomicU64>,
    total: u64,
    started: Instant,
) -> Result<(), String>
where
    F: std::future::Future<Output = Result<(), String>>,
{
    tokio::pin!(fut);
    let mut ticker = tokio::time::interval(Duration::from_millis(300));
    loop {
        tokio::select! {
            result = &mut fut => break result?,
            _ = ticker.tick() => {
                let done = progress.load(Ordering::Relaxed).min(total);
                if done > t.completed {
                    t.completed = done;
                    let elapsed = started.elapsed().as_secs_f64().max(0.001);
                    t.speed = (done as f64 / elapsed) as u64;
                    save(app, t)?;
                }
            }
        }
    }
    Ok(())
}

/// 单个分片上传：带进度轮询地等待完成，返回 ETag。
async fn pump_part<F>(
    app: &tauri::AppHandle,
    t: &mut UploadRecord,
    fut: F,
    progress: &Arc<AtomicU64>,
    baseline: u64,
    upper: u64,
) -> Result<String, String>
where
    F: std::future::Future<Output = Result<String, String>>,
{
    tokio::pin!(fut);
    let mut ticker = tokio::time::interval(Duration::from_millis(300));
    let mut last = t.completed;
    loop {
        tokio::select! {
            result = &mut fut => break result,
            _ = ticker.tick() => {
                let done = baseline + progress.load(Ordering::Relaxed).min(upper - baseline);
                if done > t.completed {
                    t.completed = done;
                    t.speed = ((done - last) as f64 / 0.3) as u64;
                    last = done;
                    save(app, t)?;
                }
            }
        }
    }
}

async fn run_upload_quark(
    app: &tauri::AppHandle,
    t: &mut UploadRecord,
    stop: Arc<AtomicBool>,
) -> Result<(), String> {
    let session = session::connect(app, Provider::Quark).await?;
    if session.cloud_identity().await?.0 != t.account_id {
        return Err("夸克账号已切换，请使用创建任务时的账号继续上传".into());
    }
    check_local(t)?;
    let total = t.size;
    let local = t.local_path.clone();
    let parent_id = t.parent_id.clone();
    let name = t.name.clone();
    let remote_path = t.remote_path.clone();

    if total <= MULTIPART_SIZE {
        return run_upload_quark_single(
            app,
            t,
            &session,
            &local,
            &parent_id,
            &name,
            &remote_path,
            total,
        )
        .await;
    }

    // 大文件：分片 + 断点续传。
    let part_size = MULTIPART_SIZE;
    // 每次运行都重新计算整文件 md5/sha1 与各分片 SHA1 中间态（一次顺序读文件）。
    let (md5, sha1, states) = {
        let path = local.clone();
        tokio::task::spawn_blocking(move || hash_with_states(Path::new(&path), part_size))
            .await
            .map_err(|_| "文件校验任务失败")??
    };

    if t.session.get("task_id").is_none() {
        let begin = session
            .quark_upload_begin(&parent_id, &name, total, &md5, &sha1)
            .await?;
        let parts = total.div_ceil(part_size) as usize;
        let mut s = begin;
        s["name"] = serde_json::json!(name);
        t.session = s;
        t.part_size = part_size;
        t.done = vec![false; parts];
        t.part_etags = vec![String::new(); parts];
        t.hashes = vec![String::new(); parts];
        t.completed = 0;
        save(app, t)?;
    }
    let parts = t.done.len();
    t.status = "active".into();
    t.completed = sum_done(total, part_size, &t.done);
    save(app, t)?;
    let started = Instant::now();
    let sess = t.session.clone();

    let mut multipart_err: Option<String> = None;
    for i in 0..parts {
        if stop.load(Ordering::Relaxed) {
            return Err("__paused".into());
        }
        if t.done[i] {
            continue;
        }
        let offset = i as u64 * part_size;
        let len = (total - offset).min(part_size);
        let hash_ctx = if i == 0 {
            None
        } else {
            let state = states.get(i).copied().unwrap_or(states[0]);
            Some(encode_hash_ctx(&state, offset))
        };
        let progress = Arc::new(AtomicU64::new(0));
        let put = session.quark_upload_put_part(
            &sess,
            (i + 1) as u32,
            hash_ctx.as_deref(),
            Path::new(&local),
            offset,
            len,
            &progress,
        );
        let baseline = sum_done(total, part_size, &t.done);
        match pump_part(app, t, put, &progress, baseline, baseline + len).await {
            Ok(etag) => {
                t.part_etags[i] = etag;
                t.done[i] = true;
                t.completed = sum_done(total, part_size, &t.done);
                t.speed = (t.completed as f64 / started.elapsed().as_secs_f64().max(0.001)) as u64;
                save(app, t)?;
            }
            Err(e) => {
                multipart_err = Some(e);
                break;
            }
        }
    }

    if multipart_err.is_none() {
        let pairs: Vec<(u32, String)> = (0..parts)
            .map(|i| ((i + 1) as u32, t.part_etags[i].clone()))
            .collect();
        match session.quark_upload_complete(&t.session, &pairs).await {
            Ok(fid) => {
                if fid.is_empty() {
                    return Err("夸克上传完成但未返回文件标识，请刷新网盘确认".into());
                }
                t.remote_file = Some(DriveFile {
                    id: fid,
                    path: remote_path,
                    name,
                    size: total,
                    is_dir: false,
                    modified: now(),
                    created: now(),
                    md5,
                });
                t.status = "complete".into();
                t.completed = total;
                t.session = serde_json::Value::Null;
                save(app, t)?;
                return Ok(());
            }
            Err(e) => multipart_err = Some(e),
        }
    }

    // 分片失败：回退整文件单 PUT。
    let err = multipart_err.unwrap_or_default();
    t.session = serde_json::Value::Null;
    t.completed = 0;
    t.done.iter_mut().for_each(|v| *v = false);
    run_upload_quark_single(
        app,
        t,
        &session,
        &local,
        &parent_id,
        &name,
        &remote_path,
        total,
    )
    .await
    .map_err(|e| format!("{err}；回退整文件上传失败：{e}"))
}

#[allow(clippy::too_many_arguments)]
async fn run_upload_quark_single(
    app: &tauri::AppHandle,
    t: &mut UploadRecord,
    session: &Session,
    local: &str,
    parent_id: &str,
    name: &str,
    remote_path: &str,
    total: u64,
) -> Result<(), String> {
    t.status = "uploading".into();
    t.completed = 0;
    save(app, t)?;
    let progress = Arc::new(AtomicU64::new(0));
    let stop_flag = AtomicBool::new(false);
    let started = Instant::now();
    let upl = session.cloud_upload(
        Path::new(local),
        parent_id,
        name,
        remote_path,
        total,
        &stop_flag,
        &progress,
    );
    tokio::pin!(upl);
    let mut ticker = tokio::time::interval(Duration::from_millis(300));
    let mut last = 0u64;
    let remote = loop {
        tokio::select! {
            result = &mut upl => break result?,
            _ = ticker.tick() => {
                let done = progress.load(Ordering::Relaxed).min(total);
                if done > t.completed {
                    t.completed = done;
                    t.speed = ((done - last) as f64 / 0.3) as u64;
                    last = done;
                    save(app, t)?;
                }
            }
        }
    };
    if remote.size != total {
        return Err("云端文件大小与本地不一致，请重试".into());
    }
    t.speed = (total as f64 / started.elapsed().as_secs_f64().max(0.001)) as u64;
    t.remote_file = Some(remote);
    t.status = "complete".into();
    t.completed = total;
    t.session = serde_json::Value::Null;
    save(app, t)?;
    Ok(())
}

async fn upload_one(
    session: &Session,
    token: &str,
    task: &UploadRecord,
    index: usize,
) -> Result<(), String> {
    upload_one_with(task, index, Duration::from_secs(1), |bytes| {
        session.upload_part(token, &task.remote_path, &task.upload_id, index, bytes)
    })
    .await
}

async fn upload_one_with<F, Fut>(
    task: &UploadRecord,
    index: usize,
    retry_delay: Duration,
    send: F,
) -> Result<(), String>
where
    F: Fn(Vec<u8>) -> Fut,
    Fut: std::future::Future<Output = Result<String, String>>,
{
    let mut last_error = String::new();
    for attempt in 0..4 {
        if attempt > 0 {
            tokio::time::sleep(retry_delay * (1 << (attempt - 1))).await;
        }
        check_local(task)?;
        let path = task.local_path.clone();
        let size = task
            .size
            .saturating_sub(index as u64 * BLOCK_SIZE)
            .min(BLOCK_SIZE) as usize;
        let bytes = tokio::task::spawn_blocking(move || {
            let mut f = std::fs::File::open(path).map_err(|_| "读取分片失败")?;
            f.seek(SeekFrom::Start(index as u64 * BLOCK_SIZE))
                .map_err(|_| "读取分片位置失败")?;
            let mut data = vec![0; size];
            f.read_exact(&mut data).map_err(|_| "本地文件内容已改变")?;
            Ok::<_, String>(data)
        })
        .await
        .map_err(|_| "分片读取任务失败")??;
        if format!("{:x}", Md5::digest(&bytes)) != task.hashes[index] {
            return Err("本地文件内容已改变，请重新创建上传任务".into());
        }
        match send(bytes).await {
            Ok(md5) if md5.eq_ignore_ascii_case(&task.hashes[index]) => return Ok(()),
            Ok(_) => last_error = "百度返回的分片校验值不匹配".into(),
            Err(e) => last_error = e,
        }
    }
    Err(format!("第{} 个分片上传失败：{last_error}", index + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn fixture(bytes: &[u8]) -> (PathBufGuard, UploadRecord) {
        let path = std::env::temp_dir().join(format!(
            "clouddownloader-upload-test-{}.bin",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&path, bytes).unwrap();
        let (size, modified_ns) = fingerprint(&path).unwrap();
        let hashes = hash_file(&path, &AtomicBool::new(false)).unwrap();
        let record = UploadRecord {
            id: uuid::Uuid::new_v4().to_string(),
            name: "test.bin".into(),
            local_path: path.to_string_lossy().into(),
            remote_path: "/test.bin".into(),
            account_id: "account".into(),
            size,
            modified_ns,
            concurrency: 4,
            status: "paused".into(),
            error: String::new(),
            completed: 0,
            speed: 0,
            created_at: now(),
            done: vec![false; hashes.len()],
            hashes,
            upload_id: "session-private".into(),
            remote_file: None,
            confirmed_requests: 0,
            provider: Provider::Baidu,
            parent_id: String::new(),
            part_size: 0,
            part_etags: Vec::new(),
            session: serde_json::Value::Null,
        };
        (PathBufGuard(path), record)
    }
    struct PathBufGuard(std::path::PathBuf);
    impl Drop for PathBufGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[tokio::test]
    async fn retry_resends_only_the_failed_slice_and_checks_response_hash() {
        let mut data = vec![37u8; BLOCK_SIZE as usize];
        data.extend_from_slice(b"final slice");
        let (_path, record) = fixture(&data);
        let calls = AtomicUsize::new(0);
        upload_one_with(&record, 1, Duration::ZERO, |bytes| {
            assert_eq!(bytes, b"final slice");
            let attempt = calls.fetch_add(1, Ordering::SeqCst);
            async move {
                match attempt {
                    0 => Err("transient connection failure".into()),
                    1 => Ok("bad-md5".into()),
                    _ => Ok(format!("{:x}", Md5::digest(&bytes))),
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        let calls = AtomicUsize::new(0);
        assert!(upload_one_with(&record, 1, Duration::ZERO, |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            async { Err("offline".into()) }
        })
        .await
        .is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn refuses_changed_source_before_resending_or_resuming() {
        let (path, record) = fixture(b"original");
        let calls = AtomicUsize::new(0);
        let result = upload_one_with(&record, 0, Duration::ZERO, |_| {
            calls.fetch_add(1, Ordering::SeqCst);
            std::fs::write(&path.0, b"modified and larger").unwrap();
            async { Err("transient".into()) }
        })
        .await;
        assert!(result.unwrap_err().contains("本地文件已修改"));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn checkpoints_survive_reopen_and_private_fields_stay_out_of_ui() {
        let mut data = vec![4; BLOCK_SIZE as usize];
        data.extend_from_slice(b"tail");
        let (_path, mut record) = fixture(&data);
        record.done[0] = true;
        record.completed = confirmed_size(&record);
        let db_path = std::env::temp_dir().join(format!(
            "clouddownloader-upload-test-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        {
            let store = crate::store::Store::open(&db_path).unwrap();
            store.save_upload(&record).unwrap();
        }
        {
            let store = crate::store::Store::open(&db_path).unwrap();
            let resumed = store.uploads().unwrap().remove(0);
            assert_eq!(resumed.upload_id, record.upload_id);
            assert_eq!(resumed.done, vec![true, false]);
            assert_eq!(resumed.completed, BLOCK_SIZE);
            assert_eq!(resumed.hashes, record.hashes);
            let view = serde_json::to_string(&UploadView::from(resumed)).unwrap();
            assert!(!view.contains("session-private"));
            assert!(!view.contains(&record.hashes[0]));
        }
        std::fs::remove_file(db_path).unwrap();
        let (_path, empty) = fixture(&[]);
        assert_eq!(empty.hashes, vec!["d41d8cd98f00b204e9800998ecf8427e"]);
    }
}
