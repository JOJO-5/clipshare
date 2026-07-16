#[derive(Debug, Clone)]
pub enum ClipboardContent {
    Text(String),
    Image(Vec<u8>),
    Files(Vec<String>),
}

impl ClipboardContent {
    pub fn data_type(&self) -> &'static str {
        match self {
            ClipboardContent::Text(_) => "text",
            ClipboardContent::Image(_) => "image",
            ClipboardContent::Files(_) => "file",
        }
    }

    pub fn summary(&self) -> String {
        match self {
            ClipboardContent::Text(t) => {
                if t.len() > 20 { format!("{}...", &t[..20]) } else { t.clone() }
            }
            ClipboardContent::Image(_) => "截图".to_string(),
            ClipboardContent::Files(v) => v.join(", "),
        }
    }

    pub fn size(&self) -> u64 {
        match self {
            ClipboardContent::Text(t) => t.len() as u64,
            ClipboardContent::Image(b) => b.len() as u64,
            ClipboardContent::Files(v) => v.iter().map(|p| {
                std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
            }).sum(),
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            ClipboardContent::Text(t) => t.as_bytes().to_vec(),
            ClipboardContent::Image(b) => b.clone(),
            ClipboardContent::Files(_) => todo!("Files handled separately"),
        }
    }
}

#[cfg(windows)]
pub struct ClipboardListener;

#[cfg(windows)]
impl ClipboardListener {
    pub fn new() -> Self { Self }

    pub fn start<F>(&self, on_change: F)
    where
        F: Fn(ClipboardContent) + Send + 'static,
    {
        use clipboard_win::{formats::{FileList, Unicode}, get_clipboard};

        std::thread::spawn(move || {
            let mut last_text = String::new();
            let mut last_files: Vec<String> = Vec::new();

            loop {
                std::thread::sleep(std::time::Duration::from_millis(300));

                if let Ok(text) = get_clipboard::<String, _>(Unicode) {
                    if !text.is_empty() && text != last_text {
                        last_text = text.clone();
                        on_change(ClipboardContent::Text(text));
                    }
                }

                if let Ok(files) = get_clipboard::<Vec<String>, _>(FileList) {
                    if !files.is_empty() && files != last_files {
                        last_files = files.clone();
                        on_change(ClipboardContent::Files(files));
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
    where F: Fn(ClipboardContent) + Send + 'static { }
}
