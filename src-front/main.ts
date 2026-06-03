import { GoldenLayout, type ComponentContainer } from "golden-layout";
import "golden-layout/dist/css/goldenlayout-base.css";
import "golden-layout/dist/css/themes/goldenlayout-dark-theme.css";
import "@xterm/xterm/css/xterm.css";
import { Terminal } from "@xterm/xterm";
import { WebglAddon } from "@xterm/addon-webgl";
import { FitAddon } from "@xterm/addon-fit";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import THEME from "./theme";

// ── 窗口拖拽（data-tauri-drag-region 在 Linux 上不生效，用 JS 处理）──

document.addEventListener("DOMContentLoaded", () => {
  document.getElementById("drag-bar")?.addEventListener("mousedown", (e) => {
    if (e.button === 0) { e.preventDefault(); getCurrentWindow().startDragging(); }
  });
});

let fontSize = 13;
const termInstances: Terminal[] = [];

function setFontSize(delta: number) {
  fontSize = Math.max(8, Math.min(24, fontSize + delta));
  termInstances.forEach(t => { t.options.fontSize = fontSize; });
}

// ── Terminal Pane ──────────────────────────────

let nextTermId = 1;

function createTerminalPane(container: ComponentContainer) {
  const term = new Terminal({
    fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
    fontSize, theme: THEME,
    cursorBlink: true, cursorStyle: "bar", allowProposedApi: true,
    scrollback: 5000,  // 限制滚动缓冲区，防止内存无限增长
  });
  termInstances.push(term);
  const fitAddon = new FitAddon(); term.loadAddon(fitAddon);
  try { term.loadAddon(new WebglAddon()); } catch (_) {}

  const el = container.element; el.style.cssText = "width:100%;height:100%;display:flex;";
  const div = document.createElement("div"); div.style.cssText = "width:100%;height:100%;";
  el.appendChild(div); term.open(div);
  setTimeout(() => fitAddon.fit(), 50);
  container.on("resize", () => {
    setTimeout(() => fitAddon.fit(), 10);
    const dims = fitAddon.proposeDimensions();
    if (dims) invoke("pty_resize", { termId, cols: dims.cols, rows: dims.rows });
  });

  const termId = nextTermId++;

  // 键盘输入批量合并：输入立即收集，Enter 或 5ms 超时触发发送
  let inputBatch = "";
  let inputTimer: any = null;
  const flushInput = () => {
    if (inputBatch.length > 0) {
      invoke("pty_input", { termId, data: inputBatch });
      inputBatch = "";
    }
    if (inputTimer) { clearTimeout(inputTimer); inputTimer = null; }
  };
  term.onData((data) => {
    inputBatch += data;
    if (data === "\r") {
      // Enter 立刻发送，保证命令即时执行
      flushInput();
    } else {
      if (inputTimer) clearTimeout(inputTimer);
      inputTimer = setTimeout(flushInput, 5);
    }
  });

  let unlisten: (() => void) | null = null;
  let rafPending = false;
  listen("pty-output", (event: any) => {
    if (event.payload?.termId === termId) {
      const atBottom = term.buffer.active.viewportY === term.buffer.active.baseY;
      // 大批量数据用 RAF 分批写入，避免阻塞渲染
      const data = event.payload.data;
      term.write(data, () => {
        if (atBottom) term.scrollToBottom();
      });
    }
  }).then((fn) => { unlisten = fn; });

  setTimeout(() => {
    fitAddon.fit();
    const dims = fitAddon.proposeDimensions();
    invoke("spawn_terminal", { termId, cols: dims?.cols ?? 80, rows: dims?.rows ?? 24 }).catch((e) => {
      term.writeln(`\x1b[31m[PTY Error]\x1b[0m ${e}`);
    });
  }, 100);

  container.on("destroy", () => {
    if (unlisten) unlisten();
    termInstances.splice(termInstances.indexOf(term), 1);
    term.dispose();
  });
}

// ── Log Pane ───────────────────────────────────

let nextLogId = 1;
const logLinesMap = new Map<number, HTMLElement>();

function createLogPane(container: ComponentContainer) {
  const logId = nextLogId++;
  const el = container.element; el.style.cssText = "width:100%;height:100%;display:flex;flex-direction:column;background:#1e1e2e;color:#cdd6f4;font-size:12px;font-family:monospace;overflow:hidden;";

  // ── 工具栏 ──
  const toolbar = document.createElement("div");
  toolbar.style.cssText = "display:flex;gap:6px;padding:8px;background:#181825;border-bottom:1px solid #313244;flex-shrink:0;flex-wrap:wrap;align-items:center;";

  const inputs: Record<string, HTMLInputElement> = {};
  function input(placeholder: string, value: string, width: string, type = "text") {
    const i = document.createElement("input");
    i.placeholder = placeholder; i.value = value;
    i.style.cssText = `width:${width};background:#313244;color:#cdd6f4;border:1px solid #45475a;border-radius:3px;padding:2px 6px;font-size:12px;font-family:monospace;outline:none;`;
    if (type === "password") i.type = "password";
    toolbar.appendChild(i);
    return i;
  }
  inputs["host"]    = input("Host", "192.168.1.98", "110px");
  inputs["port"]    = input("Port", "22", "50px");
  inputs["user"]    = input("User", "root", "80px");
  inputs["pass"]    = input("Pass", "", "80px", "password");
  inputs["logfile"] = input("File path", "/tmp/running.INFO", "180px");

  const statusEl = document.createElement("span");
  statusEl.style.cssText = "color:#a6adc8;margin-left:4px;"; statusEl.textContent = "⏳";
  toolbar.appendChild(statusEl);

  function btn(label: string, onClick: () => void, bg = "#45475a") {
    const b = document.createElement("button");
    b.textContent = label;
    b.style.cssText = `background:${bg};color:#cdd6f4;border:none;border-radius:3px;padding:3px 10px;cursor:pointer;font-size:12px;font-family:monospace;white-space:nowrap;`;
    b.onclick = onClick; toolbar.appendChild(b); return b;
  }

  const connectBtn = btn("▶ Connect", () => doConnect(), "#1e66f5");
  const stopBtn    = btn("⏹ Stop",   () => invoke("log_stop", { logId }), "#e64553");
  const clearBtn   = btn("🗑 Clear",  () => { tbody.innerHTML = ""; }, "#45475a");

  // ── 表格 ──
  const tableWrap = document.createElement("div");
  tableWrap.style.cssText = "flex:1;overflow-y:auto;";
  const table = document.createElement("table");
  table.style.cssText = "width:100%;border-collapse:collapse;table-layout:fixed;";

  const thead = document.createElement("thead");
  thead.innerHTML = `<tr style="position:sticky;top:0;background:#181825;">
    <th style="width:8%;padding:4px;text-align:left;border-bottom:2px solid #313244;">Level</th>
    <th style="width:18%;padding:4px;text-align:left;border-bottom:2px solid #313244;">Time</th>
    <th style="width:20%;padding:4px;text-align:left;border-bottom:2px solid #313244;">File:Line</th>
    <th style="width:54%;padding:4px;text-align:left;border-bottom:2px solid #313244;">Message</th>
  </tr>`;
  table.appendChild(thead);

  const tbody = document.createElement("tbody");
  table.appendChild(tbody);
  tableWrap.appendChild(table);

  el.appendChild(toolbar);
  el.appendChild(tableWrap);

  // ── 连接逻辑 ──
  function doConnect() {
    statusEl.textContent = "Connecting..."; statusEl.style.color = "#f9e2af";
    invoke("log_connect", {
      logId, host: inputs["host"].value, port: parseInt(inputs["port"].value) || 22,
      username: inputs["user"].value, password: inputs["pass"].value,
      filePath: inputs["logfile"].value,
    });
  }

  // 监听状态
  listen("log-status", (event: any) => {
    if (event.payload?.logId !== logId) return;
    const s = event.payload;
    if (s.status === "connected") { statusEl.textContent = "● Live"; statusEl.style.color = "#a6e3a1"; }
    else if (s.status === "error") { statusEl.textContent = `✖ ${s.error || "error"}`; statusEl.style.color = "#f38ba8"; }
    else if (s.status === "stopped") { statusEl.textContent = "■ Stopped"; statusEl.style.color = "#a6adc8"; }
  });

  // 监听日志行
  listen("log-line", (event: any) => {
    if (event.payload?.logId !== logId) return;
    const line: string = event.payload.line;

    // 按空格切分，最多4列
    const parts = line.trim().split(/\s+/);
    // 尝试匹配日志格式: [Level] Timestamp [File:Line] Message
    const level   = parts[0] || "-";
    const time    = parts.slice(1, 4).join(" ") || "-";
    // 找 [xxx:xxx] 模式
    let fileLine = "-";
    let msgStart = 4;
    for (let i = 0; i < parts.length; i++) {
      if (parts[i].startsWith("[") && parts[i].includes(":")) {
        fileLine = parts[i].replace(/^\[|\]$/g, "");
        msgStart = i + 1;
        break;
      }
    }
    const message = parts.slice(msgStart).join(" ") || "-";

    // 颜色
    const levelColor: Record<string, string> = {
      "E": "#f38ba8", "W": "#f9e2af", "I": "#89b4fa", "D": "#a6adc8",
    };
    const lc = levelColor[level[0]?.toUpperCase()] || "#cdd6f4";

    const tr = document.createElement("tr");
    tr.style.cssText = "border-bottom:1px solid #313244;";
    tr.innerHTML = `
      <td style="padding:2px 4px;color:${lc};white-space:nowrap;overflow:hidden;text-overflow:ellipsis;">${level}</td>
      <td style="padding:2px 4px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;">${time}</td>
      <td style="padding:2px 4px;color:#89b4fa;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;">${fileLine}</td>
      <td style="padding:2px 4px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;">${message}</td>
    `;
    tbody.appendChild(tr);

    // 限制最多 10000 行
    while (tbody.children.length > 10000) tbody.removeChild(tbody.firstChild!);
    // 自动滚到底部
    tableWrap.scrollTop = tableWrap.scrollHeight;
  });

  container.on("destroy", () => {});
}

// ── 初始化 ─────────────────────────────────────

document.addEventListener("DOMContentLoaded", () => {
  const appEl = document.getElementById("app")!;
  const layout = new GoldenLayout(appEl);

  layout.registerComponentFactoryFunction("terminal", (c) => { createTerminalPane(c); });
  layout.registerComponentFactoryFunction("log-viewer", (c) => { createLogPane(c); });

  layout.loadLayout({
    root: {
      type: "component",
      componentType: "terminal",
      title: "Terminal",
    },
  });

  window.addEventListener("resize", () => {
    layout.setSize(window.innerWidth, window.innerHeight);
  });
  layout.setSize(window.innerWidth, window.innerHeight);

  invoke("get_config").then((cfg: any) => {
    if (cfg?.font?.size) {
      fontSize = cfg.font.size;
      termInstances.forEach(t => { t.options.fontSize = fontSize; });
    }
  });

  document.addEventListener("keydown", (e) => {
    if (e.ctrlKey && e.shiftKey) {
      if (e.key.toUpperCase() === "T") { e.preventDefault(); layout.addComponent("terminal", undefined, "Terminal"); }
      if (e.key.toUpperCase() === "L") { e.preventDefault(); layout.addComponent("log-viewer", undefined, "Log Viewer"); }
    }
    if (e.ctrlKey && (e.key === "=" || e.key === "+")) { e.preventDefault(); setFontSize(1); }
    if (e.ctrlKey && e.key === "-") { e.preventDefault(); setFontSize(-1); }
  });
});
