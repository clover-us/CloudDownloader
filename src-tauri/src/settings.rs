use serde::{Deserialize, Serialize};

/// 全局限速上限（KiB/s），0 表示不限制。
pub const MAX_DOWNLOAD_LIMIT: u32 = 1024 * 1024;
pub const MAX_CONCURRENT_DOWNLOADS: u8 = 10;
pub const DEFAULT_CONCURRENT_DOWNLOADS: u8 = 3;
pub const DEFAULT_CONNECTIONS: u8 = 16;
pub const CONNECTION_CHOICES: [u8; 4] = [1, 4, 8, 16];

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
