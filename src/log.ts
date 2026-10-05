/**
 * @file log.ts
 * @brief Frontend side of the runtime log: console output and uncaught errors reach the file
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-05
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 *
 * A devtools console is of no use once the app is installed, so what the frontend logs goes
 * to the same file the backend writes. Lines are collected for a moment and handed over in
 * one piece: an IPC call per console line would cost more than the line is worth. Importing
 * this module is what turns the hook on.
 */
import { invoke } from "@tauri-apps/api/core";

type Level = "debug" | "info" | "warn" | "error";

/** One line, in the shape the backend expects. */
interface Line {
  level: Level;
  target: string;
  message: string;
}

/** How long a line waits for company; short, so a crash loses at most this much. */
const FLUSH_MS = 100;

let queue: Line[] = [];
let timer: ReturnType<typeof setTimeout> | null = null;

/**
 * Hand the collected lines to the backend, which stamps them with the time and writes them
 * next to its own. Failing to log must never break the app, and never log about itself.
 */
async function flush(): Promise<void> {
  if (timer !== null) {
    clearTimeout(timer);
    timer = null;
  }
  if (queue.length === 0) return;
  const lines = queue;
  queue = [];
  try {
    await invoke("log_write", { lines });
  } catch {
    // A browser (`npm run dev` on its own) has nowhere to send them, which is not worth
    // complaining about line by line.
  }
}

function enqueue(level: Level, target: string, message: string): void {
  queue.push({ level, target, message });
  if (timer === null) timer = setTimeout(() => void flush(), FLUSH_MS);
}

/** One console argument as text; objects only as far as they can be shown. */
function render(value: unknown): string {
  if (typeof value === "string") return value;
  if (value instanceof Error) return `${value.name}: ${value.message}`;
  try {
    return JSON.stringify(value) ?? String(value);
  } catch {
    return String(value);
  }
}

/** The log, from the app's own code. */
export const log = {
  debug: (target: string, message: string) => enqueue("debug", target, message),
  info: (target: string, message: string) => enqueue("info", target, message),
  warn: (target: string, message: string) => enqueue("warn", target, message),
  error: (target: string, message: string) => enqueue("error", target, message),
  /** Send what is still queued — for the way out, where the window goes in a moment. */
  flush,
};

/** The console as it was, so everything still shows up in the devtools console too. */
const raw = {
  log: console.log.bind(console),
  info: console.info.bind(console),
  warn: console.warn.bind(console),
  error: console.error.bind(console),
  debug: console.debug.bind(console),
};

console.log = (...args: unknown[]): void => {
  raw.log(...args);
  enqueue("info", "web", args.map(render).join(" "));
};

console.info = (...args: unknown[]): void => {
  raw.info(...args);
  enqueue("info", "web", args.map(render).join(" "));
};

console.warn = (...args: unknown[]): void => {
  raw.warn(...args);
  enqueue("warn", "web", args.map(render).join(" "));
};

console.error = (...args: unknown[]): void => {
  raw.error(...args);
  enqueue("error", "web", args.map(render).join(" "));
};

console.debug = (...args: unknown[]): void => {
  raw.debug(...args);
  enqueue("debug", "web", args.map(render).join(" "));
};

/** Failures that never passed through the console. */
window.addEventListener("error", event => {
  const place = event.filename ? ` (${event.filename}:${event.lineno})` : "";
  enqueue("error", "web", `uncaught ${event.message}${place}`);
});

window.addEventListener("unhandledrejection", event => {
  enqueue("error", "web", `unhandled rejection: ${render(event.reason)}`);
});

enqueue("info", "web", "webview started");
