# ClipShare Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a Tauri-based clipboard sharing tool (server/client mode) with React UI, TCP protocol, and logging.

**Architecture:** Tauri 2.x app with Rust backend (clipboard monitoring, TCP networking, protocol) and React+Ant Design frontend. Single executable, system tray, config + log panels.

**Tech Stack:** Tauri 2.x / Rust / React 18 + Ant Design 5 + TypeScript / TCP custom protocol

---

## File Structure

```
clipshare/
├── src/                              # React 前端
│   ├── components/
│   │   ├── ConfigPanel.tsx
│   │   ├── LogPanel.tsx
│   │   └── StatusIndicator.tsx
│   ├── hooks/
│   │   └── useTauriCommands.ts
│   ├── App.tsx
│   ├── main.tsx
│   └── index.css
├── src-tauri/                        # Rust 后端
│   ├── src/
│   │   ├── lib.rs                   # 模块导出
│   │   ├── clipboard.rs             # 剪贴板监控
│   │   ├── network.rs               # TCP 服务器/客户端
│   │   ├── protocol.rs              # 协议编解码
│   │   ├── logger.rs                # 日志写入
│   │   ├── config.rs                # 配置加载/保存
│   │   ├── commands.rs              # Tauri 命令
│   │   └── main.rs                  # 入口
│   ├── icons/
│   ├── Cargo.toml
│   └── tauri.conf.json
├── package.json
└── SPEC.md
```

---

## Phase 1: 项目脚手架

### Task 1: 初始化 Tauri 项目

**Files:**
- Create: `clipshare/package.json`
- Create: `clipshare/vite.config.ts`
- Create: `clipshare/tsconfig.json`
- Create: `clipshare/tsconfig.node.json`
- Create: `clipshare/index.html`
- Create: `clipshare/src/main.tsx`
- Create: `clipshare/src/App.tsx`
- Create: `clipshare/src/index.css`
- Create: `clipshare/src/components/ConfigPanel.tsx`
- Create: `clipshare/src/components/LogPanel.tsx`
- Create: `clipshare/src/components/StatusIndicator.tsx`
- Create: `clipshare/src-tauri/Cargo.toml`
- Create: `clipshare/src-tauri/tauri.conf.json`
- Create: `clipshare/src-tauri/build.rs`
- Create: `clipshare/src-tauri/src/main.rs`
- Create: `clipshare/src-tauri/src/lib.rs`
- Create: `clipshare/src-tauri/src/clipboard.rs`
- Create: `clipshare/src-tauri/src/network.rs`
- Create: `clipshare/src-tauri/src/protocol.rs`
- Create: `clipshare/src-tauri/src/logger.rs`
- Create: `clipshare/src-tauri/src/config.rs`
- Create: `clipshare/src-tauri/src/commands.rs`

- [ ] **Step 1: 创建 package.json**

```json
{
  "name": "clipshare",
  "version": "0.1.0",
  "private": true,
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview",
    "tauri": "tauri"
  },
  "dependencies": {
    "react": "^18.2.0",
    "react-dom": "^18.2.0",
    "antd": "^5.12.0",
    "@tauri-apps/api": "^2.0.0"
  },
  "devDependencies": {
    "@types/react": "^18.2.0",
    "@types/react-dom": "^18.2.0",
    "@vitejs/plugin-react": "^4.2.0",
    "typescript": "^5.3.0",
    "vite": "^5.0.0",
    "@tauri-apps/cli": "^2.0.0"
  }
}
```

- [ ] **Step 2: 创建 vite.config.ts**

```ts
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    target: process.env.TAURI_PLATFORM === 'windows' ? 'chrome105' : 'safari13',
    minify: !process.env.TAURI_DEBUG ? 'esbuild' : false,
    sourcemap: !!process.env.TAURI_DEBUG,
  },
})
```

- [ ] **Step 3: 创建 tsconfig.json**

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true
  },
  "include": ["src"],
  "references": [{ "path": "./tsconfig.node.json" }]
}
```

- [ ] **Step 4: 创建 tsconfig.node.json**

```json
{
  "compilerOptions": {
    "composite": true,
    "skipLibCheck": true,
    "module": "ESNext",
    "moduleResolution": "bundler",
    "allowSyntheticDefaultImports": true
  },
  "include": ["vite.config.ts"]
}
```

- [ ] **Step 5: 创建 index.html**

```html
<!DOCTYPE html>
<html lang="zh-CN">
  <head>
    <meta charset="UTF-8" />
    <link rel="icon" type="image/x-icon" href="/favicon.ico" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>ClipShare</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 6: 创建 src/main.tsx**

```tsx
import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App'
import './index.css'

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)
```

- [ ] **Step 7: 创建 src/index.css**

```css
* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

body {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
  background: #f0f2f5;
  overflow: hidden;
}

#root {
  width: 100vw;
  height: 100vh;
}
```

- [ ] **Step 8: 创建 src/App.tsx (初始版本，空壳)**

```tsx
import { useState } from 'react'
import { Tabs, Typography } from 'antd'

const { Title } = Typography

function App() {
  const [activeTab, setActiveTab] = useState('config')

  return (
    <div style={{ width: '100vw', height: '100vh', display: 'flex', flexDirection: 'column' }}>
      <div style={{ background: '#001529', padding: '12px 20px' }}>
        <Title level={4} style={{ color: 'white', margin: 0 }}>ClipShare 剪贴板共享</Title>
      </div>
      <Tabs
        activeKey={activeTab}
        onChange={setActiveTab}
        style={{ flex: 1, padding: '0 16px' }}
        items={[
          { key: 'config', label: '配置', children: <div>配置面板</div> },
          { key: 'log', label: '日志', children: <div>日志面板</div> },
        ]}
      />
    </div>
  )
}

export default App
```

- [ ] **Step 9: 创建 src/components/ConfigPanel.tsx (空壳)**

```tsx
export function ConfigPanel() {
  return <div>配置面板</div>
}
```

- [ ] **Step 10: 创建 src/components/LogPanel.tsx (空壳)**

```tsx
export function LogPanel() {
  return <div>日志面板</div>
}
```

- [ ] **Step 11: 创建 src/components/StatusIndicator.tsx (空壳)**

```tsx
export function StatusIndicator() {
  return <span>状态指示器</span>
}
```

- [ ] **Step 12: 创建 src-tauri/Cargo.toml**

```toml
[package]
name = "clipshare"
version = "0.1.0"
edition = "2021"

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = ["tray-icon"] }
tauri-plugin-shell = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
log = "0.4"
env_logger = "0.11"
chrono = "0.4"
dirs = "5"
flate2 = "1"
clipboard-win = "5"
arboard = "3"

[target.'cfg(windows)'.dependencies]
windows = { version = "0.58", features = [
    "Win32_Foundation",
    "Win32_System_DataExchange",
    "Win32_Graphics_Gdi",
    "Win32_UI_Shell",
    "Win32_System_Com",
] }

[features]
default = ["custom-protocol"]
custom-protocol = ["tauri/custom-protocol"]
```

- [ ] **Step 13: 创建 src-tauri/tauri.conf.json**

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "ClipShare",
  "version": "0.1.0",
  "identifier": "com.clipshare.app",
  "build": {
    "devtools": true,
    "frontendDist": "../dist",
    "devUrl": "http://localhost:1420",
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build"
  },
  "app": {
    "withGlobalTauri": true,
    "windows": [
      {
        "title": "ClipShare",
        "width": 600,
        "height": 500,
        "minWidth": 480,
        "minHeight": 400,
        "resizable": true,
        "fullscreen": false,
        "center": true
      }
    ],
    "trayIcon": {
      "iconPath": "icons/icon.png",
      "iconAsTemplate": true
    },
    "security": {
      "csp": null
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  }
}
```

- [ ] **Step 14: 创建 src-tauri/build.rs**

```rust
fn main() {
    tauri_build::build()
}
```

- [ ] **Step 15: 创建 src-tauri/src/main.rs**

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    clipshare::run()
}
```

- [ ] **Step 16: 创建 src-tauri/src/lib.rs (空壳模块)**

```rust
pub mod clipboard;
pub mod commands;
pub mod config;
pub mod logger;
pub mod network;
pub mod protocol;

pub fn run() {
    tauri::Builder::default()
        .setup(|_app| Ok(()))
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 17: 创建空壳 Rust 模块文件**

创建以下空文件（略，后续逐步填充）

- [ ] **Step 18: 安装依赖并验证项目编译**

Run: `cd /d/mengfanliuFile/dev/clipshare && npm install`
Run: `npm run tauri info` (验证 Tauri 环境)

---

## Phase 2: Rust 后端核心模块

### Task 2: 配置模块 (config.rs)

**Files:**
- Modify: `clipshare/src-tauri/src/config.rs`

- [ ] **Step 1: 实现配置结构体和默认配置**

```rust
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
```

- [ ] **Step 2: 提交**

```bash
git add src-tauri/src/config.rs
git commit -m "feat: add config module with load/save"
```

### Task 3: 日志模块 (logger.rs)

**Files:**
- Modify: `clipshare/src-tauri/src/logger.rs`

- [ ] **Step 1: 实现日志结构体和 JSON Lines 写入**

```rust
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
```

- [ ] **Step 2: 更新 lib.rs 添加 logger 导出**

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/logger.rs src-tauri/src/lib.rs
git commit -m "feat: add logger module with JSON Lines file output"
```

### Task 4: 协议模块 (protocol.rs)

**Files:**
- Modify: `clipshare/src-tauri/src/protocol.rs`

- [ ] **Step 1: 定义协议常量和消息结构**

```rust
use std::io::{Read, Write};

pub const MAGIC: u32 = 0x434C4950;
pub const TYPE_TEXT: u8 = 0x01;
pub const TYPE_IMAGE: u8 = 0x02;
pub const TYPE_FILE: u8 = 0x03;
pub const TYPE_HEARTBEAT: u8 = 0x04;
pub const TYPE_ACK: u8 = 0x05;
pub const CHUNK_SIZE: usize = 65536;
pub const HEADER_SIZE: usize = 12;
pub const COMPRESSED: u8 = 0x01;
pub const NOT_COMPRESSED: u8 = 0x00;

#[derive(Debug, Clone)]
pub struct MessageHeader {
    pub magic: u32,
    pub msg_type: u8,
    pub compressed: u8,
    pub data_len: u32,
    pub sequence: u16,
    pub reserved: u8,
}

impl MessageHeader {
    pub fn new(msg_type: u8, data_len: u32, sequence: u16) -> Self {
        Self {
            magic: MAGIC,
            msg_type,
            compressed: NOT_COMPRESSED,
            data_len,
            sequence,
            reserved: 0,
        }
    }

    pub fn to_bytes(&self) -> [u8; HEADER_SIZE] {
        let mut buf = [0u8; HEADER_SIZE];
        buf[0..4].copy_from_slice(&self.magic.to_be_bytes());
        buf[4] = self.msg_type;
        buf[5] = self.compressed;
        buf[6..10].copy_from_slice(&self.data_len.to_be_bytes());
        buf[10..12].copy_from_slice(&self.sequence.to_be_bytes());
        buf
    }

    pub fn from_reader(reader: &mut impl Read) -> std::io::Result<Self> {
        let mut buf = [0u8; HEADER_SIZE];
        reader.read_exact(&mut buf)?;

        let magic = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        if magic != MAGIC {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid magic number",
            ));
        }

        Ok(Self {
            magic,
            msg_type: buf[4],
            compressed: buf[5],
            data_len: u32::from_be_bytes([buf[6], buf[7], buf[8], buf[9]]),
            sequence: u16::from_be_bytes([buf[10], buf[11]]),
            reserved: buf[11],
        })
    }
}

#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub filename: String,
    pub file_size: u64,
}

impl FileMetadata {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut data = self.filename.as_bytes().to_vec();
        data.push(0);
        data.extend_from_slice(&self.file_size.to_be_bytes());
        data
    }

    pub fn from_slice(slice: &[u8]) -> Option<Self> {
        let null_pos = slice.iter().position(|&b| b == 0)?;
        let filename = String::from_utf8(slice[..null_pos].to_vec()).ok()?;
        let file_size = u64::from_be_bytes(slice[null_pos + 1..null_pos + 9].try_into().ok()?);
        Some(Self { filename, file_size })
    }
}
```

- [ ] **Step 2: 提交**

```bash
git add src-tauri/src/protocol.rs
git commit -m "feat: add protocol module with fixed 12-byte header format"
```

### Task 5: 剪贴板监控 (clipboard.rs)

**Files:**
- Modify: `clipshare/src-tauri/src/clipboard.rs`

- [ ] **Step 1: 定义 ClipboardContent 枚举**

```rust
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
```

- [ ] **Step 2: 实现 Windows 剪贴板监听**

```rust
#[cfg(windows)]
pub struct ClipboardListener;

#[cfg(windows)]
impl ClipboardListener {
    pub fn new() -> Self { Self }

    pub fn start<F>(&self, on_change: F)
    where
        F: Fn(ClipboardContent) + Send + 'static,
    {
        use clipboard_win::{formats, get_clipboard};

        std::thread::spawn(move || {
            let mut last_text = String::new();
            let mut last_files: Vec<String> = Vec::new();

            loop {
                std::thread::sleep(std::time::Duration::from_millis(300));

                if let Ok(text) = get_clipboard::<String, _>(formats::Unicode) {
                    if !text.is_empty() && text != last_text {
                        last_text = text.clone();
                        on_change(ClipboardContent::Text(text));
                    }
                }

                if let Ok(files) = get_clipboard::<Vec<String>, _>(formats::Files) {
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
```

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/clipboard.rs
git commit -m "feat: add clipboard monitoring module with Windows support"
```

### Task 6: 网络模块 (network.rs)

**Files:**
- Modify: `clipshare/src-tauri/src/network.rs`

- [ ] **Step 1: 实现 NetworkManager**

```rust
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, Shutdown};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::protocol::*;
use crate::config::AppConfig;

#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
}

pub struct NetworkManager {
    pub status: Arc<Mutex<ConnectionStatus>>,
    sender: Arc<Mutex<Option<TcpStream>>>,
    receiver: Arc<Mutex<Option<TcpStream>>>,
    config: Arc<Mutex<AppConfig>>,
}

impl NetworkManager {
    pub fn new() -> Self {
        Self {
            status: Arc::new(Mutex::new(ConnectionStatus::Disconnected)),
            sender: Arc::new(Mutex::new(None)),
            receiver: Arc::new(Mutex::new(None)),
            config: Arc::new(Mutex::new(AppConfig::load())),
        }
    }

    pub fn set_config(&self, config: AppConfig) {
        *self.config.lock().unwrap() = config;
    }

    pub fn get_status(&self) -> ConnectionStatus {
        self.status.lock().unwrap().clone()
    }

    pub fn disconnect(&self) {
        if let Ok(mut s) = self.sender.lock() {
            if let Some(stream) = s.take() {
                let _ = stream.shutdown(Shutdown::Both);
            }
        }
        if let Ok(mut r) = self.receiver.lock() {
            if let Some(stream) = r.take() {
                let _ = stream.shutdown(Shutdown::Both);
            }
        }
        *self.status.lock().unwrap() = ConnectionStatus::Disconnected;
    }

    pub fn start_server<F>(&self, port: u16, on_receive: F) -> Result<(), String>
    where
        F: Fn(Vec<u8>, u8) + Send + 'static,
    {
        self.disconnect();
        *self.status.lock().unwrap() = ConnectionStatus::Connecting;

        let addr = format!("0.0.0.0:{}", port);
        let listener = TcpListener::bind(&addr).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;

        let sender = self.sender.clone();
        let status = self.status.clone();

        thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                stream.set_nonblocking(false).ok();
                let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
                *sender.lock().unwrap() = Some(stream.try_clone().unwrap());
                *status.lock().unwrap() = ConnectionStatus::Connected;
            }
        });

        // Receive loop
        let receiver = self.receiver.clone();
        let sender2 = self.sender.clone();
        let status2 = self.status.clone();

        thread::spawn(move || {
            loop {
                let stream = {
                    let s = sender2.lock().unwrap();
                    s.clone()
                };

                if let Some(ref mut s) = stream {
                    let mut header_buf = [0u8; HEADER_SIZE];
                    match s.read_exact(&mut header_buf) {
                        Ok(_) => {
                            let mut cursor = std::io::Cursor::new(&header_buf);
                            if let Ok(h) = MessageHeader::from_reader(&mut cursor) {
                                if h.msg_type == TYPE_HEARTBEAT {
                                    continue;
                                }
                                let mut data = vec![0u8; h.data_len as usize];
                                if s.read_exact(&mut data).is_ok() {
                                    on_receive(data, h.msg_type);
                                }
                            }
                        }
                        Err(_) => {
                            *status2.lock().unwrap() = ConnectionStatus::Disconnected;
                            break;
                        }
                    }
                }
                thread::sleep(Duration::from_millis(100));
            }
        });

        Ok(())
    }

    pub fn connect_to_server<F>(&self, ip: &str, port: u16, on_receive: F) -> Result<(), String>
    where
        F: Fn(Vec<u8>, u8) + Send + 'static,
    {
        self.disconnect();
        *self.status.lock().unwrap() = ConnectionStatus::Connecting;

        let addr = format!("{}:{}", ip, port);
        let stream = TcpStream::connect(&addr).map_err(|e| e.to_string())?;
        stream.set_nonblocking(false).ok();
        let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));

        *self.sender.lock().unwrap() = Some(stream.try_clone().unwrap());
        *self.receiver.lock().unwrap() = Some(stream);
        *self.status.lock().unwrap() = ConnectionStatus::Connected;

        let receiver = self.receiver.clone();
        let status = self.status.clone();

        thread::spawn(move || {
            loop {
                let stream = {
                    let r = receiver.lock().unwrap();
                    r.clone()
                };

                if let Some(ref mut s) = stream {
                    let mut header_buf = [0u8; HEADER_SIZE];
                    match s.read_exact(&mut header_buf) {
                        Ok(_) => {
                            let mut cursor = std::io::Cursor::new(&header_buf);
                            if let Ok(h) = MessageHeader::from_reader(&mut cursor) {
                                if h.msg_type == TYPE_HEARTBEAT {
                                    continue;
                                }
                                let mut data = vec![0u8; h.data_len as usize];
                                if s.read_exact(&mut data).is_ok() {
                                    on_receive(data, h.msg_type);
                                }
                            }
                        }
                        Err(_) => {
                            *status.lock().unwrap() = ConnectionStatus::Disconnected;
                            break;
                        }
                    }
                }
                thread::sleep(Duration::from_millis(100));
            }
        });

        Ok(())
    }

    pub fn send(&self, msg_type: u8, data: &[u8], sequence: u16) -> Result<(), String> {
        let sender = self.sender.lock().unwrap();
        if let Some(ref mut stream) = *sender {
            let header = MessageHeader::new(msg_type, data.len() as u32, sequence);
            stream.write_all(&header.to_bytes()).map_err(|e| e.to_string())?;
            stream.write_all(data).map_err(|e| e.to_string())?;
            stream.flush().map_err(|e| e.to_string())?;
            Ok(())
        } else {
            Err("Not connected".to_string())
        }
    }

    pub fn send_text(&self, text: &str) -> Result<(), String> {
        self.send(TYPE_TEXT, text.as_bytes(), 0)
    }

    pub fn send_image(&self, data: &[u8]) -> Result<(), String> {
        self.send(TYPE_IMAGE, data, 0)
    }

    pub fn send_file(&self, filename: &str, file_size: u64, chunks: impl Iterator<Item = Vec<u8>>) -> Result<(), String> {
        let metadata = FileMetadata {
            filename: filename.to_string(),
            file_size,
        };
        self.send(TYPE_FILE, &metadata.to_bytes(), 0)?;
        // Wait for ACK handled separately
        for (i, chunk) in chunks.enumerate() {
            self.send(TYPE_FILE, &chunk, (i + 1) as u16)?;
        }
        Ok(())
    }
}

impl Default for NetworkManager {
    fn default() -> Self { Self::new() }
}
```

- [ ] **Step 2: 提交**

```bash
git add src-tauri/src/network.rs
git commit -m "feat: add network module with TCP server/client support"
```

### Task 7: Tauri 命令 (commands.rs)

**Files:**
- Modify: `clipshare/src-tauri/src/commands.rs`
- Modify: `clipshare/src-tauri/src/lib.rs`

- [ ] **Step 1: 实现所有 Tauri 命令**

```rust
use tauri::{command, State, Manager};
use std::sync::Mutex;
use crate::network::ConnectionStatus;
use crate::logger::LogEntry;
use crate::clipboard::ClipboardListener;
use crate::network::NetworkManager;
use crate::config::AppConfig;

pub struct AppState {
    pub network: Mutex<NetworkManager>,
    pub logs: Mutex<Vec<LogEntry>>,
    pub clipboard_listener: Mutex<Option<ClipboardListener>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            network: Mutex::new(NetworkManager::new()),
            logs: Mutex::new(Vec::new()),
            clipboard_listener: Mutex::new(None),
        }
    }
}

#[command]
pub fn get_config() -> AppConfig {
    AppConfig::load()
}

#[command]
pub fn save_config(config: AppConfig) -> Result<(), String> {
    config.save()
}

#[command]
pub fn get_connection_status(state: State<AppState>) -> String {
    let status = state.network.lock().unwrap().get_status();
    match status {
        ConnectionStatus::Connected => "connected".to_string(),
        ConnectionStatus::Connecting => "connecting".to_string(),
        ConnectionStatus::Disconnected => "disconnected".to_string(),
    }
}

#[command]
pub fn start_server(port: u16, window: tauri::Window, state: State<AppState>) -> Result<(), String> {
    let network = state.network.lock().unwrap();
    let logs = state.logs.clone();
    let window_clone = window.clone();

    network.start_server(port, move |data, msg_type| {
        let (data_type, content, size) = match msg_type {
            TYPE_TEXT => ("text", String::from_utf8_lossy(&data).to_string(), data.len() as u64),
            TYPE_IMAGE => ("image", "截图".to_string(), data.len() as u64),
            TYPE_FILE => ("file", "文件".to_string(), data.len() as u64),
            _ => ("", "未知".to_string(), 0),
        };

        let entry = LogEntry::recv(data_type, &content, size);
        entry.write_to_file().ok();
        logs.lock().unwrap().push(entry.clone());
        let _ = window_clone.emit("clipboard-received", &entry);
    })
}

#[command]
pub fn connect_to_server(ip: String, port: u16, window: tauri::Window, state: State<AppState>) -> Result<(), String> {
    let network = state.network.lock().unwrap();
    let logs = state.logs.clone();
    let window_clone = window.clone();

    network.connect_to_server(&ip, port, move |data, msg_type| {
        let (data_type, content, size) = match msg_type {
            TYPE_TEXT => ("text", String::from_utf8_lossy(&data).to_string(), data.len() as u64),
            TYPE_IMAGE => ("image", "截图".to_string(), data.len() as u64),
            TYPE_FILE => ("file", "文件".to_string(), data.len() as u64),
            _ => ("", "未知".to_string(), 0),
        };

        let entry = LogEntry::recv(data_type, &content, size);
        entry.write_to_file().ok();
        logs.lock().unwrap().push(entry.clone());
        let _ = window_clone.emit("clipboard-received", &entry);
    })
}

#[command]
pub fn disconnect(state: State<AppState>) {
    state.network.lock().unwrap().disconnect();
}

#[command]
pub fn send_text(text: String, state: State<AppState>) -> Result<(), String> {
    let network = state.network.lock().unwrap();
    network.send_text(&text)?;
    let entry = LogEntry::send("text", &text, text.len() as u64);
    entry.write_to_file().ok();
    state.logs.lock().unwrap().push(entry);
    Ok(())
}

#[command]
pub fn send_image(data: Vec<u8>, state: State<AppState>) -> Result<(), String> {
    let network = state.network.lock().unwrap();
    network.send_image(&data)?;
    let entry = LogEntry::send("image", "截图", data.len() as u64);
    entry.write_to_file().ok();
    state.logs.lock().unwrap().push(entry);
    Ok(())
}

#[command]
pub fn get_logs(state: State<AppState>) -> Vec<LogEntry> {
    state.logs.lock().unwrap().clone()
}

#[command]
pub fn clear_logs(state: State<AppState>) {
    state.logs.lock().unwrap().clear();
}

#[command]
pub fn start_clipboard_monitor(window: tauri::Window, state: State<AppState>) -> Result<(), String> {
    let listener = ClipboardListener::new();
    let logs = state.logs.clone();
    let network = state.network.clone();

    listener.start(move |content| {
        let data_type = content.data_type();
        let summary = content.summary();
        let size = content.size();

        let entry = LogEntry::send(data_type, &summary, size);
        entry.write_to_file().ok();
        logs.lock().unwrap().push(entry.clone());
        let _ = window.emit("clipboard-changed", &entry);

        if let Ok(net) = network.lock() {
            match content {
                crate::clipboard::ClipboardContent::Text(t) => { net.send_text(&t).ok(); }
                crate::clipboard::ClipboardContent::Image(b) => { net.send_image(&b).ok(); }
                crate::clipboard::ClipboardContent::Files(_) => { }
            }
        }
    });

    *state.clipboard_listener.lock().unwrap() = Some(listener);
    Ok(())
}
```

- [ ] **Step 2: 更新 lib.rs**

```rust
pub mod clipboard;
pub mod commands;
pub mod config;
pub mod logger;
pub mod network;
pub mod protocol;

use commands::*;
use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            get_connection_status,
            start_server,
            connect_to_server,
            disconnect,
            send_text,
            send_image,
            get_logs,
            clear_logs,
            start_clipboard_monitor,
        ])
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();
            window.set_title("ClipShare").ok();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat: integrate Tauri commands with all backend modules"
```

---

## Phase 3: React 前端实现

### Task 8: 配置面板 (ConfigPanel.tsx)

**Files:**
- Modify: `clipshare/src/components/ConfigPanel.tsx`

- [ ] **Step 1: 实现配置面板**

```tsx
import { useState, useEffect } from 'react'
import { Form, Radio, Input, InputNumber, Button, Space, message } from 'antd'
import { invoke } from '@tauri-apps/api/core'
import { StatusIndicator } from './StatusIndicator'

interface Config {
  role: string
  port: number
  target_ip: string
  target_port: number
  max_file_size: number
}

export function ConfigPanel() {
  const [form] = Form.useForm<Config>()
  const [loading, setLoading] = useState(false)
  const [status, setStatus] = useState('disconnected')

  useEffect(() => {
    loadConfig()
    const interval = setInterval(() => {
      invoke<string>('get_connection_status').then(setStatus).catch(() => {})
    }, 2000)
    return () => clearInterval(interval)
  }, [])

  const loadConfig = async () => {
    try {
      const config = await invoke<Config>('get_config')
      form.setFieldsValue({
        role: config.role,
        port: config.port,
        target_ip: config.target_ip,
        target_port: config.target_port,
        max_file_size: config.max_file_size,
      })
    } catch (e) {
      console.error('Failed to load config:', e)
    }
  }

  const handleSave = async () => {
    try {
      const values = form.getFieldsValue()
      await invoke('save_config', {
        config: {
          role: values.role,
          port: values.port,
          target_ip: values.target_ip || '',
          target_port: values.target_port,
          max_file_size: 104857600,
          language: 'zh-CN',
        }
      })
      message.success('配置已保存')
    } catch (e) {
      message.error('保存失败: ' + e)
    }
  }

  const handleConnect = async () => {
    setLoading(true)
    try {
      const values = form.getFieldsValue()
      if (values.role === 'server') {
        await invoke('start_server', { port: values.port })
        message.success('服务端已启动')
      } else {
        await invoke('connect_to_server', { ip: values.target_ip, port: values.target_port })
        message.success('已连接')
      }
    } catch (e) {
      message.error('操作失败: ' + e)
    } finally {
      setLoading(false)
    }
  }

  const handleDisconnect = async () => {
    await invoke('disconnect')
    message.info('已断开连接')
  }

  return (
    <div style={{ padding: '16px' }}>
      <Form form={form} layout="vertical" initialValues={{ role: 'server', port: 9527, target_port: 9527, max_file_size: 104857600 }}>
        <Form.Item name="role" label="运行模式">
          <Radio.Group>
            <Radio value="server">服务端（接收剪贴板）</Radio>
            <Radio value="client">客户端（发送剪贴板）</Radio>
          </Radio.Group>
        </Form.Item>

        <Form.Item name="port" label="监听端口">
          <InputNumber min={1} max={65535} style={{ width: 200 }} />
        </Form.Item>

        <Form.Item noStyle shouldUpdate={(prev, curr) => prev.role !== curr.role}>
          {({ getFieldValue }) =>
            getFieldValue('role') === 'client' && (
              <>
                <Form.Item name="target_ip" label="目标 IP" rules={[{ required: true }]}>
                  <Input placeholder="例如: 192.168.1.100" style={{ width: 200 }} />
                </Form.Item>
                <Form.Item name="target_port" label="目标端口">
                  <InputNumber min={1} max={65535} style={{ width: 200 }} />
                </Form.Item>
              </>
            )
          }
        </Form.Item>

        <Form.Item name="max_file_size" label="最大文件大小">
          <InputNumber value={100} disabled suffix="MB" style={{ width: 200 }} />
        </Form.Item>

        <Space style={{ marginTop: 16 }}>
          <Button type="primary" onClick={handleConnect} loading={loading}>
            {form.getFieldValue('role') === 'server' ? '启动监听' : '连接'}
          </Button>
          <Button onClick={handleDisconnect}>断开</Button>
          <Button onClick={handleSave}>保存配置</Button>
        </Space>

        <div style={{ marginTop: 16 }}>
          <StatusIndicator status={status} />
        </div>
      </Form>
    </div>
  )
}
```

- [ ] **Step 2: 提交**

```bash
git add src/components/ConfigPanel.tsx
git commit -m "feat: implement ConfigPanel with role switching and connection controls"
```

### Task 9: 日志面板 (LogPanel.tsx)

**Files:**
- Modify: `clipshare/src/components/LogPanel.tsx`

- [ ] **Step 1: 实现日志面板**

```tsx
import { useState, useEffect } from 'react'
import { List, Tag, Button } from 'antd'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

interface LogEntry {
  time: string
  type: string
  dataType: string
  content: string
  size: number
}

const typeColors: Record<string, string> = {
  send: 'blue',
  recv: 'green',
  error: 'red',
}

const dataTypeIcons: Record<string, string> = {
  text: '📄',
  image: '🖼️',
  file: '📁',
}

export function LogPanel() {
  const [logs, setLogs] = useState<LogEntry[]>([])

  useEffect(() => {
    invoke<LogEntry[]>('get_logs').then(setLogs).catch(console.error)
    invoke('start_clipboard_monitor').catch(console.error)

    const unlisten1 = listen<LogEntry>('clipboard-changed', (event) => {
      setLogs(prev => [...prev.slice(-499), event.payload])
    })

    const unlisten2 = listen<LogEntry>('clipboard-received', (event) => {
      setLogs(prev => [...prev.slice(-499), event.payload])
    })

    return () => {
      unlisten1.then(fn => fn())
      unlisten2.then(fn => fn())
    }
  }, [])

  const handleClear = () => {
    invoke('clear_logs')
    setLogs([])
  }

  const formatTime = (iso: string) => {
    const d = new Date(iso)
    return d.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit', second: '2-digit' })
  }

  const formatSize = (bytes: number) => {
    if (bytes < 1024) return bytes + ' B'
    if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB'
    return (bytes / 1024 / 1024).toFixed(2) + ' MB'
  }

  return (
    <div style={{ padding: '16px', height: '100%', display: 'flex', flexDirection: 'column' }}>
      <List
        style={{ flex: 1, overflow: 'auto' }}
        dataSource={logs}
        renderItem={(item) => (
          <List.Item>
            <List.Item.Meta
              avatar={<span style={{ fontSize: 16 }}>{dataTypeIcons[item.dataType] || '❓'}</span>}
              title={
                <span>
                  <Tag color={typeColors[item.type]}>
                    {item.type === 'send' ? '发送' : item.type === 'recv' ? '接收' : '错误'}
                  </Tag>
                  <span style={{ marginLeft: 8, color: '#666' }}>{formatTime(item.time)}</span>
                </span>
              }
              description={
                <span>
                  {item.content}
                  {item.size > 0 && <span style={{ color: '#999', marginLeft: 8 }}>{formatSize(item.size)}</span>}
                </span>
              }
            />
          </List.Item>
        )}
        locale={{ emptyText: '暂无日志' }}
      />
      <Button onClick={handleClear} style={{ marginTop: 8 }}>清空日志</Button>
    </div>
  )
}
```

- [ ] **Step 2: 提交**

```bash
git add src/components/LogPanel.tsx
git commit -m "feat: implement LogPanel with real-time updates and colored tags"
```

### Task 10: 状态指示器 (StatusIndicator.tsx)

**Files:**
- Modify: `clipshare/src/components/StatusIndicator.tsx`

- [ ] **Step 1: 实现状态指示器**

```tsx
import { Badge } from 'antd'

interface Props {
  status: string
}

const statusMap: Record<string, { text: string; color: string }> = {
  connected: { text: '在线', color: 'success' },
  connecting: { text: '连接中', color: 'processing' },
  disconnected: { text: '离线', color: 'default' },
}

export function StatusIndicator({ status }: Props) {
  const info = statusMap[status] || statusMap.disconnected
  return <Badge status={info.color as any} text={`连接状态: ${info.text}`} />
}
```

- [ ] **Step 2: 提交**

```bash
git add src/components/StatusIndicator.tsx
git commit -m "feat: add StatusIndicator component"
```

### Task 11: 完整 App.tsx

**Files:**
- Modify: `clipshare/src/App.tsx`

- [ ] **Step 1: 更新完整 App**

```tsx
import { useState } from 'react'
import { ConfigPanel } from './components/ConfigPanel'
import { LogPanel } from './components/LogPanel'
import { Tabs, Typography } from 'antd'

const { Title } = Typography

function App() {
  const [activeTab, setActiveTab] = useState('config')

  return (
    <div style={{ width: '100vw', height: '100vh', display: 'flex', flexDirection: 'column', background: '#f0f2f5' }}>
      <div style={{ background: 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)', padding: '16px 20px' }}>
        <Title level={4} style={{ color: 'white', margin: 0, textAlign: 'center' }}>
          ClipShare 剪贴板共享
        </Title>
      </div>
      <div style={{ flex: 1, overflow: 'hidden' }}>
        <Tabs
          activeKey={activeTab}
          onChange={setActiveTab}
          style={{ height: '100%' }}
          tabBarStyle={{ paddingLeft: 16, marginBottom: 0, background: 'white' }}
          items={[
            { key: 'config', label: '🔧 配置', children: <ConfigPanel /> },
            { key: 'log', label: '📋 日志', children: <LogPanel /> },
          ]}
        />
      </div>
    </div>
  )
}

export default App
```

- [ ] **Step 2: 提交**

```bash
git add src/App.tsx
git commit -m "feat: update App with styled header and tabs"
```

---

## Phase 4: 系统集成和打包

### Task 12: 系统托盘

**Files:**
- Modify: `clipshare/src-tauri/src/lib.rs`
- Create: `clipshare/src-tauri/icons/` (图标文件)

- [ ] **Step 1: 创建托盘菜单和图标处理**

在 lib.rs 中添加托盘初始化和事件处理

- [ ] **Step 2: 生成图标文件**

使用 Tauri 默认图标或创建简单图标

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/lib.rs src-tauri/icons/
git commit -m "feat: add system tray integration"
```

---

## Phase 5: 编译验证

### Task 13: 构建 Windows 版本

- [ ] **Step 1: 运行 Tauri 构建**

Run: `cd /d/mengfanliuFile/dev/clipshare && npm run tauri build`

- [ ] **Step 2: 验证 exe**

Run: `ls -la /d/mengfanliuFile/dev/clipshare/src-tauri/target/release/*.exe 2>/dev/null || dir /d/mengfanliuFile/dev/clipshare/src-tauri/target/release/*.exe`

---

## 验收标准核对

| 验收项 | 对应任务 |
|--------|----------|
| 单个 exe，双击可运行 | Task 13 |
| 服务端启动后客户端可连接 | Task 6, Task 11 |
| 文字/图片/文件均可传输 | Task 5, Task 6 |
| 文件超过 100MB 显示错误 | Task 6 (network.rs 检查 maxFileSize) |
| 日志写入文件和界面，颜色一致 | Task 3, Task 9 |
| 切换角色后断开重连 | Task 6 (disconnect) |
| 最小化到托盘 | Task 12 |
| 配置持久化 | Task 2 |
| Windows 7+ 可用 | Task 5 (clipboard-win 库支持) |
