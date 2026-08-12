# More Effective Logger 开发环境配置

本仓库为 **Tauri v2 + Vite + TypeScript + xterm.js** 桌面应用（Rust 后端 + Web 前端）。

- 后端：`src-tauri/`（Rust，Tauri v2，portable-pty / ssh2）
- 前端：`src-front/` + Vite 6 + TypeScript（xterm.js、golden-layout）
- 构建产物：前端 → `dist/`，桌面应用由 Cargo 打包

---

## 1. 环境要求

| 组件 | 版本要求 | 说明 |
|---|---|---|
| Node.js | >= 20（本项目在 v22 验证） | 前端构建 |
| npm | >= 10 | 随 Node 自带 |
| Rust | >= 1.77（本项目在 1.97 验证） | 后端编译，用 rustup 管理 |
| 系统库 | 见下文 Linux 依赖 | Tauri v2 编译/运行需要 |

## 2. Linux 系统依赖（Ubuntu 24.04）

```bash
sudo apt update
sudo apt install -y \
  libwebkit2gtk-4.1-dev \
  libgtk-3-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  libxdo-dev \
  build-essential \
  pkg-config \
  curl wget file \
  libssl-dev
```

> 其他发行版请参考 [Tauri 官方 prerequisites](https://v2.tauri.app/start/prerequisites/)。

## 3. 安装 Rust（rustup，免 sudo）

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile default
source "$HOME/.cargo/env"
rustc --version   # 验证
```

> 首次下载较慢，若中断可重新执行 `rustup toolchain install stable --profile default` 断点续传。

## 4. 安装前端依赖

```bash
npm install
```

## 5. 验证环境

```bash
# 前端类型检查 + 构建
npm run build

# 后端编译检查（首次会拉取全部 crate，耗时较长）
cargo check
```

## 6. 日常开发命令

| 命令 | 作用 |
|---|---|
| `npm run tauri dev` | 启动桌面应用开发模式（自动拉起 vite dev server，端口 1420） |
| `npm run dev` | 仅启动前端开发服务器（浏览器调试） |
| `npm run build` | 前端构建到 `dist/` |
| `npm run tauri build` | 打包桌面安装包（release） |

## 7. 常见问题

- **cargo check 报 `pkg-config` 找不到 `webkit2gtk-4.1`**：未安装系统依赖，执行第 2 节命令。
- **首次 `cargo check` 极慢**：正常现象，Tauri 依赖树很大；国内网络可配置镜像（见下）。
- **cargo 下载慢/超时**：配置国内镜像，在 `~/.cargo/config.toml` 写入：

  ```toml
  [source.crates-io]
  replace-with = 'rsproxy-sparse'

  [source.rsproxy-sparse]
  registry = "sparse+https://rsproxy.cn/index/"
  ```

## 8. 当前环境快照（2026-08-12）

| 组件 | 版本 |
|---|---|
| Ubuntu | 24.04.4 LTS |
| Node.js | 22.23.1 |
| npm | 10.9.8 |
| Rust | 1.97.1（stable-x86_64-unknown-linux-gnu） |
| Vite | 6.x |
| webkit2gtk-4.1 | 2.52.3 |
| GTK3 | 3.24.41 |
| pkg-config | 已安装 |
