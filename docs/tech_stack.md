# More Effective Logger 技术栈

> 记录本项目用到的开发语言与技术。前端依赖版本以 `package.json` 为准，后端以 `src-tauri/Cargo.toml` 为准。只有真正被上一级包含的技术才做二级列表。

## 前端（src/）

- TypeScript ^5.7 — 前端语言（JavaScript 加编译期类型检查）
- React ^19 — UI 框架（用 TS/JS 编写）
- Vite ^6 — 开发服务器与打包工具
- Tailwind CSS v4 — 原子化样式工具类（经 @tailwindcss/vite 插件接入）
- @tauri-apps/api ^2.11 — 调用 Rust IPC 的桥（invoke / listen）
- xterm.js 5.5 — 终端渲染（规划引入，VS Code 同款，WebGL addon 加速）
- GoldenLayout 2.6 — 多面板拖拽切分布局（规划引入）

## 后端（src-tauri/）

- Rust（语言，edition 2021，rust-version 1.77）
  - Tauri v2 — 桌面应用框架（Rust 后端 + 系统 WebView，Linux 上为 WebKitGTK）
  - tauri-build ^2 — Tauri 构建脚本（build.rs）
  - ssh2 0.9 — SSH 客户端库，远程日志监听 / 执行命令（规划引入）
  - portable-pty 0.8 — 本地终端 PTY（规划引入，WezTerm 作者维护）
  - serde + toml — 配置解析（规划引入）

## 常用命令

- `npm run dev` — 只起前端，浏览器热更
- `npm run tauri dev` — 整机调试（Rust + 前端 + 窗口）
- `npm run build` — 前端类型检查 + 打包

---

更新记录：2026-09-03 建文档，前后端分组，仅后端 crate 做二级。
