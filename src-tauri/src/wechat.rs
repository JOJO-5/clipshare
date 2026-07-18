use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeChatMessage {
    pub id: String,
    pub sender: String,
    pub preview: String,
    pub content: String,
    pub timestamp: i64,
    pub unread_count: u32,
}

pub fn encode_wechat(message: &WeChatMessage) -> Result<Vec<u8>, String> {
    serde_json::to_vec(message).map_err(|error| error.to_string())
}

pub fn decode_wechat(payload: &[u8]) -> Result<WeChatMessage, String> {
    serde_json::from_slice(payload).map_err(|error| error.to_string())
}

pub fn prepare_wechat_message(mut message: WeChatMessage, preview_limit: usize) -> WeChatMessage {
    if message.preview.is_empty() {
        message.preview = truncate_preview(&message.content, preview_limit);
    }
    message
}

fn truncate_preview(content: &str, limit: usize) -> String {
    let chars: Vec<char> = content.chars().collect();
    if chars.len() <= limit {
        return content.to_string();
    }
    let mut preview: String = chars.into_iter().take(limit).collect();
    preview.push('…');
    preview
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_truncation_counts_unicode_characters() {
        assert_eq!(truncate_preview("你好世界", 3), "你好世…");
        assert_eq!(truncate_preview("short", 10), "short");
    }
}
