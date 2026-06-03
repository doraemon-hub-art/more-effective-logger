# More Effective Logger 架构文档

## 概述

More Effective Logger 是一个基于 **Tauri v2 + xterm.js** 的多面板日志与终端工作台。用户可以在一个无边框窗口中自由创建终端和日志面板，所有面板通过 GoldenLayout 实现拖拽切分布局。

## 技术栈

| 层 | 技术 | 说明 |
|---|------|------|
| 桌面壳体 | Tauri v2 | Rust + WebView，无边框窗口，跨平台 |
| 终端渲染 | xterm.js 5.5 + WebGL addon | VS Code 同款终端核心，GPU 加速 |
| 面板布局 | GoldenLayout 2.6 | 多面板拖拽切分布局引擎 |
| 本地 PTY | portable-pty 0.8 | WezTerm 作者维护，跨平台 PTY 抽象 |
| 远程日志 | ssh2 0.9 | SSH + tail -f 远程文件监听 |
| 配置 | serde + toml (Rust) | ~/.config/melogger/config.toml |

## 项目结构

```
more-effective-logger/
├── Cargo.toml                  # workspace root → src-tauri
├── index.html                  # 前端入口 + 拖拽条
├── package.json                # npm 脚本 (npm run tauri)
├── vite.config.ts
├── tsconfig.json
├── src-front/
│   ├── main.ts                 # GoldenLayout + xterm.js + Tauri IPC + Log Pane
│   └── theme.ts                # Catppuccin Mocha 主题色
├── docs/
│   └── architecture.md         # 本文档
└── src-tauri/
    ├── Cargo.toml              # Rust 依赖
    ├── tauri.conf.json         # 窗口配置（无边框、透明、拖拽）
    └── src/
        ├── main.rs             # 入口
        ├── lib.rs              # PTY 管理 + SSH 日志 + Tauri commands
        └── config.rs           # 配置解析 (TOML → AppConfig)
```

## 架构图

```
+-------------------------------------------------------------------+
|                        UI 表现层 (Frontend)                        |
|                                                                   |
|   +-----------------------------+ +-----------------------------+  |
|   |        Terminal Pane        | |        Log Pane             |  |
|   |  - xterm.js + WebGL addon   | |  - HTML 表单 + 表格          |  |
|   |  - FitAddon 自适应          | |  - SSH 远程 tail -f          |  |
|   +-----------------------------+ +-----------------------------+  |
|   |                      Layout: GoldenLayout                    |  |
|   +-------------------------------------------------------------+  |
+-------------------------------------------------------------------+
                                 ▲
                     Tauri IPC (invoke + listen)
                                 ▼
+-------------------------------------------------------------------+
|                        核心逻辑层 (Rust Backend)                   |
|                                                                   |
|   +-------------------+ +-------------------+ +-----------------+ |
|   |   PTY Controller  | |   SSH Log Stream  | |  Config Manager | |
|   |   (portable-pty)  | |   (ssh2 + tail)   | |  (serde/toml)   | |
|   +-------------------+ +-------------------+ +-----------------+ |
+-------------------------------------------------------------------+
```

## 数据流

### 终端数据流

```
键盘输入 → xterm.js onData → invoke("pty_input") → Rust pty_input()
                                                         │ write()
                                                         ▼
                                                     Shell 进程
                                                         │ output
                                                         ▼
                                                 Rust reader thread
                                                         │ emit("pty-output")
xterm.js term.write(data) ← listen("pty-output") ←──────┘
    │
    ▼
WebGL 渲染 → 屏幕字符网格
```

### 日志数据流

```
Log Pane 表单 → invoke("log_connect", {host,port,user,pass,file})
                     │
                     ▼
              Rust: SSH 连接 → tail -f → 逐行读取
                     │
                     ├── emit("log-status", "connected")
                     └── emit("log-line", {line})
                              │
                              ▼
                     Log Pane: 按空格解析 → 表格 append
```

## Rust 后台 Commands

| Command | 参数 | 功能 |
|---------|------|------|
| `spawn_terminal` | termId, cols, rows | 创建 PTY 终端 |
| `pty_input` | termId, data | 键盘输入 → PTY |
| `pty_resize` | termId, cols, rows | 窗口 resize → PTY |
| `log_connect` | logId, host, port, user, pass, file | SSH 连接 + tail -f |
| `log_stop` | logId | 停止日志监听 |
| `get_config` | - | 返回 AppConfig |

## 快捷键

| 按键 | 功能 |
|------|------|
| `Ctrl+Shift+T` | 新建 Terminal Pane |
| `Ctrl+Shift+L` | 新建 Log Pane |
| `Ctrl +` / `Ctrl -` | 全局字体缩放 |

## 配置文件

路径: `~/.config/melogger/config.toml`

```toml
[window]
width = 1200
height = 800
opacity = 0.95

[font]
family = "JetBrains Mono"
size = 13

[theme]
background = "#1e1e2e"
foreground = "#cdd6f4"

[layout]
default_type = "single_terminal"
```

启动时自动加载，文件不存在则自动生成默认配置。
