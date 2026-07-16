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
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            role: "server".to_string(),
            port: 9527,
            target_ip: String::new(),
            target_port: 9527,
            max_file_size: 104857600,
            language: "zh-CN".to_string(),
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
