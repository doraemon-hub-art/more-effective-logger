/**
 * @file ipc.ts
 * @brief The two directions the frontend uses to reach the backend, for either client
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-23
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 *
 * In the Tauri window these calls go straight to the webview's own IPC. A plain browser has
 * no such channel, so the same calls go over the development bridge instead: `POST /rpc` for
 * a command, `GET /events` for the event stream. Both paths take and return the same shapes,
 * so the callers do not know which one is live.
 */
import { invoke as windowInvoke, isTauri } from "@tauri-apps/api/core";
import { listen as windowListen, type UnlistenFn } from "@tauri-apps/api/event";
import { readText as windowReadText, writeText as windowWriteText } from "@tauri-apps/plugin-clipboard-manager";

/** The bridge a dev build serves on loopback; its port is forwarded like the dev server's. */
const BRIDGE = "http://127.0.0.1:1421";

/** What a listener receives, shaped like the window client's event. */
export interface Payload<T> {
  payload: T;
}

export { isTauri };
export type { UnlistenFn };

/** Bridge calls leave one after another; the window client needs no such queue. */
let queue: Promise<unknown> = Promise.resolve();

/** Run one command in the backend. */
export async function invoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  if (isTauri()) return windowInvoke<T>(cmd, args);
  // One at a time: separate requests have no order of their own, and a keystroke that
  // overtakes the one before it reaches the shell as garbled input.
  const call = queue.then(() => send<T>(cmd, args));
  queue = call.then(
    () => undefined,
    () => undefined,
  );
  return call;
}

async function send<T>(cmd: string, args: Record<string, unknown>): Promise<T> {
  const response = await fetch(`${BRIDGE}/rpc`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ cmd, args }),
  });
  const body = await response.json();
  if (!body.ok) throw body.error;
  return body.value as T;
}

/** Listeners of the bridge, by event name; the stream itself is opened once. */
const handlers = new Map<string, Set<(event: Payload<unknown>) => void>>();
const attached = new Set<string>();
let stream: EventSource | null = null;

function attach(name: string): void {
  if (!stream || attached.has(name)) return;
  attached.add(name);
  stream.addEventListener(name, event => {
    const payload = JSON.parse((event as MessageEvent<string>).data);
    for (const handler of handlers.get(name) ?? []) handler({ payload });
  });
}

/** Subscribe to one event; the returned function unsubscribes. */
export async function listen<T>(name: string, handler: (event: Payload<T>) => void): Promise<UnlistenFn> {
  if (isTauri()) return windowListen<T>(name, handler);

  const entry = handler as (event: Payload<unknown>) => void;
  const set = handlers.get(name) ?? new Set<(event: Payload<unknown>) => void>();
  handlers.set(name, set);
  set.add(entry);
  stream ??= new EventSource(`${BRIDGE}/events`);
  attach(name);
  return () => {
    set.delete(entry);
    if (set.size === 0) handlers.delete(name);
  };
}

/** The clipboard, which a page reaches through the browser instead of the plugin. */
export async function readText(): Promise<string> {
  return isTauri() ? windowReadText() : navigator.clipboard.readText();
}

export async function writeText(text: string): Promise<void> {
  return isTauri() ? windowWriteText(text) : navigator.clipboard.writeText(text);
}
