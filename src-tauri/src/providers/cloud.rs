use super::{crypto, number, text, Download, Session};
use crate::model::*;
use base64::Engine as _;
use md5::{Digest, Md5};
use reqwest::cookie::CookieStore;
use serde::Serialize;
use serde_json::{json, Value};
use sha1::Sha1;

pub struct Entry {
    pub file: DriveFile,
    pub metadata: Value,
}
pub struct Page {
    pub entries: Vec<Entry>,
    pub next: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub account_id: String,
    pub path: String,
    pub files: Vec<DriveFile>,
    pub next_cursor: Option<String>,
}

/// 网盘容量快照（字节）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capacity {
    pub total: u64,
    pub used: u64,
}

impl Capacity {
    pub fn free(&self) -> u64 {
        self.total.saturating_sub(self.used)
    }
    /// 还差多少字节才放得下 `needed`；返回 0 表示空间够用。
    pub fn shortfall(&self, needed: u64) -> u64 {
        needed.saturating_sub(self.free())
    }
}

/// 解析容量字段。夸克/UC 放在 `data` 里，迅雷放在 `quota` 里，百度直接放顶层，
/// 键名也各不相同，所以按「三个作用域 × 一组候选键」取第一个大于 0 的值。
/// 总容量缺失时判为接口无效，避免拿 0 误拦上传。
pub fn capacity(value: &Value) -> Result<Capacity, String> {
    let scopes = [value.get("data"), value.get("quota"), Some(value)];
    let pick = |keys: [&str; 4]| {
        scopes
            .iter()
            .flatten()
            .flat_map(|scope| keys.iter().map(|key| number(&scope[*key])))
            .find(|value| *value > 0)
    };
    let total = pick(["total_capacity", "capacity", "total", "limit"])
        .ok_or("网盘未返回总容量，无法预检空间")?;
    let used = pick(["use_capacity", "used_capacity", "used", "usage"]).unwrap_or(0);
    Ok(Capacity { total, used })
}

/// 面向界面的容量文案：影响面只有错误提示，用 1024 进制并保留一位小数。
pub fn human_size(bytes: u64) -> String {
    const MIB: f64 = 1024.0 * 1024.0;
    const GIB: f64 = 1024.0 * MIB;
    let value = bytes as f64;
    if value >= GIB {
        format!("{:.2} GB", value / GIB)
    } else {
        format!("{:.1} MB", value / MIB)
    }
}

pub fn check(provider: Provider, value: Value) -> Result<Value, String> {
    let valid = match provider {
        Provider::Quark | Provider::Uc => value["code"].as_i64() == Some(0),
        Provider::Pan115 => value["state"].as_bool() == Some(true),
        Provider::Pan123 => matches!(value["code"].as_i64(), Some(0 | 200)),
        Provider::C139 => {
            value["success"].as_bool() != Some(false)
                && ["0", "0000"].contains(
                    &text(if value.get("resultCode").is_some() {
                        &value["resultCode"]
                    } else {
                        &value["code"]
                    })
                    .as_str(),
                )
        }
        Provider::Xunlei => {
            value
                .get("error")
                .is_none_or(|error| error.is_null() || error.as_str() == Some(""))
                && value
                    .get("error_code")
                    .is_none_or(|code| code.as_i64() == Some(0))
        }
        _ => false,
    };
    if valid {
        return Ok(value);
    }
    let code = ["errno", "errNo", "code", "resultCode"]
        .iter()
        .map(|key| text(&value[*key]))
        .find(|v| !v.is_empty())
        .unwrap_or_else(|| "unknown".into());
    let code: String = code
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .take(24)
        .collect();
    if provider == Provider::Uc {
        match code.as_str() {
            "31001" => return Err("该 UC 分享需要先登录账号才能下载".into()),
            "23018" => return Err("该 UC 文件超出游客下载大小上限，请先登录账号".into()),
            "14001" => return Err("UC 分享已失效或提取码有误，请重新解析".into()),
            "41020" => return Err("UC 文件令牌已过期，请重新解析分享".into()),
            _ => (),
        }
    }
    if provider == Provider::Pan115 && code == "990001" {
        return Err("115 登录会话无效（错误码 990001），请在官方窗口重新登录后刷新登录状态".into());
    }
    if matches!(provider, Provider::Quark | Provider::Uc) {
        // 官方 SDK 文档：32003 主盘空间不足，32004 单文件超过大小上限。
        match code.as_str() {
            "32003" => {
                return Err(format!(
                    "{}主盘空间不足（错误码 32003），请清理网盘空间并清空回收站后再上传",
                    provider.name()
                ))
            }
            "32004" => {
                return Err(format!(
                    "{}单个文件超过大小上限（错误码 32004），请压缩或拆分后再上传",
                    provider.name()
                ))
            }
            _ => (),
        }
    }
    Err(format!(
        "{}请求未成功（错误码 {}），请检查登录状态、提取码、会员权限或官方验证提示",
        provider.name(),
        code
    ))
}
pub fn identifier(value: &str) -> Result<&str, String> {
    if value.is_empty()
        || value.len() > 256
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        || matches!(value, "." | "..")
    {
        return Err("网盘文件标识无效，请刷新列表".into());
    }
    Ok(value)
}
pub fn file_identifier(provider: Provider, value: &str) -> Result<&str, String> {
    // 139 system folders use IDs such as "/app-folder". Keep the provider ID
    // intact for API calls and encode its leading slash inside our UI path.
    let plain = if provider == Provider::C139 {
        value.strip_prefix('/').unwrap_or(value)
    } else {
        value
    };
    identifier(plain)?;
    Ok(value)
}
fn path_component(provider: Provider, value: &str) -> Result<String, String> {
    file_identifier(provider, value)?;
    Ok(if provider == Provider::C139 && value.starts_with('/') {
        format!("%2F{}", &value[1..])
    } else {
        value.into()
    })
}
fn component_id(provider: Provider, value: &str) -> Result<String, String> {
    let decoded =
        if provider == Provider::C139 && (value.starts_with("%2F") || value.starts_with("%2f")) {
            format!("/{}", &value[3..])
        } else {
            value.into()
        };
    file_identifier(provider, &decoded)?;
    Ok(decoded)
}
pub fn parent_id(path: &str, provider: Provider) -> Result<String, String> {
    if !path.starts_with('/') || path.len() > 8192 {
        return Err("网盘路径无效".into());
    }
    for part in path.split('/').filter(|part| !part.is_empty()) {
        component_id(provider, part)?;
    }
    let last = path.rsplit('/').find(|part| !part.is_empty()).unwrap_or("");
    if last.is_empty() {
        Ok(if provider == Provider::C139 { "/" } else { "0" }.into())
    } else {
        component_id(provider, last)
    }
}
/// Convert a provider timestamp (epoch seconds/milliseconds number, or an
/// RFC 3339 / "YYYY-MM-DD HH:MM:SS" string) into epoch seconds. Returns 0 when
/// the value is missing or cannot be parsed, so callers can use it as a
/// best-effort fallback without failing the whole listing.
pub fn timestamp_seconds(raw: &Value) -> u64 {
    if let Some(value) = raw.as_u64() {
        return if value > 1_000_000_000_000 {
            value / 1000
        } else {
            value
        };
    }
    let Some(text) = raw.as_str() else {
        return 0;
    };
    let text = text.trim();
    if text.is_empty() {
        return 0;
    }
    if let Ok(value) = chrono::DateTime::parse_from_rfc3339(text) {
        return value.timestamp().max(0) as u64;
    }
    for pattern in [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y/%m/%d %H:%M:%S",
        "%Y-%m-%d",
        "%Y/%m/%d",
    ] {
        if let Ok(value) = chrono::NaiveDateTime::parse_from_str(text, pattern) {
            return value.and_utc().timestamp().max(0) as u64;
        }
        if let Ok(value) = chrono::NaiveDate::parse_from_str(text, pattern) {
            return value
                .and_hms_opt(0, 0, 0)
                .map(|dt| dt.and_utc().timestamp().max(0) as u64)
                .unwrap_or(0);
        }
    }
    0
}

/// Pick the first key present in `raw` whose value parses to a real timestamp.
pub fn entry_timestamp(raw: &Value, keys: &[&str]) -> u64 {
    for key in keys {
        let seconds = timestamp_seconds(&raw[*key]);
        if seconds > 0 {
            return seconds;
        }
    }
    0
}

pub fn entry(provider: Provider, raw: &Value, parent: &str) -> Result<Entry, String> {
    let (id, name, size, is_dir, modified, hash) = match provider {
        Provider::Quark | Provider::Uc => (
            text(&raw["fid"]),
            text(&raw["file_name"]),
            number(&raw["size"]),
            raw["dir"].as_bool().unwrap_or(false),
            number(&raw["updated_at"]) / 1000,
            text(&raw["md5"]),
        ),
        Provider::Pan115 => {
            let file = text(&raw["fid"]);
            let dir = file.is_empty();
            (
                if dir { text(&raw["cid"]) } else { file },
                text(&raw["n"]),
                number(&raw["s"]),
                dir,
                number(&raw["t"]),
                text(&raw["sha"]),
            )
        }
        Provider::Pan123 => (
            text(&raw["FileId"]),
            text(&raw["FileName"]),
            number(&raw["Size"]),
            number(&raw["Type"]) == 1,
            entry_timestamp(raw, &["UpdateAt", "CreateAt"]),
            text(&raw["Etag"]),
        ),
        Provider::C139 => (
            text(&raw["fileId"]),
            text(&raw["name"]),
            number(&raw["size"]),
            raw["type"].as_str() == Some("folder"),
            entry_timestamp(raw, &["updateTime", "createTime", "modifiedTime"]),
            text(&raw["md5"]),
        ),
        Provider::Xunlei => (
            text(&raw["id"]),
            text(&raw["name"]),
            number(&raw["size"]),
            text(&raw["kind"]) == "drive#folder",
            entry_timestamp(raw, &["modified_time", "created_time"]),
            text(&raw["hash"]),
        ),
        _ => return Err("此网盘使用专用文件接口".into()),
    };
    let created = match provider {
        Provider::Quark | Provider::Uc => {
            let c = number(&raw["created_at"]) / 1000;
            if c > 0 {
                c
            } else {
                modified
            }
        }
        Provider::Pan115 => {
            let c = number(&raw["ctime"]);
            if c > 0 {
                c
            } else {
                modified
            }
        }
        Provider::Pan123 => {
            let c = entry_timestamp(raw, &["CreateAt"]);
            if c > 0 {
                c
            } else {
                modified
            }
        }
        Provider::C139 => {
            let c = entry_timestamp(raw, &["createTime"]);
            if c > 0 {
                c
            } else {
                modified
            }
        }
        Provider::Xunlei => {
            let c = entry_timestamp(raw, &["created_time"]);
            if c > 0 {
                c
            } else {
                modified
            }
        }
        _ => modified,
    };
    let component = path_component(provider, &id)?;
    if name.is_empty() {
        return Err("网盘文件名称缺失，请刷新列表".into());
    }
    Ok(Entry {
        file: DriveFile {
            id: id.clone(),
            path: format!("{}/{}", parent.trim_end_matches('/'), component),
            name,
            size,
            is_dir,
            modified,
            created,
            md5: hash,
        },
        metadata: raw.clone(),
    })
}
impl Session {
    pub fn cookie_value(&self, origins: &[&str], key: &str) -> Option<String> {
        self.cookie_value_raw(origins, key).map(|v| {
            percent_encoding::percent_decode_str(&v)
                .decode_utf8_lossy()
                .into_owned()
        })
    }
    pub fn cookie_value_raw(&self, origins: &[&str], key: &str) -> Option<String> {
        origins
            .iter()
            .filter_map(|origin| origin.parse().ok())
            .filter_map(|url| self.transport.jar.cookies(&url))
            .filter_map(|header| header.to_str().ok().map(String::from))
            .flat_map(|value| {
                value
                    .split(';')
                    .map(str::trim)
                    .map(String::from)
                    .collect::<Vec<_>>()
            })
            .find_map(|pair| {
                pair.split_once('=')
                    .filter(|(k, _)| *k == key)
                    .map(|(_, v)| v.to_string())
            })
    }
    pub async fn cloud_identity(&self) -> Result<(String, String), String> {
        let (id, name) = match self.provider {
            Provider::Quark => {
                let value = self
                    .json(
                        self.transport
                            .client
                            .get(self.endpoint("https://pan.quark.cn/account/info"))
                            .query(&[("fr", "pc"), ("platform", "pc")])
                            .header("Referer", "https://pan.quark.cn/"),
                    )
                    .await?;
                if value["success"].as_bool() != Some(true) {
                    return Err("请先登录夸克网盘".into());
                }
                let data = &value["data"];
                let id = ["uid", "qid", "user_id", "userId", "ukey"]
                    .iter()
                    .map(|key| text(&data[*key]))
                    .find(|id| !id.is_empty())
                    .or_else(|| self.cookie_value(&["https://pan.quark.cn/"], "__uid"))
                    .ok_or("未取得夸克账号标识，请在官方窗口重新登录")?;
                (
                    id,
                    ["nickname", "nick_name", "nickName"]
                        .iter()
                        .map(|key| text(&data[*key]))
                        .find(|v| !v.is_empty())
                        .unwrap_or_else(|| "夸克账号".into()),
                )
            }
            Provider::Pan115 => self.pan115_identity().await?,
            Provider::Pan123 => self.pan123_identity().await?,
            Provider::C139 => self.c139_identity().await?,
            Provider::Uc => self.uc_identity().await?,
            Provider::Lanzou => self.lanzou_identity().await?,
            Provider::Ilanzou => self.ilanzou_identity().await?,
            Provider::Xunlei => self.xunlei_identity().await?,
            _ => return Err("此网盘使用专用文件接口".into()),
        };
        Ok((
            crypto::md5_hex(&format!("{}:{id}", self.provider.key())),
            name,
        ))
    }
    /// 上传前的空间预检。夸克/UC 的 member 接口返回总容量与已用容量，
    /// 上传前先比对一次，就不会出现「传了一半才报 32003」。
    pub async fn cloud_capacity(&self) -> Result<Capacity, String> {
        let value = match self.provider {
            Provider::Quark => check(
                self.provider,
                self.json(
                    self.transport
                        .client
                        .get(self.endpoint("https://pan.quark.cn/1/clouddrive/member"))
                        .query(&[("pr", "ucpro"), ("fr", "pc")])
                        .header("Referer", "https://pan.quark.cn/"),
                )
                .await?,
            )?,
            Provider::Uc => check(
                self.provider,
                self.json(
                    self.transport
                        .client
                        .get(self.endpoint("https://pc-api.uc.cn/1/clouddrive/member"))
                        .query(&[("pr", "UCBrowser"), ("fr", "pc")])
                        .header("Referer", "https://drive.uc.cn/"),
                )
                .await?,
            )?,
            _ => return Err("此网盘暂不支持容量查询".into()),
        };
        capacity(&value)
    }
    pub async fn cloud_page(
        &self,
        path: &str,
        page: u32,
        cursor: Option<&str>,
    ) -> Result<Page, String> {
        if page == 0 || page > 200 || cursor.is_some_and(|v| v.len() > 8192) {
            return Err("网盘分页参数无效".into());
        }
        let id = parent_id(path, self.provider)?;
        match self.provider {
            Provider::Quark => {
                let value = check(
                    self.provider,
                    self.json(
                        self.transport
                            .client
                            .get(self.endpoint("https://drive-pc.quark.cn/1/clouddrive/file/sort"))
                            .query(&[
                                ("pr", "ucpro".into()),
                                ("fr", "pc".into()),
                                ("pdir_fid", id),
                                ("_page", page.to_string()),
                                ("_size", "100".into()),
                                ("_fetch_total", "1".into()),
                                ("_sort", "file_type:asc,updated_at:desc".into()),
                            ])
                            .header("Referer", "https://pan.quark.cn/"),
                    )
                    .await?,
                )?;
                let rows = value["data"]["list"].as_array().ok_or("夸克文件列表无效")?;
                let entries = rows
                    .iter()
                    .map(|raw| entry(self.provider, raw, path))
                    .collect::<Result<Vec<_>, _>>()?;
                let more = rows.len() == 100
                    && (number(&value["metadata"]["_total"]) == 0
                        || number(&value["metadata"]["_total"]) > page as u64 * 100);
                Ok(Page {
                    entries,
                    next: more.then(|| page.to_string()),
                })
            }
            Provider::Pan115 => self.pan115_page(path, &id, page).await,
            Provider::Pan123 => self.pan123_page(path, &id, page, cursor).await,
            Provider::C139 => self.c139_page(path, &id, cursor).await,
            Provider::Uc => self.uc_page(path, &id, page).await,
            Provider::Lanzou => self.lanzou_page(path, &id, page).await,
            Provider::Ilanzou => self.ilanzou_page(path, &id, page).await,
            Provider::Xunlei => self.xunlei_page(path, &id, page, cursor).await,
            _ => Err("此网盘使用专用文件接口".into()),
        }
    }
    pub async fn cloud_find(&self, file: &DriveFile) -> Result<Entry, String> {
        file_identifier(self.provider, &file.id)?;
        if parent_id(&file.path, self.provider)? != file.id {
            return Err("文件标识与路径不一致，请刷新列表".into());
        }
        let parent = file
            .path
            .rsplit_once('/')
            .map(|(parent, _)| if parent.is_empty() { "/" } else { parent })
            .ok_or("文件路径无效")?;
        let mut cursor = None;
        for page in 1..=200 {
            let batch = self.cloud_page(parent, page, cursor.as_deref()).await?;
            if let Some(entry) = batch
                .entries
                .into_iter()
                .find(|entry| entry.file.id == file.id)
            {
                return Ok(entry);
            }
            cursor = batch.next;
            if cursor.is_none() {
                break;
            }
        }
        Err("网盘文件不存在或已被移动，请刷新列表".into())
    }
    pub async fn cloud_download(&self, source: &OwnSource) -> Result<Download, String> {
        if self.cloud_identity().await?.0 != source.account_id {
            return Err("网盘账号已切换，请使用创建任务时的账号继续下载".into());
        }
        let fresh = self.cloud_find(&source.file).await?;
        if fresh.file.is_dir
            || fresh.file.size != source.file.size
            || (!source.file.md5.is_empty() && fresh.file.md5 != source.file.md5)
        {
            return Err("源文件已变化，请刷新并新建下载任务".into());
        }
        let url = match self.provider {
            Provider::Quark => {
                let value =
                    check(
                        self.provider,
                        self.json(
                            self.transport
                                .client
                                .post(self.endpoint(
                                    "https://drive-pc.quark.cn/1/clouddrive/file/download",
                                ))
                                .query(&[
                                    ("pr", "ucpro"),
                                    ("fr", "pc"),
                                    ("sys", "win32"),
                                    ("ve", "6.9.7.761"),
                                ])
                                .header("Referer", "https://pan.quark.cn/")
                                .header("User-Agent", crate::quark::DOWNLOAD_AGENT)
                                .json(&json!({"fids":[fresh.file.id]})),
                        )
                        .await?,
                    )?;
                text(&value["data"][0]["download_url"])
            }
            Provider::Pan115 => return self.pan115_personal_download(&fresh.metadata).await,
            Provider::Pan123 => {
                let file = ShareFile {
                    id: fresh.file.id,
                    name: fresh.file.name,
                    size: fresh.file.size,
                    is_dir: false,
                    parent_id: String::new(),
                    token: fresh.metadata.to_string(),
                };
                self.pan123_download(&file, None).await?
            }
            Provider::C139 => self.c139_personal_download(&fresh.file.id).await?,
            Provider::Uc => return self.uc_download(&fresh.file.id, None).await,
            Provider::Lanzou => return self.lanzou_personal_download(&fresh.file.id).await,
            Provider::Ilanzou => self.ilanzou_download(&fresh.file.id, None).await?,
            Provider::Xunlei => self.xunlei_personal_download(&fresh.file.id).await?,
            _ => return Err("此网盘使用专用文件接口".into()),
        };
        self.download_descriptor(url)
    }

    /// Create a folder under `parent` (a UI path) and return the new entry.
    pub async fn cloud_mkdir(&self, parent: &str, name: &str) -> Result<DriveFile, String> {
        let parent_id = parent_id(parent, self.provider)?;
        match self.provider {
            Provider::Quark => {
                let value = check(
                    self.provider,
                    self.json(
                        self.transport
                            .client
                            .post(self.endpoint("https://drive-pc.quark.cn/1/clouddrive/file"))
                            .query(&[("pr", "ucpro".to_string()), ("fr", "pc".to_string())])
                            .header("Referer", "https://pan.quark.cn/")
                            .json(&json!({
                                "pdir_fid": parent_id,
                                "file_name": name,
                                "dir_path": "",
                                "dir_init_lock": false,
                            })),
                    )
                    .await?,
                )?;
                let id = text(&value["data"]["fid"]);
                if id.is_empty() {
                    return Err("创建文件夹失败，请刷新网盘确认".into());
                }
                Ok(DriveFile {
                    id: id.clone(),
                    path: format!(
                        "{}/{}",
                        parent.trim_end_matches('/'),
                        path_component(self.provider, &id)?
                    ),
                    name: name.into(),
                    size: 0,
                    is_dir: true,
                    modified: now(),
                    created: now(),
                    md5: String::new(),
                })
            }
            _ => Err("该网盘暂不支持新建文件夹".into()),
        }
    }

    /// Rename a personal-drive entry in place. The entry id (and therefore the
    /// id-based UI path) is preserved; only the display name changes.
    pub async fn cloud_rename(
        &self,
        file: &DriveFile,
        new_name: &str,
    ) -> Result<DriveFile, String> {
        file_identifier(self.provider, &file.id)?;
        match self.provider {
            Provider::Quark => {
                check(
                    self.provider,
                    self.json(
                        self.transport
                            .client
                            .post(
                                self.endpoint("https://drive-pc.quark.cn/1/clouddrive/file/rename"),
                            )
                            .query(&[("pr", "ucpro".to_string()), ("fr", "pc".to_string())])
                            .header("Referer", "https://pan.quark.cn/")
                            .json(&json!({"fid": file.id, "file_name": new_name})),
                    )
                    .await?,
                )?;
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
            _ => Err("该网盘暂不支持重命名".into()),
        }
    }

    /// Delete personal-drive entries in one batch.
    pub async fn cloud_delete(&self, files: &[DriveFile]) -> Result<(), String> {
        if files.is_empty() {
            return Err("请选择要删除的文件".into());
        }
        for file in files {
            file_identifier(self.provider, &file.id)?;
        }
        match self.provider {
            Provider::Quark => {
                let ids = files.iter().map(|f| f.id.clone()).collect::<Vec<_>>();
                check(
                    self.provider,
                    self.json(
                        self.transport
                            .client
                            .post(
                                self.endpoint("https://drive-pc.quark.cn/1/clouddrive/file/delete"),
                            )
                            .query(&[("pr", "ucpro".to_string()), ("fr", "pc".to_string())])
                            .header("Referer", "https://pan.quark.cn/")
                            .json(&json!({
                                "action_type": 2,
                                "filelist": ids,
                                "exclude_fids": [],
                            })),
                    )
                    .await?,
                )?;
                Ok(())
            }
            _ => Err("该网盘暂不支持删除文件".into()),
        }
    }

    /// Create a share link for the given personal-drive entries. Quark uses a
    /// three-step task flow (`share` -> poll `task` -> `share/password`).
    ///
    /// `code_mode` is `"auto"` for a private link whose passcode the server
    /// generates, and `"none"` for a public link without one. Quark does not
    /// accept caller-supplied passcodes.
    pub async fn cloud_share(
        &self,
        files: &[DriveFile],
        period: u32,
        code_mode: &str,
    ) -> Result<CreatedShare, String> {
        if files.is_empty() {
            return Err("请选择要分享的文件".into());
        }
        if !matches!(self.provider, Provider::Quark) {
            return Err("该网盘暂不支持创建分享".into());
        }
        if !matches!(code_mode, "auto" | "none") {
            return Err("夸克网盘的提取码由服务端生成，不支持自定义".into());
        }
        let account_id = self.cloud_identity().await?.0;
        let names: Vec<String> = files.iter().map(|f| f.name.clone()).collect();
        let title = if names.len() == 1 {
            names[0].clone()
        } else {
            format!("{} 等 {} 个文件", names[0], names.len())
        };
        let mut body = json!({
            "fid_list": files.iter().map(|f| f.id.clone()).collect::<Vec<_>>(),
            "title": title,
            "url_type": if code_mode == "auto" { 2 } else { 1 },
            "expired_type": if period == 0 { 1 } else { 2 },
        });
        if period > 0 {
            body["expired_at"] = json!(now().saturating_add(period as u64 * 86400) * 1000);
        }
        let value = check(
            self.provider,
            self.json(
                self.transport
                    .client
                    .post(self.endpoint("https://drive-pc.quark.cn/1/clouddrive/share"))
                    .query(&[("pr", "ucpro".to_string()), ("fr", "pc".to_string())])
                    .header("Referer", "https://pan.quark.cn/")
                    .json(&body),
            )
            .await?,
        )?;
        let task_id = text(&value["data"]["task_id"]);
        if task_id.is_empty() {
            return Err("创建分享任务失败，请稍后重试".into());
        }
        let mut pwd_id = String::new();
        for retry in 0..10u32 {
            let value = check(
                self.provider,
                self.json(
                    self.transport
                        .client
                        .get(self.endpoint("https://drive-pc.quark.cn/1/clouddrive/task"))
                        .query(&[
                            ("pr", "ucpro".to_string()),
                            ("fr", "pc".to_string()),
                            ("task_id", task_id.clone()),
                            ("retry_index", retry.to_string()),
                        ])
                        .header("Referer", "https://pan.quark.cn/"),
                )
                .await?,
            )?;
            match value["data"]["status"].as_i64() {
                Some(2) => {
                    pwd_id = text(&value["data"]["share_id"]);
                    break;
                }
                Some(3) => return Err("创建分享失败，请稍后重试".into()),
                _ => {
                    if retry < 9 {
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    }
                }
            }
        }
        if pwd_id.is_empty() {
            return Err("创建分享超时，请稍后重试".into());
        }
        let value = check(
            self.provider,
            self.json(
                self.transport
                    .client
                    .post(self.endpoint("https://drive-pc.quark.cn/1/clouddrive/share/password"))
                    .query(&[("pr", "ucpro".to_string()), ("fr", "pc".to_string())])
                    .header("Referer", "https://pan.quark.cn/")
                    .json(&json!({ "share_id": pwd_id })),
            )
            .await?,
        )?;
        let url = text(&value["data"]["share_url"]);
        let passcode = text(&value["data"]["passcode"]);
        if url.is_empty() {
            return Err("创建分享失败，请稍后重试".into());
        }
        Ok(CreatedShare {
            id: pwd_id,
            url,
            password: passcode,
            period,
            created_at: now(),
            account_id,
            names,
            cancelled: false,
            provider: self.provider.key().into(),
            items: Vec::new(),
            code_mode: code_mode.into(),
        })
    }

    /// List the shares this account created on the provider, newest first.
    ///
    /// The 分享记录 page keeps a local copy so a link is never lost when the
    /// network is down, but the account may also have shares created elsewhere,
    /// so the page pulls this list once and merges it in. Quark and UC expose
    /// it as `share/mypage/detail` on the share host.
    pub async fn cloud_share_list(&self) -> Result<Vec<CreatedShare>, String> {
        if !matches!(self.provider, Provider::Quark | Provider::Uc) {
            return Err("该网盘暂不支持拉取分享记录".into());
        }
        let account_id = self.cloud_identity().await?.0;
        let host = match self.provider {
            Provider::Uc => "https://drive.uc.cn/1/clouddrive",
            _ => "https://drive.quark.cn/1/clouddrive",
        };
        let value = check(
            self.provider,
            self.json(
                self.transport
                    .client
                    .get(self.endpoint(&format!("{host}/share/mypage/detail")))
                    .query(&[
                        ("pr", "ucpro".to_string()),
                        ("fr", "pc".to_string()),
                        ("_page", "1".to_string()),
                        ("_size", "100".to_string()),
                        ("_order_field", "created_at".to_string()),
                        ("_order_type", "desc".to_string()),
                        ("_fetch_total", "1".to_string()),
                    ])
                    .header("Referer", self.provider.origin()),
            )
            .await?,
        )?;
        super::debug_share("cloud", &value);
        let mut shares = Vec::new();
        for item in value["data"]["list"].as_array().into_iter().flatten() {
            let url = text(&item["share_url"]);
            if url.is_empty() {
                continue;
            }
            let created_at = timestamp_seconds(&item["created_at"]);
            let expired_at = timestamp_seconds(&item["expired_at"]);
            let period = if expired_at > created_at {
                u32::try_from((expired_at - created_at) / 86_400).unwrap_or(0)
            } else {
                0
            };
            let title = text(&item["title"]);
            // A single-file share keeps the file id with the record, which is
            // enough to create an identical share again after it was stopped.
            let first = {
                let direct = text(&item["first_fid"]);
                if direct.is_empty() {
                    text(&item["first_file"]["fid"])
                } else {
                    direct
                }
            };
            let items = if item["file_num"].as_u64() == Some(1) && !first.is_empty() {
                vec![ShareItem {
                    id: first,
                    path: String::new(),
                    name: title.clone(),
                    is_dir: false,
                }]
            } else {
                Vec::new()
            };
            shares.push(CreatedShare {
                id: text(&item["share_id"]),
                url,
                password: text(&item["passcode"]),
                period,
                created_at,
                account_id: account_id.clone(),
                names: if title.is_empty() {
                    Vec::new()
                } else {
                    vec![title]
                },
                cancelled: false,
                provider: self.provider.key().into(),
                items,
                code_mode: "auto".into(),
            });
        }
        Ok(shares)
    }

    /// Cancel one share created on the provider (`share/delete`).
    pub async fn cloud_cancel_share(&self, share: &CreatedShare) -> Result<(), String> {
        if !matches!(self.provider, Provider::Quark | Provider::Uc) {
            return Err("该网盘暂不支持取消分享".into());
        }
        let host = match self.provider {
            Provider::Uc => "https://drive.uc.cn/1/clouddrive",
            _ => "https://drive.quark.cn/1/clouddrive",
        };
        check(
            self.provider,
            self.json(
                self.transport
                    .client
                    .post(self.endpoint(&format!("{host}/share/delete")))
                    .query(&[("pr", "ucpro".to_string()), ("fr", "pc".to_string())])
                    .header("Referer", self.provider.origin())
                    .json(&json!({ "share_id": share.id })),
            )
            .await?,
        )?;
        Ok(())
    }

    /// Upload one local file into a Quark personal drive folder.
    ///
    /// Quark stores blobs on Aliyun OSS: the client asks the API for a resumable
    /// upload session, exchanges a canonical `auth_meta` string for a short-lived
    /// OSS `auth_key`, PUTs the bytes straight to `pds.quark.cn`, then asks the
    /// API to complete the multipart upload and finalise the file entry. The
    /// whole file is sent as a single part (partNumber=1), which avoids the
    /// fragile SHA1-midstate `x-oss-hash-ctx` that multi-part uploads require.
    #[allow(clippy::too_many_arguments)]
    pub async fn cloud_upload(
        &self,
        local_path: &std::path::Path,
        parent_id: &str,
        name: &str,
        remote_path: &str,
        size: u64,
        stop: &std::sync::atomic::AtomicBool,
        progress: &std::sync::Arc<std::sync::atomic::AtomicU64>,
    ) -> Result<DriveFile, String> {
        use std::sync::atomic::Ordering;
        if !matches!(self.provider, Provider::Quark) {
            return Err("该网盘暂不支持上传".into());
        }
        let meta = tokio::fs::metadata(local_path)
            .await
            .map_err(|_| "读取上传文件失败")?;
        if meta.len() != size {
            return Err("本地文件已修改，请移除该任务并重新选择文件".into());
        }
        if stop.load(Ordering::Relaxed) {
            return Err("__paused".into());
        }
        // 计算整文件 md5/sha1（阻塞读取，避免整文件常驻内存）
        let (md5, sha1) = {
            let path = local_path.to_path_buf();
            tokio::task::spawn_blocking(move || -> Result<(String, String), String> {
                use std::io::Read;
                let mut file =
                    std::fs::File::open(&path).map_err(|_| "读取上传文件失败".to_string())?;
                let mut md5 = Md5::new();
                let mut sha1 = Sha1::new();
                let mut buffer = vec![0u8; 1024 * 1024];
                loop {
                    let n = file
                        .read(&mut buffer)
                        .map_err(|_| "读取上传文件失败".to_string())?;
                    if n == 0 {
                        break;
                    }
                    md5.update(&buffer[..n]);
                    sha1.update(&buffer[..n]);
                }
                Ok((
                    format!("{:x}", md5.finalize()),
                    format!("{:x}", sha1.finalize()),
                ))
            })
            .await
            .map_err(|_| "文件校验任务失败")??
        };
        let parent = if parent_id.is_empty() { "0" } else { parent_id };
        let now_ms = now().saturating_mul(1000);
        let query = [("pr", "ucpro".to_string()), ("fr", "pc".to_string())];
        let value = check(
            self.provider,
            self.json(
                self.transport
                    .client
                    .post(self.endpoint("https://drive-pc.quark.cn/1/clouddrive/file/upload/pre"))
                    .query(&query)
                    .header("Referer", "https://pan.quark.cn/")
                    .json(&json!({
                        "ccp_hash_update": true,
                        "parallel_upload": true,
                        "pdir_fid": parent,
                        "dir_name": "",
                        "size": size,
                        "file_name": name,
                        "format_type": "application/octet-stream",
                        "l_updated_at": now_ms,
                        "l_created_at": now_ms,
                    })),
            )
            .await?,
        )?;
        let data = &value["data"];
        let task_id = text(&data["task_id"]);
        let upload_id = text(&data["upload_id"]);
        let obj_key = text(&data["obj_key"]);
        let bucket = {
            let bucket = text(&data["bucket"]);
            if bucket.is_empty() {
                "ul-zb".to_string()
            } else {
                bucket
            }
        };
        let auth_info = data["auth_info"].clone();
        let callback = data["callback"].clone();
        if task_id.is_empty() || upload_id.is_empty() || obj_key.is_empty() {
            return Err("夸克返回的上传会话无效，请稍后重试".into());
        }
        check(
            self.provider,
            self.json(
                self.transport
                    .client
                    .post(self.endpoint("https://drive-pc.quark.cn/1/clouddrive/file/update/hash"))
                    .query(&query)
                    .header("Referer", "https://pan.quark.cn/")
                    .json(&json!({ "task_id": task_id, "md5": md5, "sha1": sha1 })),
            )
            .await?,
        )?;
        let oss_date = chrono::Utc::now()
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();
        let ua = "aliyun-sdk-js/1.0.0 Chrome Mobile 139.0.0.0 on Google Nexus 5 (Android 6.0)";
        let put_key = obj_key.trim_start_matches('/');
        let auth_meta = format!(
            "PUT\n\napplication/octet-stream\n{oss_date}\nx-oss-date:{oss_date}\nx-oss-user-agent:{ua}\n/{bucket}/{put_key}?partNumber=1&uploadId={upload_id}"
        );
        let value = check(
            self.provider,
            self.json(
                self.transport
                    .client
                    .post(self.endpoint("https://drive-pc.quark.cn/1/clouddrive/file/upload/auth"))
                    .query(&query)
                    .header("Referer", "https://pan.quark.cn/")
                    .json(&json!({ "task_id": task_id, "auth_info": auth_info, "auth_meta": auth_meta })),
            )
            .await?,
        )?;
        let auth_key = text(&value["data"]["auth_key"]);
        if auth_key.is_empty() {
            return Err("夸克上传鉴权失败，请稍后重试".into());
        }
        let counter = progress.clone();
        let file = tokio::fs::File::open(local_path)
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
        let response = self
            .transport
            .client
            .put(format!(
                "https://{bucket}.pds.quark.cn/{put_key}?partNumber=1&uploadId={upload_id}"
            ))
            .header("Content-Type", "application/octet-stream")
            .header("x-oss-date", &oss_date)
            .header("x-oss-user-agent", ua)
            .header("Authorization", &auth_key)
            .body(body)
            // 上传大文件可能远超客户端默认的 30 秒总超时，这里按请求放宽。
            .timeout(std::time::Duration::from_secs(3600))
            .send()
            .await
            .map_err(|e| format!("连接夸克云端存储失败，请稍后重试（{e}）"))?;
        if !response.status().is_success() {
            return Err(format!(
                "夸克云端存储上传失败（HTTP {}），请稍后重试",
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
        let xml = format!(
            "<CompleteMultipartUpload><Part><PartNumber>1</PartNumber><ETag>\"{etag}\"</ETag></Part></CompleteMultipartUpload>"
        );
        let xml_md5 = base64::engine::general_purpose::STANDARD.encode(Md5::digest(xml.as_bytes()));
        let callback_b64 = base64::engine::general_purpose::STANDARD.encode(
            serde_json::to_string(&callback)
                .unwrap_or_default()
                .as_bytes(),
        );
        let ua2 = "aliyun-sdk-js/1.0.0 Chrome 139.0.0.0 on OS X 10.15.7 64-bit";
        let auth_meta2 = format!(
            "POST\n{xml_md5}\napplication/xml\n{oss_date}\nx-oss-callback:{callback_b64}\nx-oss-date:{oss_date}\nx-oss-user-agent:{ua2}\n/{bucket}/{put_key}?uploadId={upload_id}"
        );
        let value = check(
            self.provider,
            self.json(
                self.transport
                    .client
                    .post(self.endpoint("https://drive-pc.quark.cn/1/clouddrive/file/upload/auth"))
                    .query(&query)
                    .header("Referer", "https://pan.quark.cn/")
                    .json(&json!({ "task_id": task_id, "auth_meta": auth_meta2, "auth_info": auth_info })),
            )
            .await?,
        )?;
        let auth_key2 = text(&value["data"]["auth_key"]);
        if auth_key2.is_empty() {
            return Err("夸克上传鉴权失败，请稍后重试".into());
        }
        let response = self
            .transport
            .client
            .post(format!(
                "https://{bucket}.pds.quark.cn/{put_key}?uploadId={upload_id}"
            ))
            .header("Content-Type", "application/xml")
            .header("x-oss-date", &oss_date)
            .header("x-oss-user-agent", ua2)
            .header("Authorization", &auth_key2)
            .header("x-oss-callback", &callback_b64)
            .header("Content-MD5", &xml_md5)
            .body(xml)
            .timeout(std::time::Duration::from_secs(600))
            .send()
            .await
            .map_err(|e| format!("夸克云端合并失败，请稍后重试（{e}）"))?;
        if !matches!(response.status().as_u16(), 200 | 203) {
            return Err(format!(
                "夸克云端合并失败（HTTP {}），请稍后重试",
                response.status().as_u16()
            ));
        }
        let value = check(
            self.provider,
            self.json(
                self.transport
                    .client
                    .post(
                        self.endpoint("https://drive-pc.quark.cn/1/clouddrive/file/upload/finish"),
                    )
                    .query(&query)
                    .header("Referer", "https://pan.quark.cn/")
                    .json(&json!({ "task_id": task_id, "obj_key": obj_key })),
            )
            .await?,
        )?;
        let mut fid = text(&value["data"]["fid"]);
        if fid.is_empty() {
            'search: for page in 1..=20u32 {
                let value = check(
                    self.provider,
                    self.json(
                        self.transport
                            .client
                            .get(self.endpoint("https://drive-pc.quark.cn/1/clouddrive/file/sort"))
                            .query(&[
                                ("pr", "ucpro".to_string()),
                                ("fr", "pc".to_string()),
                                ("pdir_fid", parent.to_string()),
                                ("_page", page.to_string()),
                                ("_size", "100".to_string()),
                                ("_sort", "file_type:asc,updated_at:desc".to_string()),
                            ])
                            .header("Referer", "https://pan.quark.cn/"),
                    )
                    .await?,
                )?;
                let rows = value["data"]["list"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                for row in &rows {
                    if text(&row["file_name"]) == name && row["dir"].as_bool() != Some(true) {
                        fid = text(&row["fid"]);
                        break 'search;
                    }
                }
                if rows.len() < 100 {
                    break;
                }
            }
        }
        Ok(DriveFile {
            id: fid,
            path: remote_path.to_string(),
            name: name.to_string(),
            size,
            is_dir: false,
            modified: now(),
            created: now(),
            md5,
        })
    }

    // ---- 夸克 OSS 分片上传（大文件续传）----

    /// 开始一次夸克上传会话：创建任务、写入整文件 md5/sha1，返回会话 JSON。
    pub async fn quark_upload_begin(
        &self,
        parent_id: &str,
        name: &str,
        size: u64,
        md5: &str,
        sha1: &str,
    ) -> Result<serde_json::Value, String> {
        if !matches!(self.provider, Provider::Quark) {
            return Err("该网盘暂不支持上传".into());
        }
        let parent = if parent_id.is_empty() { "0" } else { parent_id };
        let now_ms = now().saturating_mul(1000);
        let query = [("pr", "ucpro".to_string()), ("fr", "pc".to_string())];
        let value = check(
            self.provider,
            self.json(
                self.transport
                    .client
                    .post(self.endpoint("https://drive-pc.quark.cn/1/clouddrive/file/upload/pre"))
                    .query(&query)
                    .header("Referer", "https://pan.quark.cn/")
                    .json(&json!({
                        "ccp_hash_update": true,
                        "parallel_upload": true,
                        "pdir_fid": parent,
                        "dir_name": "",
                        "size": size,
                        "file_name": name,
                        "format_type": "application/octet-stream",
                        "l_updated_at": now_ms,
                        "l_created_at": now_ms,
                    })),
            )
            .await?,
        )?;
        let data = &value["data"];
        let task_id = text(&data["task_id"]);
        let upload_id = text(&data["upload_id"]);
        let obj_key = text(&data["obj_key"]);
        let bucket = {
            let bucket = text(&data["bucket"]);
            if bucket.is_empty() {
                "ul-zb".to_string()
            } else {
                bucket
            }
        };
        let auth_info = data["auth_info"].clone();
        let callback = data["callback"].clone();
        if task_id.is_empty() || upload_id.is_empty() || obj_key.is_empty() {
            return Err("夸克返回的上传会话无效，请稍后重试".into());
        }
        check(
            self.provider,
            self.json(
                self.transport
                    .client
                    .post(self.endpoint("https://drive-pc.quark.cn/1/clouddrive/file/update/hash"))
                    .query(&query)
                    .header("Referer", "https://pan.quark.cn/")
                    .json(&json!({ "task_id": task_id, "md5": md5, "sha1": sha1 })),
            )
            .await?,
        )?;
        Ok(json!({
            "task_id": task_id,
            "upload_id": upload_id,
            "obj_key": obj_key,
            "bucket": bucket,
            "auth_info": auth_info,
            "callback": callback,
            "parent": parent,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn quark_upload_put_part(
        &self,
        session: &serde_json::Value,
        part_number: u32,
        hash_ctx: Option<&str>,
        local_path: &std::path::Path,
        offset: u64,
        len: u64,
        progress: &std::sync::Arc<std::sync::atomic::AtomicU64>,
    ) -> Result<String, String> {
        use std::sync::atomic::Ordering;
        let task_id = text(&session["task_id"]);
        let upload_id = text(&session["upload_id"]);
        let obj_key = text(&session["obj_key"]);
        let bucket = text(&session["bucket"]);
        let auth_info = session["auth_info"].clone();
        let put_key = obj_key.trim_start_matches('/');
        let query = [("pr", "ucpro".to_string()), ("fr", "pc".to_string())];
        let oss_date = chrono::Utc::now()
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();
        let ua = "aliyun-sdk-js/1.0.0 Chrome Mobile 139.0.0.0 on Google Nexus 5 (Android 6.0)";
        let hash_line = match hash_ctx {
            Some(ctx) => format!("x-oss-hash-ctx:{ctx}\n"),
            None => String::new(),
        };
        let auth_meta = format!(
            "PUT\n\napplication/octet-stream\n{oss_date}\nx-oss-date:{oss_date}\n{hash_line}x-oss-user-agent:{ua}\n/{bucket}/{put_key}?partNumber={part_number}&uploadId={upload_id}"
        );
        let value = check(
            self.provider,
            self.json(
                self.transport
                    .client
                    .post(self.endpoint("https://drive-pc.quark.cn/1/clouddrive/file/upload/auth"))
                    .query(&query)
                    .header("Referer", "https://pan.quark.cn/")
                    .json(&json!({ "task_id": task_id, "auth_info": auth_info, "auth_meta": auth_meta })),
            )
            .await?,
        )?;
        let auth_key = text(&value["data"]["auth_key"]);
        if auth_key.is_empty() {
            return Err("夸克上传鉴权失败，请稍后重试".into());
        }
        let counter = progress.clone();
        let mut file = tokio::fs::File::open(local_path)
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
                "https://{bucket}.pds.quark.cn/{put_key}?partNumber={part_number}&uploadId={upload_id}"
            ))
            .header("Content-Type", "application/octet-stream")
            .header("x-oss-date", &oss_date)
            .header("x-oss-user-agent", ua)
            .header("Authorization", &auth_key)
            .body(body)
            .timeout(std::time::Duration::from_secs(3600));
        if let Some(ctx) = hash_ctx {
            request = request.header("X-Oss-Hash-Ctx", ctx);
        }
        let response = request
            .send()
            .await
            .map_err(|e| format!("连接夸克云端存储失败，请稍后重试（{e}）"))?;
        if !response.status().is_success() {
            return Err(format!(
                "夸克分片上传失败（HTTP {}），请稍后重试",
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
            return Err("夸克分片上传未返回 ETag".into());
        }
        Ok(etag)
    }

    /// 合并分片并完成文件，返回文件 id。
    pub async fn quark_upload_complete(
        &self,
        session: &serde_json::Value,
        parts: &[(u32, String)],
    ) -> Result<String, String> {
        let task_id = text(&session["task_id"]);
        let upload_id = text(&session["upload_id"]);
        let obj_key = text(&session["obj_key"]);
        let bucket = text(&session["bucket"]);
        let auth_info = session["auth_info"].clone();
        let callback = session["callback"].clone();
        let parent = text(&session["parent"]);
        let name = text(&session["name"]);
        let put_key = obj_key.trim_start_matches('/');
        let query = [("pr", "ucpro".to_string()), ("fr", "pc".to_string())];
        let mut xml = String::from("<CompleteMultipartUpload>");
        for (number, etag) in parts {
            xml.push_str(&format!(
                "<Part><PartNumber>{number}</PartNumber><ETag>\"{etag}\"</ETag></Part>"
            ));
        }
        xml.push_str("</CompleteMultipartUpload>");
        let oss_date = chrono::Utc::now()
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string();
        let xml_md5 = base64::engine::general_purpose::STANDARD.encode(Md5::digest(xml.as_bytes()));
        let callback_b64 = base64::engine::general_purpose::STANDARD.encode(
            serde_json::to_string(&callback)
                .unwrap_or_default()
                .as_bytes(),
        );
        let ua2 = "aliyun-sdk-js/1.0.0 Chrome 139.0.0.0 on OS X 10.15.7 64-bit";
        let auth_meta2 = format!(
            "POST\n{xml_md5}\napplication/xml\n{oss_date}\nx-oss-callback:{callback_b64}\nx-oss-date:{oss_date}\nx-oss-user-agent:{ua2}\n/{bucket}/{put_key}?uploadId={upload_id}"
        );
        let value = check(
            self.provider,
            self.json(
                self.transport
                    .client
                    .post(self.endpoint("https://drive-pc.quark.cn/1/clouddrive/file/upload/auth"))
                    .query(&query)
                    .header("Referer", "https://pan.quark.cn/")
                    .json(&json!({ "task_id": task_id, "auth_meta": auth_meta2, "auth_info": auth_info })),
            )
            .await?,
        )?;
        let auth_key2 = text(&value["data"]["auth_key"]);
        if auth_key2.is_empty() {
            return Err("夸克上传鉴权失败，请稍后重试".into());
        }
        let response = self
            .transport
            .client
            .post(format!(
                "https://{bucket}.pds.quark.cn/{put_key}?uploadId={upload_id}"
            ))
            .header("Content-Type", "application/xml")
            .header("x-oss-date", &oss_date)
            .header("x-oss-user-agent", ua2)
            .header("Authorization", &auth_key2)
            .header("x-oss-callback", &callback_b64)
            .header("Content-MD5", &xml_md5)
            .body(xml)
            .timeout(std::time::Duration::from_secs(600))
            .send()
            .await
            .map_err(|e| format!("夸克云端合并失败，请稍后重试（{e}）"))?;
        if !matches!(response.status().as_u16(), 200 | 203) {
            return Err(format!(
                "夸克云端合并失败（HTTP {}），请稍后重试",
                response.status().as_u16()
            ));
        }
        let value = check(
            self.provider,
            self.json(
                self.transport
                    .client
                    .post(
                        self.endpoint("https://drive-pc.quark.cn/1/clouddrive/file/upload/finish"),
                    )
                    .query(&query)
                    .header("Referer", "https://pan.quark.cn/")
                    .json(&json!({ "task_id": task_id, "obj_key": obj_key })),
            )
            .await?,
        )?;
        let mut fid = text(&value["data"]["fid"]);
        if fid.is_empty() && !name.is_empty() {
            'search: for page in 1..=20u32 {
                let value = check(
                    self.provider,
                    self.json(
                        self.transport
                            .client
                            .get(self.endpoint("https://drive-pc.quark.cn/1/clouddrive/file/sort"))
                            .query(&[
                                ("pr", "ucpro".to_string()),
                                ("fr", "pc".to_string()),
                                ("pdir_fid", parent.to_string()),
                                ("_page", page.to_string()),
                                ("_size", "100".to_string()),
                                ("_sort", "file_type:asc,updated_at:desc".to_string()),
                            ])
                            .header("Referer", "https://pan.quark.cn/"),
                    )
                    .await?,
                )?;
                let rows = value["data"]["list"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                for row in &rows {
                    if text(&row["file_name"]) == name && row["dir"].as_bool() != Some(true) {
                        fid = text(&row["fid"]);
                        break 'search;
                    }
                }
                if rows.len() < 100 {
                    break;
                }
            }
        }
        Ok(fid)
    }
}
