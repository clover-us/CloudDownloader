use serde::{Deserialize, Serialize};

/// 全局限速上限（KiB/s），0 表示不限制。
pub const MAX_DOWNLOAD_LIMIT: u32 = 1024 * 1024;
pub const MAX_CONCURRENT_DOWNLOADS: u8 = 10;
pub const DEFAULT_CONCURRENT_DOWNLOADS: u8 = 3;
pub const DEFAULT_CONNECTIONS: u8 = 16;
pub const CONNECTION_CHOICES: [u8; 4] = [1, 4, 8, 16];

/// 上传分片大小（MiB）：影响百度/迅雷/夸克的分片粒度与续传。
pub const MIN_UPLOAD_CHUNK_MIB: u8 = 1;
pub const MAX_UPLOAD_CHUNK_MIB: u8 = 32;
pub const DEFAULT_UPLOAD_CHUNK_MIB: u8 = 8;

/// 同时上传的任务数（个）：控制有多少个文件并行上传。
pub const MIN_UPLOAD_CONCURRENCY: u8 = 1;
pub const MAX_UPLOAD_CONCURRENCY: u8 = 8;
pub const DEFAULT_UPLOAD_CONCURRENCY: u8 = 2;

/// 单个文件内部的分片并发上限：控制每个文件同时上传多少个分片。
pub const MIN_UPLOAD_PART_CONCURRENCY: u8 = 1;
pub const MAX_UPLOAD_PART_CONCURRENCY: u8 = 16;
pub const DEFAULT_UPLOAD_PART_CONCURRENCY: u8 = 4;

/// aria2 最小分段大小（MiB）：越小分段越细、连接越均匀，越大分段越少。
pub const MIN_SPLIT_MIB: u8 = 1;
pub const MAX_SPLIT_MIB: u8 = 64;
pub const DEFAULT_SPLIT_MIB: u8 = 1;

/// 浏览器扩展连接的端口下限：低端口多为系统保留，扩展也用不上。
pub const MIN_BRIDGE_PORT: u16 = 1024;

/// 关闭主窗口时的行为。
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CloseAction {
    #[default]
    Tray,
    Exit,
}

impl CloseAction {
    fn key(self) -> &'static str {
        match self {
            Self::Tray => "tray",
            Self::Exit => "exit",
        }
    }

    fn parse(value: Option<String>) -> Self {
        match value.as_deref() {
            Some("exit") => Self::Exit,
            _ => Self::Tray,
        }
    }
}

/// 用户偏好设置，持久化在 SQLite 的 settings 表里。
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub download_dir: String,
    pub max_overall_download_limit: u32,
    pub max_concurrent_downloads: u8,
    pub default_connections: u8,
    pub close_action: CloseAction,
    pub auto_check_updates: bool,
    pub upload_chunk_mib: u8,
    pub upload_concurrency: u8,
    pub upload_part_concurrency: u8,
    pub min_split_mib: u8,
    /// 浏览器扩展把下载接力给本机时使用的开关（关闭后不再接受请求）。
    pub browser_bridge: bool,
    /// 浏览器扩展连接的端口，扩展选项里的端口需要与此一致。
    pub bridge_port: u16,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            download_dir: String::new(),
            max_overall_download_limit: 0,
            max_concurrent_downloads: DEFAULT_CONCURRENT_DOWNLOADS,
            default_connections: DEFAULT_CONNECTIONS,
            close_action: CloseAction::Tray,
            auto_check_updates: true,
            upload_chunk_mib: DEFAULT_UPLOAD_CHUNK_MIB,
            upload_concurrency: DEFAULT_UPLOAD_CONCURRENCY,
            upload_part_concurrency: DEFAULT_UPLOAD_PART_CONCURRENCY,
            min_split_mib: DEFAULT_SPLIT_MIB,
            browser_bridge: true,
            bridge_port: crate::bridge::DEFAULT_PORT,
        }
    }
}

impl Settings {
    /// 读取设置。损坏或越界的值回退到默认值，避免历史数据让程序启动失败。
    pub fn load(store: &crate::store::Store) -> Self {
        let mut settings = Self::default();
        if let Some(dir) = store.setting("download_dir") {
            settings.download_dir = dir;
        }
        if let Some(value) = store
            .setting("max_overall_download_limit")
            .and_then(|value| value.parse::<u32>().ok())
        {
            settings.max_overall_download_limit = value.min(MAX_DOWNLOAD_LIMIT);
        }
        if let Some(value) = store
            .setting("max_concurrent_downloads")
            .and_then(|value| value.parse::<u8>().ok())
        {
            if (1..=MAX_CONCURRENT_DOWNLOADS).contains(&value) {
                settings.max_concurrent_downloads = value;
            }
        }
        if let Some(value) = store
            .setting("default_connections")
            .and_then(|value| value.parse::<u8>().ok())
        {
            if CONNECTION_CHOICES.contains(&value) {
                settings.default_connections = value;
            }
        }
        settings.close_action = CloseAction::parse(store.setting("close_action"));
        if let Some(value) = store.setting("auto_check_updates") {
            settings.auto_check_updates = value != "0";
        }
        if let Some(value) = store
            .setting("upload_chunk_mib")
            .and_then(|value| value.parse::<u8>().ok())
        {
            if (MIN_UPLOAD_CHUNK_MIB..=MAX_UPLOAD_CHUNK_MIB).contains(&value) {
                settings.upload_chunk_mib = value;
            }
        }
        if let Some(value) = store
            .setting("upload_concurrency")
            .and_then(|value| value.parse::<u8>().ok())
        {
            if (MIN_UPLOAD_CONCURRENCY..=MAX_UPLOAD_CONCURRENCY).contains(&value) {
                settings.upload_concurrency = value;
            }
        }
        if let Some(value) = store
            .setting("upload_part_concurrency")
            .and_then(|value| value.parse::<u8>().ok())
        {
            if (MIN_UPLOAD_PART_CONCURRENCY..=MAX_UPLOAD_PART_CONCURRENCY).contains(&value) {
                settings.upload_part_concurrency = value;
            }
        }
        if let Some(value) = store
            .setting("min_split_mib")
            .and_then(|value| value.parse::<u8>().ok())
        {
            if (MIN_SPLIT_MIB..=MAX_SPLIT_MIB).contains(&value) {
                settings.min_split_mib = value;
            }
        }
        if let Some(value) = store.setting("browser_bridge") {
            settings.browser_bridge = value != "0";
        }
        if let Some(value) = store
            .setting("bridge_port")
            .and_then(|value| value.parse::<u16>().ok())
        {
            if value >= MIN_BRIDGE_PORT {
                settings.bridge_port = value;
            }
        }
        settings
    }

    /// 写入设置前校验，保证界面拿到的永远是合法值。
    pub fn validate(&self) -> Result<(), String> {
        let dir = std::path::Path::new(&self.download_dir);
        if !dir.is_absolute() || !dir.is_dir() {
            return Err("请选择已存在的本地下载文件夹".into());
        }
        if self.max_overall_download_limit > MAX_DOWNLOAD_LIMIT {
            return Err(format!("全局限速不能超过 {} KiB/s", MAX_DOWNLOAD_LIMIT));
        }
        if self.max_concurrent_downloads == 0
            || self.max_concurrent_downloads > MAX_CONCURRENT_DOWNLOADS
        {
            return Err(format!(
                "同时下载数需要在 1–{} 之间",
                MAX_CONCURRENT_DOWNLOADS
            ));
        }
        if !CONNECTION_CHOICES.contains(&self.default_connections) {
            return Err("默认连接数只能是 1、4、8 或 16".into());
        }
        if self.upload_chunk_mib < MIN_UPLOAD_CHUNK_MIB
            || self.upload_chunk_mib > MAX_UPLOAD_CHUNK_MIB
        {
            return Err(format!(
                "上传分片大小需要在 {}–{} MiB 之间",
                MIN_UPLOAD_CHUNK_MIB, MAX_UPLOAD_CHUNK_MIB
            ));
        }
        if self.upload_concurrency < MIN_UPLOAD_CONCURRENCY
            || self.upload_concurrency > MAX_UPLOAD_CONCURRENCY
        {
            return Err(format!(
                "同时上传数需要在 {}–{} 之间",
                MIN_UPLOAD_CONCURRENCY, MAX_UPLOAD_CONCURRENCY
            ));
        }
        if self.upload_part_concurrency < MIN_UPLOAD_PART_CONCURRENCY
            || self.upload_part_concurrency > MAX_UPLOAD_PART_CONCURRENCY
        {
            return Err(format!(
                "单文件分片并发需要在 {}–{} 之间",
                MIN_UPLOAD_PART_CONCURRENCY, MAX_UPLOAD_PART_CONCURRENCY
            ));
        }
        if self.min_split_mib < MIN_SPLIT_MIB || self.min_split_mib > MAX_SPLIT_MIB {
            return Err(format!(
                "最小分段大小需要在 {}–{} MiB 之间",
                MIN_SPLIT_MIB, MAX_SPLIT_MIB
            ));
        }
        if self.bridge_port < MIN_BRIDGE_PORT {
            return Err(format!(
                "浏览器接力的端口需要在 {}–65535 之间",
                MIN_BRIDGE_PORT
            ));
        }
        Ok(())
    }

    /// 需要立即同步给 aria2 的全局选项。
    pub fn aria2_options(&self) -> Vec<(&'static str, String)> {
        let limit = if self.max_overall_download_limit == 0 {
            "0".to_string()
        } else {
            format!("{}K", self.max_overall_download_limit)
        };
        vec![
            ("max-overall-download-limit", limit),
            (
                "max-concurrent-downloads",
                self.max_concurrent_downloads.to_string(),
            ),
        ]
    }
}

pub fn save(store: &crate::store::Store, settings: &Settings) -> Result<Settings, String> {
    settings.validate()?;
    // Thirteen keys are written together; a failure halfway through used to leave
    // a mixture of old and new settings (a new download directory with the old
    // bridge port, for example).
    store.transaction(|store| {
        store.set_setting("download_dir", &settings.download_dir)?;
        store.set_setting(
            "max_overall_download_limit",
            &settings.max_overall_download_limit.to_string(),
        )?;
        store.set_setting(
            "max_concurrent_downloads",
            &settings.max_concurrent_downloads.to_string(),
        )?;
        store.set_setting(
            "default_connections",
            &settings.default_connections.to_string(),
        )?;
        store.set_setting("close_action", settings.close_action.key())?;
        store.set_setting(
            "auto_check_updates",
            if settings.auto_check_updates {
                "1"
            } else {
                "0"
            },
        )?;
        store.set_setting("upload_chunk_mib", &settings.upload_chunk_mib.to_string())?;
        store.set_setting(
            "upload_concurrency",
            &settings.upload_concurrency.to_string(),
        )?;
        store.set_setting(
            "upload_part_concurrency",
            &settings.upload_part_concurrency.to_string(),
        )?;
        store.set_setting("min_split_mib", &settings.min_split_mib.to_string())?;
        store.set_setting(
            "browser_bridge",
            if settings.browser_bridge { "1" } else { "0" },
        )?;
        store.set_setting("bridge_port", &settings.bridge_port.to_string())?;
        Ok(())
    })?;
    Ok(settings.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (crate::store::Store, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "clouddownloader-settings-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        let store = crate::store::Store::open(&path).unwrap();
        (store, path)
    }

    fn valid(dir: &str) -> Settings {
        Settings {
            download_dir: dir.into(),
            ..Settings::default()
        }
    }

    #[test]
    fn defaults_are_safe_and_round_trip_through_the_store() {
        let (store, path) = store();
        let dir = std::env::temp_dir();
        let saved = save(&store, &valid(&dir.to_string_lossy())).unwrap();
        assert_eq!(saved.max_concurrent_downloads, DEFAULT_CONCURRENT_DOWNLOADS);
        assert_eq!(saved.default_connections, DEFAULT_CONNECTIONS);
        assert_eq!(saved.close_action, CloseAction::Tray);
        assert!(saved.auto_check_updates);

        let mut changed = saved.clone();
        changed.max_overall_download_limit = 2048;
        changed.max_concurrent_downloads = 5;
        changed.default_connections = 4;
        changed.close_action = CloseAction::Exit;
        changed.auto_check_updates = false;
        save(&store, &changed).unwrap();
        assert_eq!(Settings::load(&store), changed);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn the_bridge_port_round_trips_and_rejects_reserved_ports() {
        let (store, path) = store();
        let dir = std::env::temp_dir();
        let mut changed = valid(&dir.to_string_lossy());
        changed.bridge_port = 18890;
        save(&store, &changed).unwrap();
        assert_eq!(Settings::load(&store).bridge_port, 18890);

        changed.bridge_port = 80;
        assert!(save(&store, &changed).is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn load_ignores_out_of_range_values_instead_of_failing() {
        let (store, path) = store();
        store
            .set_setting("max_overall_download_limit", "99999999")
            .unwrap();
        store.set_setting("max_concurrent_downloads", "0").unwrap();
        store.set_setting("default_connections", "7").unwrap();
        store.set_setting("close_action", "unknown").unwrap();
        let settings = Settings::load(&store);
        assert_eq!(settings.max_overall_download_limit, MAX_DOWNLOAD_LIMIT);
        assert_eq!(
            settings.max_concurrent_downloads,
            DEFAULT_CONCURRENT_DOWNLOADS
        );
        assert_eq!(settings.default_connections, DEFAULT_CONNECTIONS);
        assert_eq!(settings.close_action, CloseAction::Tray);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn validation_rejects_invalid_limits_and_directories() {
        let dir = std::env::temp_dir().to_string_lossy().to_string();
        assert!(valid(&dir).validate().is_ok());
        assert!(valid("relative/path").validate().is_err());
        assert!(valid(&format!(
            "{dir}/definitely-missing-{}",
            uuid::Uuid::new_v4()
        ))
        .validate()
        .is_err());
        assert!(Settings {
            max_concurrent_downloads: 0,
            ..valid(&dir)
        }
        .validate()
        .is_err());
        assert!(Settings {
            max_concurrent_downloads: 11,
            ..valid(&dir)
        }
        .validate()
        .is_err());
        assert!(Settings {
            default_connections: 7,
            ..valid(&dir)
        }
        .validate()
        .is_err());
        assert!(Settings {
            max_overall_download_limit: MAX_DOWNLOAD_LIMIT + 1,
            ..valid(&dir)
        }
        .validate()
        .is_err());
    }

    #[test]
    fn aria2_options_express_unlimited_and_capped_speeds() {
        let unlimited = Settings::default().aria2_options();
        assert_eq!(unlimited[0], ("max-overall-download-limit", "0".into()));
        assert_eq!(unlimited[1], ("max-concurrent-downloads", "3".into()));

        let capped = Settings {
            max_overall_download_limit: 1500,
            max_concurrent_downloads: 8,
            ..Settings::default()
        }
        .aria2_options();
        assert_eq!(capped[0], ("max-overall-download-limit", "1500K".into()));
        assert_eq!(capped[1], ("max-concurrent-downloads", "8".into()));
    }
}
