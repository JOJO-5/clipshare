# ClipShare

## Tray and startup

- The configuration page can hide the window to the tray when it is closed or minimized.
- The tray menu restores the main window or exits ClipShare.
- “Autostart” uses the current user's Windows Run entry and launches with `--minimized`.
- The Win7 flavor pins `uiautomation = 0.23.0` so the executable does not import the missing `combase.dll`.
- The Win7 flavor also uses the `win7-compat` Cargo feature, which leaves the modern WinRT notification plugin out of that binary; Win10/11 remains the notification display endpoint.
- The Win7 installer bundles the fixed WebView2 109 runtime because newer WebView2 bootstrapper versions call APIs that do not exist on Windows 7.
- The Win7 executable links the Microsoft WebView2 SDK 1.0.1054.31 loader so it also starts on Windows 7 installations where `EventSetInformation` is absent. CI inspects the final PE import table to prevent this compatibility regression.
- Client connections remain in `Connecting` and retry with backoff until the receiver is available. After a TCP disconnect, the client reconnects automatically and the server continues accepting a replacement client.
- On every launch, ClipShare restores the last saved role automatically: receivers resume listening and senders reconnect to the last target. Clicking “Start listening” or “Connect” also persists that endpoint for the next restart.
- Connections now use a protocol ACK plus 5-second heartbeats. A silent or half-open peer is detected within about 15 seconds, and blocking connect/write operations have timeouts.
- The log panel records `network-status` transitions and their reason, such as `protocol ack received`, `heartbeat timeout`, `peer closed connection`, or `send failed`.
- Both peers must run this or a newer build because the protocol ACK and heartbeat checks are not compatible with older ClipShare builds.
- The log panel records `clipboard-read`, `clipboard-send`, and periodic `clipboard-monitor alive` diagnostics, including failed sends and retry attempts.

ClipShare 是一个面向局域网的剪贴板共享工具，使用 Tauri、React、TypeScript 和 Rust 构建。连接后，设备之间可以实时同步文本、图片和文件。

## 功能

- 文本、RGBA 图片和文件实时传输
- TCP 服务端/客户端两种工作模式
- 连接状态和传输日志
- 微信消息提示：Win7 端通过 UI Automation 识别旧版微信 `ChatWnd` 或微信 4.0 的 Qt/`mmui::*` 消息列表，Win10/11 端显示系统通知
- 通知显示短摘要，点击后查看完整微信消息
- 单条消息最大传输大小为 100 MB

## 系统兼容性

- Windows 7：使用独立的 `ClipShare Win7` 可执行文件，采用 Rust 官方 `x86_64-win7-windows-msvc` target 构建；运行前需准备兼容的 WebView2 运行时
- Windows 10/11：使用 `ClipShare` 构建包，使用系统或在线安装的 WebView2
- macOS：Intel 和 Apple Silicon
- Linux：x64

Win7 微信监听依赖 Windows UI Automation。旧版微信优先定位 `ChatWnd`；微信 4.0 Qt 客户端会通过原生 HWND 重新连接 UIA Provider，再使用 Raw View 中的 `mmui::ChatSessionList`、`mmui::MessageView` 和 `mmui::Chat*ItemView`，根据未读会话数量提取最后的新消息，并使用 Runtime ID 去重。若 Qt 客户端的 UIA Control/Raw View 都为空，程序会枚举原生子窗口并尝试 Win7 自带的 MSAA/`IAccessible` 兜底。当前 4.0 兼容逻辑参考了公开的 wxauto4 4.0.5 控件结构；微信升级后控件名仍可能变化。日志中的 `handle_rebound_used`、`raw_descendants`、`native_classes`、`msaa_nodes`、`msaa_error` 和 `msaa_sample` 可用于判断微信暴露了哪一层可访问性数据。华为云桌面需要保持用户会话运行，注销或没有交互桌面时监听可能暂停。

## 开发环境

- Node.js 20+
- Rust stable toolchain
- Windows 开发需要 Visual Studio 2022 C++ 桌面开发工具
- Windows 运行和打包需要 WebView2 相关运行时

安装依赖：

```bash
npm install
```

启动开发环境：

```bash
npm run tauri dev
```

## 本地构建

构建前端：

```bash
npm run build
```

构建默认 Tauri 包：

```bash
npm run tauri build
```

构建现代 Windows 包：

```bash
npm run tauri build -- --config src-tauri/tauri.modern.conf.json
```

Win7 包需要 nightly Rust、`rust-src` 和官方 Win7 target 的 `build-std`：

```bash
rustup toolchain install nightly --profile minimal --component rust-src
$env:RUSTUP_TOOLCHAIN = "nightly"
npm run build
cargo build --manifest-path src-tauri/Cargo.toml --release --target x86_64-win7-windows-msvc --bin clipshare
```

现代 Windows 构建产物位于 `src-tauri/target/release/bundle/`；Win7 构建产物为 `src-tauri/target/x86_64-win7-windows-msvc/release/clipshare.exe`。

## 测试

运行 Rust 单元测试和网络测试：

```bash
cd src-tauri
cargo test --lib
```

检查前端类型并构建：

```bash
npm run build
```

## GitHub Actions 和下载

推送 `v*` tag 会触发多平台构建，并自动创建 Draft Release。构建产物包括：

- `clipshare-win7-x64`
- `clipshare-windows-x64`
- `clipshare-macos-x64`
- `clipshare-macos-arm64`
- `clipshare-linux-x64`

正式安装包请从 GitHub 的 [Releases](https://github.com/JOJO-5/clipshare/releases) 下载；Actions artifact 仅作为短期构建结果保留。

## 使用方式

1. 设备 A 选择“接收端”，设置监听端口并开始监听。
2. 设备 B 选择“发送端”，填写设备 A 的局域网 IP 和端口，然后连接。
3. 连接成功后，复制文本、图片或文件即可传输。
4. 启用微信消息提示后，Win7 端检测到微信新消息会发送到监听端，Win10/11 端显示通知。

默认端口为 `9527`。成功执行一次“开始监听”或“连接设备”后，当前角色、IP 和端口会自动保存；后续启动时接收端自动监听、发送端自动连接。如果接收端尚未启动，发送端会保持重试。使用局域网 IP 连接时，请确保防火墙允许该端口的 TCP 入站连接。

## 项目结构

```text
src/                         React 界面、配置和日志展示
src-tauri/src/               Rust 命令、剪贴板、TCP 协议和微信监听
src-tauri/tauri.conf.json    默认 Tauri 窗口和打包配置
src-tauri/tauri.win7.conf.json   Win7 flavor 配置
src-tauri/tauri.modern.conf.json Win10/11 flavor 配置
.github/workflows/           多平台构建和 Release 工作流
```
