use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub role: String,
    pub port: u16,
    pub target_ip: String,
    pub target_port: u16,
    pub max_file_size: u64,
    pub language: String,
    #[serde(default = "default_true")]
    pub wechat_enabled: bool,
    #[serde(default = "default_preview_limit")]
    pub wechat_preview_limit: usize,
    #[serde(default)]
    pub wechat_show_content: bool,
    #[serde(default = "default_true")]
    pub minimize_to_tray: bool,
    #[serde(default)]
    pub autostart: bool,
}

fn default_true() -> bool { true }
fn default_preview_limit() -> usize { 40 }

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            role: "server".to_string(),
            port: 9527,
            target_ip: String::new(),
            target_port: 9527,
            max_file_size: 104857600,
            language: "zh-CN".to_string(),
            wechat_enabled: true,
            wechat_preview_limit: 40,
            wechat_show_content: true,
            minimize_to_tray: true,
            autostart: false,
        }
    }
}

impl AppConfig {
    pub fn config_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".clipshare")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.json")
    }

    pub fn logs_dir() -> PathBuf {
        Self::config_dir().join("logs")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            let content = fs::read_to_string(&path).unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_default()
        } else {
            Self::default()
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let dir = Self::config_dir();
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let content = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(Self::config_path(), content).map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_config_gets_wechat_defaults() {
        let config: AppConfig = serde_json::from_str(
            r#"{"role":"server","port":9527,"target_ip":"","target_port":9527,"max_file_size":104857600,"language":"zh-CN"}"#,
        )
        .unwrap();

        assert!(config.wechat_enabled);
        assert_eq!(config.wechat_preview_limit, 40);
        assert!(!config.wechat_show_content);
        assert!(config.minimize_to_tray);
        assert!(!config.autostart);
    }

    #[test]
    fn default_config_persists_tray_and_autostart_preferences() {
        let value = serde_json::to_value(AppConfig::default()).unwrap();

        assert_eq!(value.get("minimize_to_tray").and_then(|v| v.as_bool()), Some(true));
        assert_eq!(value.get("autostart").and_then(|v| v.as_bool()), Some(false));
    }
}
