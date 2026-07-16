# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

ClipShare is a Tauri-based cross-platform clipboard sharing tool for intranets. Machines can sync text/images/files in real-time via a custom TCP protocol.

**Stack:** Tauri 2.x | React 18 + Ant Design 5 + TypeScript (frontend) | Rust (backend)

## Commands

```bash
# Frontend only (hot reload)
npm run dev

# Full Tauri app (frontend + Rust backend)
npm run tauri dev

# Production build
npm run build
npm run tauri build
```

## Architecture

### Hybrid Server/Client Model
Each peer operates as both server and client but takes only one role at a time:
- **Server**: Listens on TCP port (default 9527), waits for connections
- **Client**:主动连接其他机器

Role switching disconnects current connection and reconnects as the new role.

### Network Protocol
Custom TCP with 12-byte fixed header:
```
[4-byte magic: 0x434C4950] [1-byte type] [1-byte compressed] [6-byte length] [2-byte sequence] [1-byte reserved]
```

Message types: TEXT (0x01), IMAGE (0x02), FILE (0x03), HEARTBEAT (0x04), ACK (0x05)

### Module Structure

**Frontend (`src/`):**
- `App.tsx` — Main layout with tab navigation (Config/Log)
- `components/ConfigPanel.tsx` — Role/port/IP settings + connection control
- `components/LogPanel.tsx` — Real-time log display with color coding
- `components/StatusIndicator.tsx` — Connection status indicator

**Backend (`src-tauri/src/`):**
- `clipboard.rs` — Clipboard monitoring (Windows API `AddClipboardFormatListener`)
- `network.rs` — TCP server/client with async I/O (tokio)
- `protocol.rs` — Message encoding/decoding, compression
- `commands.rs` — Tauri command handlers (invoked from frontend)
- `config.rs` — Config file loading/saving (`~/.clipshare/config.json`)
- `logger.rs` — JSONL log writing (`~/.clipshare/logs/`)
- `lib.rs` — Module exports, command registration
- `main.rs` — Entry point

### Clipboard Monitoring
- **Windows**: `AddClipboardFormatListener` API for real-time notifications
- **Linux**: `wl-paste` (Wayland) / `xclip` (X11) via subprocess polling every 300ms

### Data Flow
1. Clipboard change detected → encode with protocol header → compress if needed → send via TCP
2. TCP data received → decompress → decode header → invoke frontend handler → write log

## Key Files

| File | Purpose |
|------|---------|
| `src-tauri/tauri.conf.json` | Window config (600x500, min 480x400), tray icon, bundle settings |
| `src-tauri/Cargo.toml` | Rust deps: tauri, tokio, clipboard-win, arboard, flate2 |
| `src/components/ConfigPanel.tsx` | Role toggle, port/IP inputs, connect button |
| `src/components/LogPanel.tsx` | Log list with send(blue)/recv(green)/error(red) coloring |

## Tauri IPC

Frontend invokes Rust commands via `@tauri-apps/api`:
```typescript
import { invoke } from '@tauri-apps/api';
await invoke('start_listening', { port: 9527 });
await invoke('connect_to', { ip: '192.168.1.100', port: 9527 });
await invoke('send_clipboard', { dataType: 'text', content: '...' });
```

Commands defined in `src-tauri/src/commands.rs` and registered in `lib.rs`.

## Build Artifacts

- Frontend: `dist/` (Vite build output)
- Rust: `src-tauri/target/release/clipshare.exe`
- Full installer: `src-tauri/target/release/bundle/`
