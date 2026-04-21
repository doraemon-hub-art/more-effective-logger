# 当前项目结构梳理

本文档用于记录 `more-effective-logger` 当前代码结构和运行现状，避免聊天上下文丢失后难以快速恢复项目认知。

## 1. 项目目标

当前项目是一个基于 `Rust + Slint` 的桌面日志工具，核心目标：

- 配置并保存一个全局 SSH 会话（设备地址、用户名、密码）
- 基于该会话读取远端日志文件（tail 场景）
- 基于该会话在 Terminal 页面执行远端命令
- 为后续多窗口布局（类似 Terminator）预留结构

## 2. 目录与模块职责

### `src/main.rs`

应用主编排入口，负责：

- 创建窗口与 UI model
- 初始化 channel 与定时器
- 维护全局 `current_session`
- 绑定 UI 回调（connect/start/stop/execute 等）
- 启动日志读取与命令执行线程

### `src/session.rs`

会话领域模型：

- `Session { host, username, password }`
- `Session::new(...)`
- `from_wire()/to_wire()`（历史兼容，当前主流程已弱化依赖）

### `src/config.rs`

配置持久化模块：

- `Config::load()` 加载配置
- `Config::save_session(session)` 保存会话
- 配置路径：`~/.config/melogger/user_config.toml`

### `src/ssh.rs`

SSH 能力模块：

- `open_session()`：统一建连与认证
- `file_exists()`：检查远端文件是否存在
- `tail_file()`：按指定文件启动日志流
- `execute_single_command()`：执行单条终端命令并返回输出

说明：文件中仍有旧接口（如 `tail_logs` / `execute_shell_command`），当前主流程已不依赖它们。

### `src/window.rs`

窗口抽象层（扩展预留）：

- `WindowKind`：窗口类型枚举
- `UiWindow`：对 `AppWindow` 的轻量封装
- `WindowFactory`：统一窗口创建入口

当前仅实际创建主窗口，其他类型用于后续多窗口扩展。

### `src/parser.rs`

日志解析模块：

- 使用正则解析日志文本为结构化 `LogEntry`
- 已含基础单元测试

### `src/filter.rs`

过滤器模块：

- `FilterRule`、`FilterManager`、匹配逻辑
- 当前尚未接入主日志展示链路

### `ui/app.slint`

UI 定义文件（单窗口多页面）：

- Logs 页面
- Session 页面
- Settings 页面
- Terminal 页面

## 3. 当前关键行为（已实现）

### 3.1 全局 Session

- 用户在 Session 页面点击 `Connect`
- 运行时更新全局会话 `current_session`
- 同时写入配置文件
- 后续 Log 和 Terminal 都基于该全局会话执行

### 3.2 Log 页面交互

- 已移除 `OK` 按钮，仅保留 `Start/Stop`
- 点击 `Start` 时先检查远端文件存在性
  - 存在：开始读取日志，状态切为连接中（按钮显示 `Stop`）
  - 不存在：不启动读取，不做其他动作

### 3.3 Terminal 页面交互

- 输入命令后，基于当前全局会话执行
- 输出通过 channel 回推到 UI 列表

## 4. 当前边界与待优化项

### 4.1 `Stop` 行为

目前 `Stop` 是 UI 层停止显示/状态切换；底层 tail 任务尚未做成“可取消句柄”的硬中断。

### 4.2 过滤器未闭环

`filter.rs` 已实现规则逻辑，但还未接入日志显示主链路。

### 4.3 文档一致性

`docs/config.md` 仍提到 `config.toml`，需与当前实现路径 `user_config.toml` 对齐。

## 5. 建议下一步（按优先级）

1. 实现可取消的 tail 任务（真正的 `Stop`）
2. 接入 `filter.rs` 到日志展示流程
3. 同步修正文档中的配置文件路径
4. 逐步落地多窗口：`LogWindow` / `TerminalWindow` / `SessionWindow`

---

更新说明：本文档根据当前代码现状整理，后续每次关键结构改动后应同步更新本文件。
