# ClipShare

ClipShare 是一个面向局域网的跨平台剪切板共享工具，支持实时同步文本、图片和文件，也可以把 Windows 7 云桌面上的微信新消息转发到 Windows 10/11 端进行通知。

## 下载

正式版本：[v0.1.1 Release](https://github.com/JOJO-5/clipshare/releases/tag/v0.1.1)

常用下载项：

- [Win7 x64 安装包](https://github.com/JOJO-5/clipshare/releases/download/v0.1.1/ClipShare.Win7_0.1.0_x64-setup.exe)：推荐在 Windows 7 上使用，已包含固定版本的 WebView2 Runtime。
- [Win7 x64 可执行文件](https://github.com/JOJO-5/clipshare/releases/download/v0.1.1/ClipShare-clipshare-win7-x64.exe)：便携版本，运行环境仍需满足 WebView2 要求。
- [Windows 10/11 安装包](https://github.com/JOJO-5/clipshare/releases/download/v0.1.1/ClipShare_0.1.0_x64-setup.exe)
- [Windows 10/11 MSI](https://github.com/JOJO-5/clipshare/releases/download/v0.1.1/ClipShare_0.1.0_x64_en-US.msi)
- macOS、Linux 及其他构建产物请从 [Release 页面](https://github.com/JOJO-5/clipshare/releases/tag/v0.1.1)选择。

> 当前 `v0.1.1` Release 中部分安装器文件名仍带有 `0.1.0`，这是应用包内部版本字段尚未同步；下载时以 Release 标签和文件用途为准。

## 功能

- 文本、图片和文件实时传输，单条消息最大 100 MB。
- Server/Client 两种工作模式，默认端口为 `9527`。
- TCP 协议 ACK、NACK、心跳检测和断线自动重连。
- 接收端保存文件到 `下载/ClipShare`，并自动清理过旧的接收文件。
- 支持最小化到系统托盘、托盘恢复和当前用户开机自启。
- 日志显示连接状态、剪切板读取、发送、接收和失败重试信息。

## 微信消息通知

微信监控通常运行在 Windows 7 华为云桌面上，Windows 10/11 端负责接收并显示系统通知：

1. Win7 端通过 Windows UI Automation 读取微信未读会话和消息。
2. Win7 端将微信消息通过 ClipShare 网络连接发送到监听端。
3. Win10/11 端按发送人合并连续消息，并显示系统通知。

通知行为：

- 短消息会显示预览，长消息会截断显示。
- 连续消息会显示“X 条新消息”和最新预览，避免通知气泡刷屏。
- 同一发送人持续有新消息时，系统通知最多每 30 秒再次提醒一次。
- 点击通知后，在 ClipShare 窗口中查看该组完整消息；软件最小化到托盘时需要先打开 ClipShare。
- 微信自身的“消息免打扰”不会自动关闭 ClipShare 提醒，因为 ClipShare 读取的是微信 UI 中的未读状态，而不是微信系统通知。

微信 UI 结构会随微信版本变化。日志中的 `wechat-ui`、`wechat-ui-tree`、`msaa_*` 和 `activation_*` 字段可用于定位 UI Automation、Raw View 或 MSAA 兼容问题。

## 系统兼容性

| 平台 | 构建 / 运行说明 |
| --- | --- |
| Windows 7 x64 | 使用独立的 `ClipShare Win7` 构建，Rust target 为 `x86_64-win7-windows-msvc`，安装包内置 WebView2 109 Runtime。 |
| Windows 10/11 x64 | 使用现代 Windows 构建，需要系统已安装或能够安装 WebView2 Runtime。 |
| macOS | 提供 Intel 和 Apple Silicon 构建。 |
| Linux | 提供 x64 AppImage 和 deb 包。 |

Win7 构建使用 `uiautomation = 0.23.0`、`win7-compat` Cargo feature 和兼容 WebView2 loader，避免引用 Windows 7 不存在的 API。Win7 与现代 Windows 包不能混用。

## 使用方式

1. 在接收端选择“接收端”，设置监听端口并启动监听。
2. 在发送端选择“发送端”，填写接收端局域网 IP 和端口并连接。
3. 连接成功后，复制文本、图片或文件即可传输。
4. 若启用微信消息监控，在 Win7 端保持微信登录并运行；Win10/11 端保持 ClipShare 连接即可接收通知。

发送端在接收端暂时离线时会自动重试。两端应使用本项目的同一版本或兼容版本，并确保防火墙允许配置端口的 TCP 连接。

## 开发环境

- Node.js 20+
- Rust stable toolchain
- Windows 构建需要 Visual Studio 2022 C++ 桌面开发工具
- Windows 运行和打包需要 WebView2 相关运行环境

安装依赖：

```bash
npm install
```

启动开发环境：

```bash
npm run tauri dev
```

## 本地构建与测试

前端构建：

```bash
npm run build
```

默认 Tauri 构建：

```bash
npm run tauri build
```

Windows 10/11 构建：

```bash
npm run tauri build -- --config src-tauri/tauri.modern.conf.json
```

Win7 构建需要 nightly Rust、`rust-src` 和 Win7 target：

```powershell
rustup toolchain install nightly-2026-07-22 --profile minimal --component rust-src
$env:RUSTUP_TOOLCHAIN = "nightly-2026-07-22"
npm run build
cargo build --manifest-path src-tauri/Cargo.toml --release --target x86_64-win7-windows-msvc --bin clipshare --features win7-compat
```

运行 Rust 测试：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

## GitHub Actions

推送 `v*` 标签会触发 Windows 7、Windows 10/11、macOS Intel、macOS Apple Silicon 和 Linux 多平台构建，并自动创建正式 GitHub Release。构建产物会统一整理到 Release 页面，Actions 临时 artifact 仅保留较短时间。

## 项目结构

```text
src/                              React 界面、配置和日志展示
src-tauri/src/                    Rust 命令、剪切板、网络协议和微信监控
src-tauri/tauri.win7.conf.json    Windows 7 构建配置
src-tauri/tauri.modern.conf.json  Windows 10/11 构建配置
.github/workflows/build.yml       多平台构建和 Release 工作流
tests/                            Win7、功能回归和构建配置检查
```
