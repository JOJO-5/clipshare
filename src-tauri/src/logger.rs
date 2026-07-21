use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use chrono::Local;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub time: String,
    #[serde(rename = "type")]
    pub log_type: String,
    #[serde(rename = "dataType")]
    pub data_type: String,
    pub content: String,
    pub size: u64,
}

impl LogEntry {
    pub fn send(data_type: &str, content: &str, size: u64) -> Self {
        Self {
            time: Local::now().format("%Y-%m-%dT%H:%M:%S%.3f").to_string(),
            log_type: "send".to_string(),
            data_type: data_type.to_string(),
            content: content.to_string(),
            size,
        }
    }

    pub fn recv(data_type: &str, content: &str, size: u64) -> Self {
        Self {
            time: Local::now().format("%Y-%m-%dT%H:%M:%S%.3f").to_string(),
            log_type: "recv".to_string(),
            data_type: data_type.to_string(),
            content: content.to_string(),
            size,
        }
    }

    pub fn error(content: &str) -> Self {
        Self {
            time: Local::now().format("%Y-%m-%dT%H:%M:%S%.3f").to_string(),
            log_type: "error".to_string(),
            data_type: String::new(),
            content: content.to_string(),
            size: 0,
        }
    }

    pub fn info(data_type: &str, content: &str) -> Self {
        Self {
            time: Local::now().format("%Y-%m-%dT%H:%M:%S%.3f").to_string(),
            log_type: "info".to_string(),
            data_type: data_type.to_string(),
            content: content.to_string(),
            size: 0,
        }
    }


    pub fn log_file_path() -> PathBuf {
        let today = Local::now().format("%Y-%m-%d").to_string();
        crate::config::AppConfig::logs_dir().join(format!("{}.jsonl", today))
    }

    pub fn write_to_file(&self) -> Result<(), String> {
        let path = Self::log_file_path();
        let dir = path.parent().unwrap();
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| e.to_string())?;

        let mut writer = BufWriter::new(file);
        let json = serde_json::to_string(self).map_err(|e| e.to_string())?;
        writeln!(writer, "{}", json).map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_entries_are_visible_as_diagnostic_logs() {
        let entry = LogEntry::info("wechat-monitor", "UIA initialized");

        assert_eq!(entry.log_type, "info");
        assert_eq!(entry.data_type, "wechat-monitor");
        assert_eq!(entry.content, "UIA initialized");
        assert_eq!(entry.size, 0);
    }
}
