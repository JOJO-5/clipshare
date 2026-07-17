# ClipShare

ClipShare 是一个面向局域网的剪贴板共享工具，使用 Tauri、React、TypeScript 和 Rust 构建。两台设备连接后，可以在设备之间同步文本、图片和文件。

## 功能

- 文本剪贴板实时同步
- RGBA 图片传输与接收写回剪贴板
- 文件剪贴板传输，接收文件保存到 `下载/ClipShare`
- TCP 连接状态显示、断线处理和传输日志
- 支持监听端和主动连接端两种工作模式
- 单条消息最大传输大小为 100 MB

## 开发环境

- Node.js 20+
- Rust stable toolchain
- Windows 开发需要 Visual Studio 2022 C++ 桌面开发工具和 WebView2
- Tauri 2.x 所需的系统依赖

安装前端依赖：

```bash
npm install
```

开发模式：

```bash
npm run tauri dev
```

## 构建

构建前端：

```bash
npm run build
```

构建 Tauri 安装包：

```bash
npm run tauri build
```

Windows 也可以使用仓库中的脚本：

```bat
build-full.bat
```

构建产物位于 `src-tauri/target/release/bundle/`，包括 MSI 和 NSIS 安装包。

## 测试

运行 Rust 单元和网络回归测试：

```bash
cd src-tauri
cargo test --lib
```

前端类型检查和生产构建：

```bash
npm run build
```

## 使用方式

1. 在设备 A 选择“接收端”，设置监听端口并开始监听。
2. 在设备 B 选择“发送端”，填写设备 A 的局域网 IP 和端口，然后连接设备。
3. 连接状态显示为“在线”后，复制文本、图片或文件即可传输。
4. 在“传输日志”页查看发送和接收记录。

默认端口为 `9527`。使用局域网 IP 连接时，请确保防火墙允许该端口的 TCP 入站连接。

## 项目结构

```text
src/                  React 界面和日志展示
src-tauri/src/        Rust 命令、剪贴板、TCP 网络和协议实现
src-tauri/tauri.conf.json  Tauri 窗口和打包配置
.github/workflows/    多平台构建工作流
```

当前主要在 Windows 环境验证。非 Windows 平台的剪贴板写入和监听仍需要补充对应平台实现。
