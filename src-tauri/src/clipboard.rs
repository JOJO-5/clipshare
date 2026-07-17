#[derive(Debug, Clone)]
pub enum ClipboardContent {
    Text(String),
    Image { width: usize, height: usize, bytes: Vec<u8> },
    Files(Vec<String>),
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
                if t.len() > 20 { format!("{}...", &t[..20]) } else { t.clone() }
            }
            ClipboardContent::Image { .. } => "图片".to_string(),
            ClipboardContent::Files(v) => v.join(", "),
        }
    }

    pub fn size(&self) -> u64 {
        match self {
            ClipboardContent::Text(t) => t.len() as u64,
            ClipboardContent::Image { bytes, .. } => bytes.len() as u64,
            ClipboardContent::Files(v) => v.iter().map(|p| {
                std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
            }).sum(),
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

pub fn set_image(width: usize, height: usize, bytes: Vec<u8>) -> Result<(), String> {
    use std::borrow::Cow;

    if width == 0 || height == 0 || bytes.len() != width.saturating_mul(height).saturating_mul(4) {
        return Err("Invalid RGBA image payload".to_string());
    }

    let mut clipboard = arboard::Clipboard::new().map_err(|error| error.to_string())?;
    clipboard
        .set_image(arboard::ImageData { width, height, bytes: Cow::Owned(bytes) })
        .map_err(|error| error.to_string())
}

#[cfg(windows)]
pub struct ClipboardListener;

#[cfg(windows)]
impl ClipboardListener {
    pub fn new() -> Self { Self }

    pub fn start<F>(&self, on_change: F)
    where
        F: Fn(ClipboardContent) -> bool + Send + 'static,
    {
        use clipboard_win::{formats::{FileList, Unicode}, get_clipboard};

        std::thread::spawn(move || {
            let mut last_text = String::new();
            let mut last_files: Vec<String> = Vec::new();
            let mut last_image: Vec<u8> = Vec::new();

            loop {
                std::thread::sleep(std::time::Duration::from_millis(300));

                if let Ok(text) = get_clipboard::<String, _>(Unicode) {
                    if !text.is_empty() && text != last_text {
                        if on_change(ClipboardContent::Text(text.clone())) {
                            last_text = text;
                        }
                    }
                }

                if let Ok(files) = get_clipboard::<Vec<String>, _>(FileList) {
                    if !files.is_empty() && files != last_files {
                        if on_change(ClipboardContent::Files(files.clone())) {
                            last_files = files;
                        }
                    }
                }

                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    if let Ok(image) = clipboard.get_image() {
                        let width = image.width;
                        let height = image.height;
                        let bytes = image.bytes.into_owned();
                        if !bytes.is_empty() && bytes != last_image {
                            if on_change(ClipboardContent::Image { width, height, bytes: bytes.clone() }) {
                                last_image = bytes;
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
    pub fn new() -> Self { Self }
    pub fn start<F>(&self, _on_change: F)
    where F: Fn(ClipboardContent) -> bool + Send + 'static { }
}
