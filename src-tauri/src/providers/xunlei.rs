// Xunlei Cloud Drive (pan.xunlei.com) adapter.
//
// Personal-vault and share endpoints mirror the public Alist/OpenList
// `drivers/thunder` implementation, and the share/detail flow mirrors the
// public gopeed `xunlei` extension. The web client is identified with the same
// public client id/version that the browser at pan.xunlei.com uses, and every
// request is authenticated with the cookies captured from the login WebView.
// The captcha sign is a deterministic md5 chain over the public web salt list.
use super::cloud::{check, entry, Page};
use super::{crypto, number, text, Session};
use crate::model::*;
use base64::Engine as _;
use serde_json::{json, Value};
use sha1::{Digest, Sha1};
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

pub const CLIENT_ID: &str = "Xqp0kJBXWhwaTpB6";
const CLIENT_VERSION: &str = "1.92.89";
const PACKAGE_NAME: &str = "pan.xunlei.com";
const API: &str = "https://api-pan.xunlei.com/drive/v1";
const CAPTCHA_INIT: &str = "https://xluser-ssl.xunlei.com/v1/shield/captcha/init";
const USER_ME: &str = "https://xluser-ssl.xunlei.com/v1/user/me";
// The share endpoints take the share id under an obfuscated query key.
const SHARE_ID_PARAM: &str = "share_id";
pub const DOWNLOAD_UA: &str =
    "Dalvik/2.1.0 (Linux; U; Android 12; M2004J7AC Build/SP1A.210812.016)";
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

// Public captcha-sign salts for the pan.xunlei.com web client.
const ALGORITHMS: [&str; 14] = [
    "q1FLg",
    "OECaw0higFYs7qCdOWvMEe",
    "1UQvF/CWht+RATnmBnyakvJFHL1jaAb8MRYvdVHQLZ",
    "OV+xWBsbnAehPNmDEUnjcVT",
    "bbuvyCYHdJwBOtJ8Cdeg",
    "9ROgxnyRfQInpATpkchBTZW",
    "vXA2EpRk8",
    "oEd",
    "QCBQwX/",
    "P5NlYzyVtGjJUI/dIzi+SOL+mf0Wl17",
    "7GRbxu7OmpasI841c66J",
    "PEVQN9w+A4wRbGoX",
    "CJ3yz0fu1kqhPDOyL438W",
    "tD75Q",
];

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn hmac_sha1(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut block = [0u8; 64];
    if key.len() > 64 {
        block[..20].copy_from_slice(Sha1::digest(key).as_slice());
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for (dst, &byte) in ipad.iter_mut().zip(block.iter()) {
        *dst ^= byte;
    }
    for (dst, &byte) in opad.iter_mut().zip(block.iter()) {
        *dst ^= byte;
    }
    let mut inner = Sha1::new();
    inner.update(ipad);
    inner.update(data);
    let inner = inner.finalize();
    let mut outer = Sha1::new();
    outer.update(opad);
    outer.update(inner);
    outer.finalize().to_vec()
}

/// Xunlei global content id: sha1 over the concatenated sha1 of fixed blocks.
pub fn gcid(path: &Path, size: u64, stop: &AtomicBool) -> Result<String, String> {
    let mut psize = 0x40000u64;
    while psize > 0 && size / psize > 0x200 && psize < 0x200000 {
        psize *= 2;
    }
    let mut file = std::fs::File::open(path).map_err(|_| "无法打开上传文件")?;
    let mut buffer = vec![0u8; psize as usize];
    let mut digests = Vec::new();
    loop {
        if stop.load(Ordering::Relaxed) {
            return Err("已暂停".into());
        }
        let mut length = 0;
        while length < buffer.len() {
            let read = file
                .read(&mut buffer[length..])
                .map_err(|_| "读取上传文件失败")?;
            if read == 0 {
                break;
            }
            length += read;
        }
        if length == 0 && !digests.is_empty() {
            break;
        }
        digests.push(Sha1::digest(&buffer[..length]).to_vec());
        if length < buffer.len() {
            break;
        }
    }
    let mut outer = Sha1::new();
    for digest in &digests {
        outer.update(digest);
    }
    Ok(format!("{:x}", outer.finalize()))
}

fn share_file(raw: &Value, parent: &str, token: &str) -> Result<ShareFile, String> {
    let id = ["id", "file_id"]
        .iter()
        .map(|key| text(&raw[*key]))
        .find(|value| !value.is_empty())
        .ok_or("迅雷分享文件标识缺失")?;
    let name = ["name", "file_name"]
        .iter()
        .map(|key| text(&raw[*key]))
        .find(|value| !value.is_empty())
        .ok_or("迅雷分享文件名称缺失，请刷新列表")?;
    let size = ["size", "file_size"]
        .iter()
        .map(|key| number(&raw[*key]))
        .find(|value| *value > 0)
        .unwrap_or(0);
    Ok(ShareFile {
        id,
        name,
        size,
        is_dir: text(&raw["kind"]) == "drive#folder",
        parent_id: parent.into(),
        token: token.into(),
    })
}

impl Session {
    fn captcha_sign(&self, timestamp: &str) -> String {
        let mut value = format!(
            "{CLIENT_ID}{CLIENT_VERSION}{PACKAGE_NAME}{}{timestamp}",
            self.device_id
        );
        for salt in ALGORITHMS {
            value = crypto::md5_hex(&format!("{value}{salt}"));
        }
        format!("1.{value}")
    }

    /// Decode the subscription id from the bearer token so the captcha request
    /// can be bound to the real user id (the shield service rejects "0").
    fn token_user_id(&self) -> String {
        let raw = self.authorization.trim();
        let token = raw.strip_prefix("Bearer ").unwrap_or(raw).trim();
        let Some(payload) = token.split('.').nth(1) else {
            return String::new();
        };
        let Ok(bytes) = crypto::base64_decode(payload) else {
            return String::new();
        };
        serde_json::from_slice::<Value>(&bytes)
            .map(|value| text(&value["sub"]))
            .unwrap_or_default()
    }

    async fn captcha(&self, action: &str) -> Result<String, String> {
        let timestamp = now_ms().to_string();
        let sign = self.captcha_sign(&timestamp);
        let body = json!({
            "action": action,
            "captcha_token": "",
            "client_id": CLIENT_ID,
            "device_id": self.device_id,
            "meta": {
                "client_version": CLIENT_VERSION,
                "package_name": PACKAGE_NAME,
                "user_id": self.token_user_id(),
                "timestamp": timestamp,
                "captcha_sign": sign,
            },
            "redirect_uri": "xlaccsdk01://xunlei.com/callback?state=harbor",
        });
        let request = self
            .transport
            .client
            .post(self.endpoint(CAPTCHA_INIT))
            .header("Accept", "application/json, text/plain, */*")
            .header("Referer", "https://pan.xunlei.com/")
            .header("User-Agent", USER_AGENT)
            .header("X-Client-Id", CLIENT_ID)
            .header("X-Client-Version", CLIENT_VERSION)
            .header("X-Device-Id", &self.device_id)
            .json(&body);
        let value = self.json(request).await?;
        if let Some(url) = value["url"].as_str() {
            if !url.is_empty() {
                return Err(
                    "迅雷要求完成一次人机验证，请在登录窗口打开 pan.xunlei.com 完成验证后重试"
                        .into(),
                );
            }
        }
        let token = text(&value["captcha_token"]);
        if token.is_empty() {
            return Err("未能获取迅雷访问令牌，请在登录窗口重新登录".into());
        }
        Ok(token)
    }

    async fn api_get(
        &self,
        path: &str,
        query: &[(&str, String)],
        captcha: &str,
    ) -> Result<Value, String> {
        let mut request = self
            .transport
            .client
            .get(self.endpoint(&format!("{API}{path}")))
            .header("X-Client-Id", CLIENT_ID)
            .header("X-Client-Version", CLIENT_VERSION)
            .header("X-Device-Id", &self.device_id)
            .header("X-Captcha-Token", captcha)
            .header("Accept", "application/json, text/plain, */*")
            .header("Referer", "https://pan.xunlei.com/")
            .header("User-Agent", USER_AGENT)
            .query(query);
        if !self.authorization.is_empty() {
            request = request.header("Authorization", &self.authorization);
        }
        let value = self.json(request).await?;
        check(self.provider, value)
    }

    async fn api_post(&self, path: &str, body: &Value, captcha: &str) -> Result<Value, String> {
        let mut request = self
            .transport
            .client
            .post(self.endpoint(&format!("{API}{path}")))
            .header("X-Client-Id", CLIENT_ID)
            .header("X-Client-Version", CLIENT_VERSION)
            .header("X-Device-Id", &self.device_id)
            .header("X-Captcha-Token", captcha)
            .header("Accept", "application/json, text/plain, */*")
            .header("Referer", "https://pan.xunlei.com/")
            .header("User-Agent", USER_AGENT)
            .json(body);
        if !self.authorization.is_empty() {
            request = request.header("Authorization", &self.authorization);
        }
        let value = self.json(request).await?;
        check(self.provider, value)
    }

    /// Issue an arbitrary authenticated API request (e.g. PATCH or a batch POST)
    /// that needs a captcha token, reusing the shared header/authorization setup.
    async fn api_send(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<&Value>,
        captcha: &str,
    ) -> Result<Value, String> {
        let mut request = self
            .transport
            .client
            .request(method, self.endpoint(&format!("{API}{path}")))
            .header("X-Client-Id", CLIENT_ID)
            .header("X-Client-Version", CLIENT_VERSION)
            .header("X-Device-Id", &self.device_id)
            .header("X-Captcha-Token", captcha)
            .header("Accept", "application/json, text/plain, */*")
            .header("Referer", "https://pan.xunlei.com/")
            .header("User-Agent", USER_AGENT);
        if let Some(body) = body {
            request = request.json(body);
        }
        if !self.authorization.is_empty() {
            request = request.header("Authorization", &self.authorization);
        }
        let value = self.json(request).await?;
        check(self.provider, value)
    }

    fn jwt_identity(&self) -> Option<(String, String)> {
        let raw = self.authorization.trim();
        let token = raw.strip_prefix("Bearer ").unwrap_or(raw).trim();
        let payload = token.split('.').nth(1)?;
        let bytes = crypto::base64_decode(payload).ok()?;
        let value: Value = serde_json::from_slice(&bytes).ok()?;
        let as_text = |v: &Value| -> String {
            if let Some(s) = v.as_str() {
                s.to_string()
            } else if let Some(n) = v.as_i64() {
                n.to_string()
            } else if let Some(n) = v.as_u64() {
                n.to_string()
            } else {
                String::new()
            }
        };
        let id = ["sub", "user_id", "userId", "uid"]
            .iter()
            .map(|key| as_text(&value[*key]))
            .find(|value| !value.is_empty())?;
        let name = ["nickname", "nickName", "name", "userName"]
            .iter()
            .map(|key| as_text(&value[*key]))
            .chain(std::iter::once(as_text(&value["user_info"]["nickname"])))
            .find(|value| !value.is_empty())
            .unwrap_or_else(|| id.clone());
        Some((id, name))
    }

    pub async fn xunlei_identity(&self) -> Result<(String, String), String> {
        if let Some(identity) = self.jwt_identity() {
            return Ok(identity);
        }
        let mut request = self
            .transport
            .client
            .get(self.endpoint(USER_ME))
            .header("X-Client-Id", CLIENT_ID)
            .header("X-Client-Version", CLIENT_VERSION)
            .header("X-Device-Id", &self.device_id)
            .header("Accept", "application/json")
            .header("User-Agent", USER_AGENT);
        if !self.authorization.is_empty() {
            request = request.header("Authorization", &self.authorization);
        }
        let value = self
            .json(request)
            .await
            .map_err(|_| "请先登录迅雷网盘".to_string())?;
        let id = ["sub", "user_id", "userId", "uid"]
            .iter()
            .map(|key| text(&value[*key]))
            .find(|value| !value.is_empty())
            .or_else(|| self.cookie_value(&["https://pan.xunlei.com/"], "userId"))
            .ok_or("未取得迅雷账号标识，请在官方窗口重新登录")?;
        let name = ["name", "nickname", "nickName", "userName"]
            .iter()
            .map(|key| text(&value[*key]))
            .find(|value| !value.is_empty())
            .unwrap_or_else(|| "迅雷账号".into());
        Ok((id, name))
    }

    pub async fn xunlei_page(
        &self,
        path: &str,
        id: &str,
        _page: u32,
        cursor: Option<&str>,
    ) -> Result<Page, String> {
        let captcha = self.captcha("GET:/drive/v1/files").await?;
        let parent = if id == "0" {
            String::new()
        } else {
            id.to_string()
        };
        let mut query = vec![
            ("space", String::new()),
            ("__type", "drive".to_string()),
            ("refresh", "true".to_string()),
            ("__sync", "true".to_string()),
            ("parent_id", parent),
            ("with_audit", "true".to_string()),
            ("limit", "100".to_string()),
            (
                "filters",
                r#"{"phase":{"eq":"PHASE_TYPE_COMPLETE"},"trashed":{"eq":false}}"#.to_string(),
            ),
        ];
        if let Some(cursor) = cursor {
            query.push(("page_token", cursor.to_string()));
        }
        let value = self.api_get("/files", &query, &captcha).await?;
        let rows = value["files"].as_array().cloned().unwrap_or_default();
        let entries = rows
            .iter()
            .map(|raw| entry(self.provider, raw, path))
            .collect::<Result<Vec<_>, _>>()?;
        let next = text(&value["next_page_token"]);
        Ok(Page {
            entries,
            next: if next.is_empty() { None } else { Some(next) },
        })
    }

    pub async fn xunlei_find(&self, path: &str) -> Result<Option<DriveFile>, String> {
        let (parent, name) = match path.rsplit_once('/') {
            Some((parent, name)) => (if parent.is_empty() { "/" } else { parent }, name),
            None => ("/", path),
        };
        let mut cursor = None;
        for page in 1..=200u32 {
            let batch = self.cloud_page(parent, page, cursor.as_deref()).await?;
            if let Some(found) = batch.entries.into_iter().find(|e| e.file.name == name) {
                return Ok(Some(found.file));
            }
            cursor = batch.next;
            if cursor.is_none() {
                break;
            }
        }
        Ok(None)
    }

    pub async fn xunlei_personal_download(&self, id: &str) -> Result<String, String> {
        let captcha = self.captcha("GET:/drive/v1/files").await?;
        let value = self
            .api_get(
                &format!("/files/{}", identifier_component(id)?),
                &[("space", String::new()), ("usage", "CONSUME".to_string())],
                &captcha,
            )
            .await?;
        let link = text(&value["web_content_link"]);
        if link.is_empty() {
            return Err("迅雷未返回可用的下载地址，请刷新后重试".into());
        }
        Ok(link)
    }

    pub async fn xunlei_share(
        &self,
        share: &ShareLink,
        parent: &str,
    ) -> Result<ShareListing, String> {
        let captcha = self.captcha("GET:/drive/v1/share").await?;
        let root = self
            .api_get(
                "/share",
                &[
                    (SHARE_ID_PARAM, share.id.clone()),
                    ("limit", "100".to_string()),
                    ("space", String::new()),
                    ("thumbnail_size", "SIZE_MEDIUM".to_string()),
                    ("scene", "NORMAL".to_string()),
                    ("pass_code", share.passcode.clone()),
                ],
                &captcha,
            )
            .await?;
        let token = text(&root["pass_code_token"]);
        let title = text(&root["share_info"]["name"]);
        let title = if title.is_empty() {
            format!("{}分享", self.provider.name())
        } else {
            title
        };
        let rows = if parent == "0" || parent.is_empty() {
            root["files"].as_array().cloned().unwrap_or_default()
        } else {
            let detail = self
                .api_get(
                    "/share/detail",
                    &[
                        (SHARE_ID_PARAM, share.id.clone()),
                        ("limit", "100".to_string()),
                        ("space", String::new()),
                        ("thumbnail_size", "SIZE_MEDIUM".to_string()),
                        ("scene", "NORMAL".to_string()),
                        ("parent_id", parent.to_string()),
                        ("pass_code_token", token.clone()),
                    ],
                    &captcha,
                )
                .await?;
            detail["files"].as_array().cloned().unwrap_or_default()
        };
        let files = rows
            .iter()
            .map(|raw| share_file(raw, parent, &token))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ShareListing {
            stoken: token,
            share: share.clone(),
            title,
            parent_id: parent.into(),
            files,
        })
    }

    pub async fn xunlei_share_download(
        &self,
        share: &ShareLink,
        file: &ShareFile,
    ) -> Result<String, String> {
        let captcha = self.captcha("GET:/drive/v1/share/file_info").await?;
        let value = self
            .api_get(
                "/share/file_info",
                &[
                    (SHARE_ID_PARAM, share.id.clone()),
                    ("file_id", file.id.clone()),
                    ("pass_code", share.passcode.clone()),
                    ("pass_code_token", file.token.clone()),
                    ("folder_id", file.parent_id.clone()),
                    ("space", String::new()),
                    ("scene", "NORMAL".to_string()),
                ],
                &captcha,
            )
            .await?;
        let info = value.get("file_info").cloned().unwrap_or(value);
        let link = ["web_content_link", "download_url"]
            .iter()
            .map(|key| text(&info[*key]))
            .find(|value| !value.is_empty());
        if let Some(link) = link {
            return Ok(link);
        }
        Err("该迅雷分享文件需要先转存到自己的网盘才能下载，请稍后在迅雷客户端转存".into())
    }

    /// 在迅雷云盘创建上传文件条目，返回 (文件 id, OSS 直传会话参数)。
    pub async fn xunlei_begin_upload(
        &self,
        parent_id: &str,
        name: &str,
        size: u64,
        gcid: &str,
    ) -> Result<(String, Value), String> {
        let captcha = self.captcha("POST:/drive/v1/files").await?;
        let body = json!({
            "kind": "drive#file",
            "parent_id": parent_id,
            "name": name,
            "size": size,
            "hash": gcid,
            "upload_type": "UPLOAD_TYPE_RESUMABLE",
        });
        let value = self.api_post("/files", &body, &captcha).await?;
        let params = value
            .get("resumable")
            .and_then(|resumable| resumable.get("params"))
            .cloned()
            .ok_or("迅雷未返回上传会话信息")?;
        let file = value.get("file").cloned().unwrap_or_else(|| json!({}));
        let id = text(&file["id"]);
        Ok((id, params))
    }

    /// 单次整对象直传（大文件分片失败时的回退路径）。
    pub async fn xunlei_put_object(
        &self,
        params: &Value,
        path: &Path,
        size: u64,
        progress: &std::sync::Arc<std::sync::atomic::AtomicU64>,
    ) -> Result<(), String> {
        self.put_object(params, path, size, progress).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn xunlei_mkdir(&self, parent_id: &str, name: &str) -> Result<DriveFile, String> {
        let captcha = self.captcha("POST:/drive/v1/files").await?;
        let body = json!({
            "kind": "drive#folder",
            "parent_id": if parent_id == "0" { "" } else { parent_id },
            "name": name,
        });
        let value = self.api_post("/files", &body, &captcha).await?;
        let file = value.get("file").cloned().unwrap_or_else(|| value.clone());
        let id = text(&file["id"]);
        if id.is_empty() {
            return Err("迅雷创建文件夹失败，请刷新网盘确认".into());
        }
        Ok(DriveFile {
            id,
            path: String::new(),
            name: name.into(),
            size: 0,
            is_dir: true,
            modified: now(),
            created: now(),
            md5: String::new(),
        })
    }

    pub async fn xunlei_rename(
        &self,
        file: &DriveFile,
        new_name: &str,
    ) -> Result<DriveFile, String> {
        let captcha = self.captcha("PATCH:/drive/v1/files").await?;
        self.api_send(
            reqwest::Method::PATCH,
            &format!("/files/{}", file.id),
            Some(&json!({ "name": new_name })),
            &captcha,
        )
        .await?;
        Ok(DriveFile {
            id: file.id.clone(),
            path: file.path.clone(),
            name: new_name.into(),
            size: file.size,
            is_dir: file.is_dir,
            modified: now(),
            created: file.created,
            md5: file.md5.clone(),
        })
    }

    pub async fn xunlei_delete(&self, files: &[DriveFile]) -> Result<(), String> {
        if files.is_empty() {
            return Err("请选择要删除的文件".into());
        }
        for file in files {
            let id = identifier_component(&file.id)?;
            let captcha = self.captcha("PATCH:/drive/v1/files").await?;
            self.api_send(
                reqwest::Method::PATCH,
                &format!("/files/{id}/trash?space="),
                Some(&json!({})),
                &captcha,
            )
            .await?;
        }
        Ok(())
    }

    /// Create a share link for personal-drive entries (OpenList thunder `POST /share`).
    pub async fn xunlei_create_share(
        &self,
        files: &[DriveFile],
        period: u32,
        _password: &str,
    ) -> Result<CreatedShare, String> {
        if files.is_empty() {
            return Err("请选择要分享的文件".into());
        }
        let names: Vec<String> = files.iter().map(|f| f.name.clone()).collect();
        let title = if names.len() == 1 {
            names[0].clone()
        } else {
            format!("{} 等 {} 个文件", names[0], names.len())
        };
        let body = json!({
            "expiration_days": if period == 0 { "-1".to_string() } else { period.to_string() },
            "file_ids": files.iter().map(|f| f.id.clone()).collect::<Vec<_>>(),
            "params": { "subscribe_push": "false", "withPassCodeInLink": "true" },
            "restore_limit": "-1",
            "share_to": "copy",
            "title": title,
        });
        let captcha = self.captcha("POST:/drive/v1/share").await?;
        let value = self
            .api_send(reqwest::Method::POST, "/share", Some(&body), &captcha)
            .await?;
        if value["share_error_files"]
            .as_array()
            .is_some_and(|files| !files.is_empty())
        {
            return Err("部分文件分享失败，请稍后重试".into());
        }
        let url = text(&value["share_url"]);
        let passcode = text(&value["pass_code"]);
        if url.is_empty() {
            return Err("创建分享失败，请稍后重试".into());
        }
        Ok(CreatedShare {
            id: text(&value["share_id"]),
            url,
            password: passcode,
            period,
            created_at: now(),
            account_id: self.cloud_identity().await?.0,
            names,
            cancelled: false,
        })
    }

    async fn put_object(
        &self,
        params: &Value,
        path: &Path,
        size: u64,
        progress: &std::sync::Arc<std::sync::atomic::AtomicU64>,
    ) -> Result<(), String> {
        let bucket = text(&params["bucket"]);
        let key = text(&params["key"]);
        let endpoint = text(&params["endpoint"]);
        let access = text(&params["access_key_id"]);
        let secret = text(&params["access_key_secret"]);
        let security_token = text(&params["security_token"]);
        if bucket.is_empty() || key.is_empty() || endpoint.is_empty() {
            return Err("迅雷上传会话缺少必要参数".into());
        }
        let base = endpoint
            .trim_start_matches("https://")
            .trim_start_matches("http://");
        let base = base.trim_start_matches(&format!("{bucket}."));
        let base = base.trim_end_matches('/');
        let host = format!("{bucket}.{base}");
        let canonical_uri = format!("/{}", key.trim_start_matches('/'));
        let url = format!("https://{host}{canonical_uri}");

        let date = chrono::Utc::now()
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();
        let content_type = "application/octet-stream";
        let canonical_oss = if security_token.is_empty() {
            String::new()
        } else {
            format!("x-oss-security-token:{security_token}\n")
        };
        let canonical_resource = format!("/{bucket}/{}", key.trim_start_matches('/'));
        let string_to_sign =
            format!("PUT\n\n{content_type}\n{date}\n{canonical_oss}{canonical_resource}");
        let signature = base64::engine::general_purpose::STANDARD
            .encode(hmac_sha1(secret.as_bytes(), string_to_sign.as_bytes()));
        let authorization = format!("OSS {access}:{signature}");

        let meta = tokio::fs::metadata(path)
            .await
            .map_err(|_| "读取上传文件失败")?;
        if meta.len() != size {
            return Err("本地文件已修改，请移除该任务并重新选择文件".into());
        }
        let counter = progress.clone();
        let file = tokio::fs::File::open(path)
            .await
            .map_err(|_| "读取上传文件失败")?;
        let body_stream = futures_util::stream::unfold(file, move |mut file| {
            let counter = counter.clone();
            async move {
                use tokio::io::AsyncReadExt;
                let mut buf = vec![0u8; 256 * 1024];
                match file.read(&mut buf).await {
                    Ok(0) => None,
                    Ok(n) => {
                        buf.truncate(n);
                        counter.fetch_add(n as u64, Ordering::Relaxed);
                        Some((Ok::<Vec<u8>, std::io::Error>(buf), file))
                    }
                    Err(e) => Some((Err(e), file)),
                }
            }
        });
        let body = reqwest::Body::wrap_stream(body_stream);
        let mut request = self
            .transport
            .client
            .put(&url)
            .header("Date", &date)
            .header("Content-Type", content_type)
            .header("Content-Length", size.to_string())
            .header("Authorization", authorization)
            .body(body)
            // 上传大文件可能远超客户端默认的 30 秒总超时，这里按请求放宽。
            .timeout(std::time::Duration::from_secs(3600));
        if !security_token.is_empty() {
            request = request.header("x-oss-security-token", &security_token);
        }
        let response = request
            .send()
            .await
            .map_err(|e| format!("连接迅雷云端存储失败，请稍后重试（{e}）"))?;
        if !response.status().is_success() {
            return Err(format!(
                "迅雷云端存储上传失败（HTTP {}），请稍后重试",
                response.status().as_u16()
            ));
        }
        Ok(())
    }

    // ---- OSS 分片上传（大文件续传）----

    fn oss_target(&self, params: &Value) -> Result<(String, String, String), String> {
        let bucket = text(&params["bucket"]);
        let key = text(&params["key"]);
        let endpoint = text(&params["endpoint"]);
        if bucket.is_empty() || key.is_empty() || endpoint.is_empty() {
            return Err("迅雷上传会话缺少必要参数".into());
        }
        let base = endpoint
            .trim_start_matches("https://")
            .trim_start_matches("http://");
        let base = base.trim_start_matches(&format!("{bucket}."));
        let base = base.trim_end_matches('/');
        Ok((bucket.clone(), key, format!("{bucket}.{base}")))
    }

    fn oss_authorization(
        &self,
        params: &Value,
        method: &str,
        content_type: &str,
        content_md5: &str,
        date: &str,
        resource: &str,
    ) -> String {
        let access = text(&params["access_key_id"]);
        let secret = text(&params["access_key_secret"]);
        let security_token = text(&params["security_token"]);
        let oss_headers = if security_token.is_empty() {
            String::new()
        } else {
            format!("x-oss-security-token:{security_token}\n")
        };
        let string_to_sign =
            format!("{method}\n{content_md5}\n{content_type}\n{date}\n{oss_headers}{resource}");
        let signature = base64::engine::general_purpose::STANDARD
            .encode(hmac_sha1(secret.as_bytes(), string_to_sign.as_bytes()));
        format!("OSS {access}:{signature}")
    }

    pub async fn xunlei_oss_initiate(&self, params: &Value) -> Result<String, String> {
        let (bucket, key, host) = self.oss_target(params)?;
        let date = chrono::Utc::now()
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();
        let put_key = key.trim_start_matches('/');
        let resource = format!("/{bucket}/{put_key}?uploads");
        let authorization = self.oss_authorization(params, "POST", "", "", &date, &resource);
        let security_token = text(&params["security_token"]);
        let mut request = self
            .transport
            .client
            .post(format!("https://{host}/{put_key}?uploads"))
            .header("Date", &date)
            .header("Authorization", &authorization)
            .timeout(std::time::Duration::from_secs(120));
        if !security_token.is_empty() {
            request = request.header("x-oss-security-token", &security_token);
        }
        let response = request
            .send()
            .await
            .map_err(|e| format!("迅雷创建分片上传失败（{e}）"))?;
        if !response.status().is_success() {
            return Err(format!(
                "迅雷创建分片上传失败（HTTP {}）",
                response.status().as_u16()
            ));
        }
        let body = response
            .text()
            .await
            .map_err(|_| "迅雷分片上传响应读取失败")?;
        extract_xml_tag(&body, "UploadId").ok_or_else(|| "迅雷未返回分片上传标识".to_string())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn xunlei_oss_put_part(
        &self,
        params: &Value,
        upload_id: &str,
        part_number: u32,
        path: &Path,
        offset: u64,
        len: u64,
        progress: &std::sync::Arc<std::sync::atomic::AtomicU64>,
    ) -> Result<String, String> {
        let (bucket, key, host) = self.oss_target(params)?;
        let put_key = key.trim_start_matches('/');
        let date = chrono::Utc::now()
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();
        let content_type = "application/octet-stream";
        let resource = format!("/{bucket}/{put_key}?partNumber={part_number}&uploadId={upload_id}");
        let authorization =
            self.oss_authorization(params, "PUT", content_type, "", &date, &resource);
        let security_token = text(&params["security_token"]);
        let counter = progress.clone();
        let mut file = tokio::fs::File::open(path)
            .await
            .map_err(|_| "读取上传文件失败")?;
        {
            use tokio::io::AsyncSeekExt;
            file.seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(|_| "定位上传文件失败")?;
        }
        let body_stream =
            futures_util::stream::unfold((file, len), move |(mut file, remaining)| {
                let counter = counter.clone();
                async move {
                    use tokio::io::AsyncReadExt;
                    if remaining == 0 {
                        return None;
                    }
                    let want = (256 * 1024).min(remaining as usize);
                    let mut buf = vec![0u8; want];
                    match file.read(&mut buf).await {
                        Ok(0) => None,
                        Ok(n) => {
                            buf.truncate(n);
                            counter.fetch_add(n as u64, Ordering::Relaxed);
                            Some((
                                Ok::<Vec<u8>, std::io::Error>(buf),
                                (file, remaining - n as u64),
                            ))
                        }
                        Err(e) => Some((Err(e), (file, 0))),
                    }
                }
            });
        let body = reqwest::Body::wrap_stream(body_stream);
        let mut request = self
            .transport
            .client
            .put(format!(
                "https://{host}/{put_key}?partNumber={part_number}&uploadId={upload_id}"
            ))
            .header("Date", &date)
            .header("Content-Type", content_type)
            .header("Content-Length", len.to_string())
            .header("Authorization", &authorization)
            .body(body)
            .timeout(std::time::Duration::from_secs(3600));
        if !security_token.is_empty() {
            request = request.header("x-oss-security-token", &security_token);
        }
        let response = request
            .send()
            .await
            .map_err(|e| format!("连接迅雷云端存储失败，请稍后重试（{e}）"))?;
        if !response.status().is_success() {
            return Err(format!(
                "迅雷分片上传失败（HTTP {}）",
                response.status().as_u16()
            ));
        }
        let etag = response
            .headers()
            .get("etag")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        if etag.is_empty() {
            return Err("迅雷分片上传未返回 ETag".into());
        }
        Ok(etag)
    }

    pub async fn xunlei_oss_complete(
        &self,
        params: &Value,
        upload_id: &str,
        parts: &[(u32, String)],
    ) -> Result<(), String> {
        let (bucket, key, host) = self.oss_target(params)?;
        let put_key = key.trim_start_matches('/');
        let date = chrono::Utc::now()
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();
        let mut xml = String::from("<CompleteMultipartUpload>");
        for (number, etag) in parts {
            xml.push_str(&format!(
                "<Part><PartNumber>{number}</PartNumber><ETag>\"{etag}\"</ETag></Part>"
            ));
        }
        xml.push_str("</CompleteMultipartUpload>");
        let content_type = "application/xml";
        let content_md5 =
            base64::engine::general_purpose::STANDARD.encode(md5::Md5::digest(xml.as_bytes()));
        let resource = format!("/{bucket}/{put_key}?uploadId={upload_id}");
        let authorization =
            self.oss_authorization(params, "POST", content_type, &content_md5, &date, &resource);
        let security_token = text(&params["security_token"]);
        let mut request = self
            .transport
            .client
            .post(format!("https://{host}/{put_key}?uploadId={upload_id}"))
            .header("Date", &date)
            .header("Content-Type", content_type)
            .header("Content-MD5", &content_md5)
            .header("Authorization", &authorization)
            .body(xml)
            .timeout(std::time::Duration::from_secs(600));
        if !security_token.is_empty() {
            request = request.header("x-oss-security-token", &security_token);
        }
        let response = request
            .send()
            .await
            .map_err(|e| format!("迅雷分片合并失败，请稍后重试（{e}）"))?;
        if !response.status().is_success() {
            return Err(format!(
                "迅雷分片合并失败（HTTP {}）",
                response.status().as_u16()
            ));
        }
        Ok(())
    }
}

fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].to_string())
}

fn identifier_component(value: &str) -> Result<&str, String> {
    super::cloud::identifier(value)
}
