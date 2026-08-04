use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_role")]
    pub role: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub target_ip: String,
    #[serde(default = "default_port")]
    pub target_port: u16,
    #[serde(default = "default_max_file_size")]
    pub max_file_size: u64,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_true")]
    pub wechat_enabled: bool,
    #[serde(default = "default_preview_limit")]
    pub wechat_preview_limit: usize,
    #[serde(default)]
    pub wechat_show_content: bool,
    #[serde(default)]
    pub wechat_session_filter: String,
    #[serde(default = "default_true")]
    pub minimize_to_tray: bool,
    #[serde(default)]
    pub autostart: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartupConnection {
    Server { port: u16 },
    Client { ip: String, port: u16 },
}

fn default_role() -> String {
    "server".to_string()
}
fn default_port() -> u16 {
    9527
}
fn default_max_file_size() -> u64 {
    104857600
}
fn default_language() -> String {
    "zh-CN".to_string()
}
fn default_true() -> bool {
    true
}
fn default_preview_limit() -> usize {
    40
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
            wechat_enabled: true,
            wechat_preview_limit: 40,
            wechat_show_content: true,
            wechat_session_filter: String::new(),
            minimize_to_tray: true,
            autostart: false,
        }
    }
}

impl AppConfig {
    pub fn remember_server_connection(&mut self, port: u16) -> Result<(), String> {
        if port == 0 {
            return Err("Server port is invalid".to_string());
        }
        self.role = "server".to_string();
        self.port = port;
        Ok(())
    }

    pub fn remember_client_connection(&mut self, ip: &str, port: u16) -> Result<(), String> {
        let ip = ip.trim();
        if ip.is_empty() {
            return Err("Client target IP is empty".to_string());
        }
        if port == 0 {
            return Err("Client target port is invalid".to_string());
        }
        self.role = "client".to_string();
        self.target_ip = ip.to_string();
        self.target_port = port;
        Ok(())
    }

    pub fn startup_connection(&self) -> Result<StartupConnection, String> {
        match self.role.as_str() {
            "server" if self.port > 0 => Ok(StartupConnection::Server { port: self.port }),
            "server" => Err("Saved server port is invalid".to_string()),
            "client" => {
                let ip = self.target_ip.trim();
                if ip.is_empty() {
                    return Err("Saved client target IP is empty".to_string());
                }
                if self.target_port == 0 {
                    return Err("Saved client target port is invalid".to_string());
                }
                Ok(StartupConnection::Client {
                    ip: ip.to_string(),
                    port: self.target_port,
                })
            }
            role => Err(format!("Saved connection role is invalid: {role}")),
        }
    }

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

    pub fn pending_wechat_path() -> PathBuf {
        Self::config_dir().join("wechat-pending.json")
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
        assert!(config.wechat_session_filter.is_empty());
        assert!(config.minimize_to_tray);
        assert!(!config.autostart);
    }

    #[test]
    fn default_config_persists_tray_and_autostart_preferences() {
        let value = serde_json::to_value(AppConfig::default()).unwrap();

        assert_eq!(
            value.get("minimize_to_tray").and_then(|v| v.as_bool()),
            Some(true)
        );
        assert_eq!(
            value.get("autostart").and_then(|v| v.as_bool()),
            Some(false)
        );
    }

    #[test]
    fn partial_config_uses_connection_defaults() {
        let config: AppConfig = serde_json::from_str(
            r#"{"role":"client","target_ip":"192.168.1.20","target_port":9527}"#,
        )
        .unwrap();

        assert_eq!(config.port, 9527);
        assert_eq!(config.max_file_size, 104857600);
        assert_eq!(config.language, "zh-CN");
    }

    #[test]
    fn saved_server_config_restores_listening_on_startup() {
        let config = AppConfig {
            role: "server".to_string(),
            port: 18789,
            ..AppConfig::default()
        };

        assert_eq!(
            config.startup_connection().unwrap(),
            StartupConnection::Server { port: 18789 }
        );
    }

    #[test]
    fn saved_client_config_restores_target_on_startup() {
        let config = AppConfig {
            role: "client".to_string(),
            target_ip: " 192.168.1.20 ".to_string(),
            target_port: 18789,
            ..AppConfig::default()
        };

        assert_eq!(
            config.startup_connection().unwrap(),
            StartupConnection::Client {
                ip: "192.168.1.20".to_string(),
                port: 18789,
            }
        );
    }

    #[test]
    fn client_startup_rejects_an_empty_saved_target() {
        let config = AppConfig {
            role: "client".to_string(),
            target_ip: "   ".to_string(),
            ..AppConfig::default()
        };

        assert_eq!(
            config.startup_connection().unwrap_err(),
            "Saved client target IP is empty"
        );
    }

    #[test]
    fn manual_client_connection_becomes_the_next_startup_target() {
        let mut config = AppConfig::default();

        config
            .remember_client_connection(" 10.0.0.8 ", 18789)
            .unwrap();

        assert_eq!(config.role, "client");
        assert_eq!(config.target_ip, "10.0.0.8");
        assert_eq!(config.target_port, 18789);
        assert_eq!(
            config.startup_connection().unwrap(),
            StartupConnection::Client {
                ip: "10.0.0.8".to_string(),
                port: 18789,
            }
        );
    }

    #[test]
    fn manual_server_start_becomes_the_next_startup_listener() {
        let mut config = AppConfig::default();

        config.remember_server_connection(18789).unwrap();

        assert_eq!(config.role, "server");
        assert_eq!(config.port, 18789);
    }
}
