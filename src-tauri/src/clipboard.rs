use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub enum ClipboardContent {
    Text(String),
    Image {
        width: usize,
        height: usize,
        bytes: Vec<u8>,
    },
    Files(Vec<String>),
}

fn should_retry_observation(
    current: &str,
    delivered: &str,
    attempted: &str,
    retry_elapsed: bool,
) -> bool {
    current != delivered && (current != attempted || retry_elapsed)
}

fn should_emit_health_log(elapsed: Duration) -> bool {
    elapsed >= Duration::from_secs(30)
}

impl ClipboardContent {
    pub fn data_type(&self) -> &'static str {
        match self {
            ClipboardContent::Text(_) => "text",
            ClipboardContent::Image { .. } => "image",
            ClipboardContent::Files(_) => "file",
        }
    }

    pub fn summary(&self) -> String {
        match self {
            ClipboardContent::Text(t) => {
                if t.chars().count() > 20 {
                    format!("{}...", t.chars().take(20).collect::<String>())
                } else {
                    t.clone()
                }
            }
            ClipboardContent::Image { .. } => "图片".to_string(),
            ClipboardContent::Files(v) => v.join(", "),
        }
    }

    pub fn size(&self) -> u64 {
        match self {
            ClipboardContent::Text(t) => t.len() as u64,
            ClipboardContent::Image { bytes, .. } => bytes.len() as u64,
            ClipboardContent::Files(v) => v
                .iter()
                .map(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
                .sum(),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            ClipboardContent::Text(t) => t.as_bytes().to_vec(),
            ClipboardContent::Image { bytes, .. } => bytes.clone(),
            ClipboardContent::Files(_) => todo!("Files handled separately"),
        }
    }
}

pub fn set_text(text: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        clipboard_win::set_clipboard_string(text).map_err(|error| error.to_string())
    }

    #[cfg(not(windows))]
    {
        let _ = text;
        Err("Clipboard writes are not implemented on this platform".to_string())
    }
}

#[cfg(windows)]
pub fn set_files(paths: &[String]) -> Result<(), String> {
    use clipboard_win::{formats::FileList, Clipboard, Setter};

    if paths.is_empty() {
        return Err("Cannot write an empty file list to the clipboard".to_string());
    }

    let _clipboard = Clipboard::new_attempts(10).map_err(|error| error.to_string())?;
    FileList
        .write_clipboard(paths)
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
pub fn set_files(_paths: &[String]) -> Result<(), String> {
    Ok(())
}

pub fn set_image(width: usize, height: usize, bytes: Vec<u8>) -> Result<(), String> {
    use std::borrow::Cow;

    if width == 0 || height == 0 || bytes.len() != width.saturating_mul(height).saturating_mul(4) {
        return Err("Invalid RGBA image payload".to_string());
    }

    let mut clipboard = arboard::Clipboard::new().map_err(|error| error.to_string())?;
    clipboard
        .set_image(arboard::ImageData {
            width,
            height,
            bytes: Cow::Owned(bytes),
        })
        .map_err(|error| error.to_string())
}

#[cfg(windows)]
pub struct ClipboardListener;

#[cfg(windows)]
impl ClipboardListener {
    pub fn new() -> Self {
        Self
    }

    pub fn start<F, D>(&self, on_change: F, on_diagnostic: D)
    where
        F: Fn(ClipboardContent) -> Result<(), String> + Send + 'static,
        D: Fn(String) + Send + 'static,
    {
        use clipboard_win::{
            formats::{FileList, Unicode},
            get_clipboard,
        };

        std::thread::spawn(move || {
            const RETRY_INTERVAL: Duration = Duration::from_secs(2);
            let mut last_text = String::new();
            let mut attempted_text = String::new();
            let mut last_text_attempt = Instant::now() - RETRY_INTERVAL;
            let mut last_files: Vec<String> = Vec::new();
            let mut attempted_files: Vec<String> = Vec::new();
            let mut last_files_attempt = Instant::now() - RETRY_INTERVAL;
            let mut last_image: Vec<u8> = Vec::new();
            let mut attempted_image: Vec<u8> = Vec::new();
            let mut last_image_attempt = Instant::now() - RETRY_INTERVAL;
            let mut last_health_log = Instant::now();

            on_diagnostic("clipboard-monitor started interval=300ms retry=2s".to_string());

            loop {
                std::thread::sleep(std::time::Duration::from_millis(300));
                if should_emit_health_log(last_health_log.elapsed()) {
                    on_diagnostic("clipboard-monitor alive".to_string());
                    last_health_log = Instant::now();
                }

                if let Ok(text) = get_clipboard::<String, _>(Unicode) {
                    if !text.is_empty()
                        && should_retry_observation(
                            &text,
                            &last_text,
                            &attempted_text,
                            last_text_attempt.elapsed() >= RETRY_INTERVAL,
                        )
                    {
                        attempted_text = text.clone();
                        last_text_attempt = Instant::now();
                        let content = ClipboardContent::Text(text.clone());
                        on_diagnostic(format!(
                            "clipboard-read type=text size={} summary={}",
                            content.size(),
                            content.summary()
                        ));
                        match on_change(content) {
                            Ok(()) => {
                                last_text = text;
                                on_diagnostic("clipboard-send type=text status=sent".to_string());
                            }
                            Err(error) => on_diagnostic(format!(
                                "clipboard-send type=text status=failed error={error}"
                            )),
                        }
                    }
                }

                if let Ok(files) = get_clipboard::<Vec<String>, _>(FileList) {
                    let retry_elapsed = last_files_attempt.elapsed() >= RETRY_INTERVAL;
                    if !files.is_empty()
                        && (files != last_files)
                        && (files != attempted_files || retry_elapsed)
                    {
                        attempted_files = files.clone();
                        last_files_attempt = Instant::now();
                        let content = ClipboardContent::Files(files.clone());
                        on_diagnostic(format!(
                            "clipboard-read type=file size={} summary={}",
                            content.size(),
                            content.summary()
                        ));
                        match on_change(content) {
                            Ok(()) => {
                                last_files = files;
                                on_diagnostic("clipboard-send type=file status=sent".to_string());
                            }
                            Err(error) => on_diagnostic(format!(
                                "clipboard-send type=file status=failed error={error}"
                            )),
                        }
                    }
                }

                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    if let Ok(image) = clipboard.get_image() {
                        let width = image.width;
                        let height = image.height;
                        let bytes = image.bytes.into_owned();
                        if !bytes.is_empty()
                            && bytes != last_image
                            && (bytes != attempted_image
                                || last_image_attempt.elapsed() >= RETRY_INTERVAL)
                        {
                            attempted_image = bytes.clone();
                            last_image_attempt = Instant::now();
                            let content = ClipboardContent::Image {
                                width,
                                height,
                                bytes: bytes.clone(),
                            };
                            on_diagnostic(format!(
                                "clipboard-read type=image width={} height={} size={}",
                                width,
                                height,
                                content.size()
                            ));
                            match on_change(content) {
                                Ok(()) => {
                                    last_image = bytes;
                                    on_diagnostic(
                                        "clipboard-send type=image status=sent".to_string(),
                                    );
                                }
                                Err(error) => on_diagnostic(format!(
                                    "clipboard-send type=image status=failed error={error}"
                                )),
                            }
                        }
                    }
                }
            }
        });
    }
}

#[cfg(not(windows))]
pub struct ClipboardListener;

#[cfg(not(windows))]
impl ClipboardListener {
    pub fn new() -> Self {
        Self
    }
    pub fn start<F, D>(&self, _on_change: F, _on_diagnostic: D)
    where
        F: Fn(ClipboardContent) -> Result<(), String> + Send + 'static,
        D: Fn(String) + Send + 'static,
    {
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_a_failed_clipboard_value_after_the_retry_interval() {
        assert!(should_retry_observation("new", "old", "", false));
        assert!(!should_retry_observation("new", "old", "new", false));
        assert!(should_retry_observation("new", "old", "new", true));
        assert!(!should_retry_observation("new", "new", "new", true));
    }

    #[test]
    fn unicode_text_summary_never_slices_inside_a_character() {
        let text = "\u{4e2d}".repeat(21);
        let summary = ClipboardContent::Text(text).summary();

        assert_eq!(summary, format!("{}...", "\u{4e2d}".repeat(20)));
    }

    #[test]
    fn clipboard_health_log_is_rate_limited() {
        assert!(!should_emit_health_log(Duration::from_secs(29)));
        assert!(should_emit_health_log(Duration::from_secs(30)));
    }
}
