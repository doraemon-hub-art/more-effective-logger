# more-effective-logger

致力于打造更好用的日志查看工具。

致敬 C++ 的经典书籍，《More Effective xx》 系列。

---

当前为 **Tauri v2 最小前后端骨架** 阶段：无业务逻辑，仅验证前后端 IPC 通信。

## 技术栈

| 层 | 技术 |
|---|------|
| 桌面壳体 | Tauri v2 (Rust) |
| 前端 | Vite + React 19 + TypeScript + TailwindCSS v4 |

## 目录结构

```
├── index.html                  # Vite 入口
├── src/                        # React 前端
│   ├── main.tsx                # React 挂载
│   ├── App.tsx                 # 测试页：Test IPC Connection
│   ├── index.css               # Tailwind v4
│   ├── components/             # 通用组件（空）
│   └── layout/                 # 布局组件（空）
├── src-tauri/                  # Rust 后端
│   ├── src/
│   │   ├── main.rs             # 入口（仅调用 app_lib::run）
│   │   ├── lib.rs              # Builder 装配：挂载 AppState + 注册命令
│   │   ├── state.rs            # 全局 AppState（Mutex 占位）
│   │   ├── commands.rs         # IPC 命令（app_ping）
│   │   └── modules/            # 业务子模块目录（空）
│   └── tauri.conf.json
└── docs/                       # 设计文档
```

## 开发

```bash
npm install          # 安装前端依赖
npm run tauri dev    # 一键启动（先起 Vite 再起 Rust）
```

## IPC 验证

前端 `src/App.tsx` 点击 "Test IPC Connection" → 调用 Rust 命令 `app_ping` → 返回 `Pong from Rust!` 显示在页面上，即前后端打通。
