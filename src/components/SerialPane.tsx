/**
 * @file SerialPane.tsx
 * @brief Serial pane: port picker (scan list + details) around a raw byte stream
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-02
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 *
 * Two faces in one pane: while nothing is open the body is the scan list; once a port
 * is open the body is the stream, and the picker moves behind the device selector in
 * the head. Everything here is protocol-agnostic — raw bytes in, raw bytes out, plus
 * the counters (bytes, lines, frame rate) that can be derived without knowing what the
 * bytes mean.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { log } from "../log";

/** One scanned port, mirroring modules/serial.rs. */
interface PortInfo {
  device: string;
  driver: string;
  chip: string;
  vid_pid: string;
  serial: string;
  usb_path: string;
  busy_pid: number | null;
}

interface DataPayload {
  id: string;
  data: number[];
}

interface ExitPayload {
  id: string;
  reason: string;
}

/** One received line: when it landed, and the raw bytes (so HEX view can show them). */
interface StreamLine {
  at: number;
  bytes: Uint8Array;
}

/**
 * A frame ends when the line goes quiet for this long. The port has no frames of its
 * own, so the idle gap is the only protocol-free way to count them.
 */
const IDLE_GAP_MS = 2;
/** Received lines kept in memory; the oldest are dropped. */
const MAX_LINES = 2000;
/** Lines actually rendered (the tail); keeps a long stream from costing the DOM. */
const RENDER_LINES = 400;
/** Rates the picker offers; the backend accepts exactly these. */
const BAUD_CHOICES = [9600, 115200, 921600];

const decoder = new TextDecoder("utf-8", { fatal: false });

const pad = (n: number, width = 2) => String(n).padStart(width, "0");

function stampOf(at: number): string {
  const d = new Date(at);
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(d.getMilliseconds(), 3)}`;
}

function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

/** Toggle chip styling, shared by the view switch and the send-line switches. */
function chip(on: boolean): string {
  return `rounded-sm border px-1.5 py-px ${
    on ? "border-green bg-green/10 text-green" : "border-surface0 text-subtext0"
  }`;
}

export interface SerialPaneProps {
  /** Pane id, also used as the serial session id */
  paneId: string;
  /** Active pane: blue head */
  focused?: boolean;
  /** Reporting upwards: the page shows this in the top bar */
  onLabel?: (paneId: string, label: string) => void;
}

function SerialPane({ paneId, focused = false, onLabel }: SerialPaneProps) {
  const [ports, setPorts] = useState<PortInfo[]>([]);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [baud, setBaud] = useState(115200);
  const [open, setOpen] = useState<{ device: string; baud: number } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const [lines, setLines] = useState<StreamLine[]>([]);
  const [rx, setRx] = useState(0);
  const [tx, setTx] = useState(0);
  const [frames, setFrames] = useState(0);
  const [clock, setClock] = useState(Date.now());
  const [hexView, setHexView] = useState(false);

  const [input, setInput] = useState("");
  const [hexSend, setHexSend] = useState(false);
  const [crlf, setCrlf] = useState(true);

  const pending = useRef<Uint8Array>(new Uint8Array(0));
  const lastChunkAt = useRef(0);
  const startedAt = useRef(0);
  const stick = useRef(true);
  const scrollRef = useRef<HTMLDivElement | null>(null);

  /** Scan the machine's ports. Reads /sys and /proc only — never opens anything. */
  const scan = useCallback(async () => {
    try {
      const list = await invoke<PortInfo[]>("serial_list");
      setPorts(list);
      setExpanded(current => (current && list.some(p => p.device === current) ? current : null));
      setError(null);
    } catch (e) {
      log.error("serial", `scan failed: ${String(e)}`);
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void scan();
  }, [scan]);

  useEffect(() => {
    onLabel?.(paneId, "串口");
  }, [paneId, onLabel]);

  /** Split incoming bytes into lines; a partial line waits for its newline. */
  const append = useCallback((chunk: Uint8Array) => {
    const buffered = pending.current;
    const all = new Uint8Array(buffered.length + chunk.length);
    all.set(buffered);
    all.set(chunk, buffered.length);

    const at = Date.now();
    const fresh: StreamLine[] = [];
    let start = 0;
    for (let i = 0; i < all.length; i += 1) {
      if (all[i] !== 0x0a) continue;
      let end = i;
      if (end > start && all[end - 1] === 0x0d) end -= 1;
      fresh.push({ at, bytes: all.slice(start, end) });
      start = i + 1;
    }
    pending.current = all.slice(start);
    if (fresh.length) {
      setLines(list => {
        const next = [...list, ...fresh];
        return next.length > MAX_LINES ? next.slice(-MAX_LINES) : next;
      });
    }
  }, []);

  useEffect(() => {
    const offs: Array<() => void> = [];
    let live = true;
    void (async () => {
      offs.push(
        await listen<DataPayload>("serial-data", event => {
          if (event.payload.id !== paneId || event.payload.data.length === 0) return;
          const bytes = Uint8Array.from(event.payload.data);
          const now = performance.now();
          if (now - lastChunkAt.current > IDLE_GAP_MS) setFrames(f => f + 1);
          lastChunkAt.current = now;
          setRx(v => v + bytes.length);
          append(bytes);
        }),
      );
      offs.push(
        await listen<ExitPayload>("serial-exit", event => {
          if (event.payload.id !== paneId) return;
          setOpen(null);
          setError(event.payload.reason || "串口已断开");
          onLabel?.(paneId, "串口");
        }),
      );
      if (!live) offs.forEach(off => off());
    })();
    return () => {
      live = false;
      offs.forEach(off => off());
    };
  }, [paneId, append, onLabel]);

  /** The frame rate only means anything next to a clock that moves. */
  useEffect(() => {
    if (!open) return;
    const timer = window.setInterval(() => setClock(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [open]);

  // Keep the tail in view, but only while the user has not scrolled away.
  useEffect(() => {
    const box = scrollRef.current;
    if (box && stick.current) box.scrollTop = box.scrollHeight;
  }, [lines, hexView]);

  const connect = useCallback(
    async (device: string) => {
      setError(null);
      try {
        await invoke("serial_open", { id: paneId, device, baud });
        startedAt.current = Date.now();
        lastChunkAt.current = 0;
        pending.current = new Uint8Array(0);
        setLines([]);
        setRx(0);
        setTx(0);
        setFrames(0);
        setOpen({ device, baud });
        onLabel?.(paneId, `${device} · ${baud}`);
      } catch (e) {
        log.error("serial", `open ${device} failed: ${String(e)}`);
        setError(String(e));
      }
    },
    [paneId, baud, onLabel],
  );

  const disconnect = useCallback(async () => {
    await invoke("serial_close", { id: paneId }).catch(() => {});
    setOpen(null);
    setError(null);
    onLabel?.(paneId, "串口");
  }, [paneId, onLabel]);

  useEffect(
    () => () => {
      void invoke("serial_close", { id: paneId }).catch(() => {});
    },
    [paneId],
  );

  const send = useCallback(async () => {
    if (!open || !input) return;
    let bytes: Uint8Array;
    if (hexSend) {
      const digits = input.replace(/[^0-9a-fA-F]/g, "");
      if (digits.length === 0 || digits.length % 2 !== 0) {
        setError("HEX 发送要偶数个十六进制字符");
        return;
      }
      bytes = new Uint8Array(digits.length / 2);
      for (let i = 0; i < bytes.length; i += 1) {
        bytes[i] = parseInt(digits.slice(i * 2, i * 2 + 2), 16);
      }
    } else {
      bytes = new TextEncoder().encode(crlf ? `${input}\r\n` : input);
    }
    try {
      await invoke("serial_write", { id: paneId, data: Array.from(bytes) });
      setTx(v => v + bytes.length);
      setInput("");
      setError(null);
    } catch (e) {
      log.error("serial", `write failed: ${String(e)}`);
      setError(String(e));
    }
  }, [open, input, hexSend, crlf, paneId]);

  const elapsed = open ? Math.max(1, (clock - startedAt.current) / 1000) : 0;
  const shown = lines.length > RENDER_LINES ? lines.slice(-RENDER_LINES) : lines;

  const header = (
    <div
      className={`flex h-[26px] flex-none items-center gap-2 border-b border-surface0 px-2 font-mono text-[11.5px] ${
        focused ? "bg-blue text-base" : "bg-mantle text-subtext0"
      }`}
    >
      <span
        className={`flex-none rounded-sm px-[5px] py-px font-mono text-[10px] tracking-wide ${
          focused ? "bg-base/25 text-base" : "bg-green/15 text-green"
        }`}
      >
        SER
      </span>
      {open ? <span className="flex-none">{open.device}</span> : null}
      {open ? <span className="flex-none opacity-75">{open.baud} · 8N1 · 无流控</span> : null}
      <span className="flex-1" />
      {error ? <span className="flex-none text-[10.5px] text-red">{error}</span> : null}
      {open ? null : <span className="flex-none opacity-75">{ports.length} 个串口</span>}
      {/* Not connected: the head carries the scan action, not a status line. */}
      {open ? (
        <button
          type="button"
          onClick={() => void disconnect()}
          className="flex-none rounded-sm border border-surface0 px-2.5 py-0.5 text-[10.5px] text-subtext0 hover:border-subtext0 hover:text-fg"
        >
          断开
        </button>
      ) : (
        <button
          type="button"
          onClick={() => void scan()}
          className="flex-none rounded-sm border border-surface0 px-2.5 py-0.5 text-[10.5px] text-subtext0 hover:border-subtext0 hover:text-fg"
        >
          重新扫描
        </button>
      )}
    </div>
  );

  const picker = (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto p-1.5 font-mono text-[11.5px]">
        {ports.length === 0 ? <div className="px-2 py-3 text-overlay0">没有扫描到串口</div> : null}
        {ports.map(port => {
          const isExpanded = expanded === port.device;
          const busy = port.busy_pid != null;
          return (
            <div
              key={port.device}
              className={`rounded-sm border ${isExpanded ? "border-surface0 bg-crust/60" : "border-transparent"}`}
            >
              <button
                type="button"
                className="flex w-full items-center gap-2.5 rounded-sm px-2 py-1.5 text-left hover:bg-surface0/40"
                onClick={() => setExpanded(value => (value === port.device ? null : port.device))}
              >
                <span className="w-2 text-[9px] text-overlay0">{isExpanded ? "▾" : "▸"}</span>
                <span className={isExpanded ? "text-green" : "text-fg"}>{port.device}</span>
                {port.chip ? (
                  <span className="rounded-sm bg-sky/15 px-1.5 py-px text-[10px] text-sky">{port.chip}</span>
                ) : null}
                <span className="flex-1" />
                {busy ? <span className="text-[10.5px] text-yellow">占用中 · pid {port.busy_pid}</span> : null}
                <span className="text-[10.5px] text-overlay0">{port.vid_pid}</span>
              </button>

              {isExpanded ? (
                <div className="px-2 pb-2 pl-7 text-[10.5px]">
                  <div className="flex gap-5 text-overlay0">
                    <span>
                      驱动 <span className="text-subtext1">{port.driver || "—"}</span>
                    </span>
                    <span>
                      序列号 <span className="text-subtext1">{port.serial || "—"}</span>
                    </span>
                    <span>
                      USB 位置 <span className="text-subtext1">{port.usb_path || "—"}</span>
                    </span>
                  </div>
                  <div className="my-1.5 h-px bg-surface0" />
                  <div className="flex flex-wrap items-center gap-1.5">
                    <span className="text-overlay0">连接参数</span>
                    <span className="text-overlay0">波特率</span>
                    {BAUD_CHOICES.map(value => (
                      <button
                        key={value}
                        type="button"
                        onClick={() => setBaud(value)}
                        className={`rounded-sm border px-1.5 py-px ${
                          baud === value ? "border-green bg-green/10 text-green" : "border-surface0 text-subtext0"
                        }`}
                      >
                        {value}
                      </button>
                    ))}
                    <span className="text-overlay0">8N1 · 无流控</span>
                    <span className="flex-1" />
                    <button
                      type="button"
                      disabled={busy}
                      onClick={() => void connect(port.device)}
                      className={`rounded-sm border px-3 py-0.5 ${
                        busy ? "border-surface0 text-overlay0" : "border-green bg-green/10 text-green"
                      }`}
                    >
                      {busy ? "被占用" : "连接"}
                    </button>
                  </div>
                </div>
              ) : null}
            </div>
          );
        })}
      </div>
    </div>
  );

  const stream = (
    <>
      <div className="flex h-[22px] flex-none items-center gap-3 border-b border-surface0 px-2.5 font-mono text-[10.5px] text-overlay0">
        <span>
          RX <span className="text-subtext1">{fmtBytes(rx)}</span>
        </span>
        <span>
          TX <span className="text-subtext1">{fmtBytes(tx)}</span>
        </span>
        <span>
          行 <span className="text-subtext1">{lines.length}</span>
        </span>
        <span className="flex-1" />
        <span>
          帧率 <span className="text-subtext1">{elapsed ? `${(frames / elapsed).toFixed(1)}/s` : "—"}</span> · 空闲{" "}
          {IDLE_GAP_MS}ms 切帧
        </span>
        <span className="flex gap-1">
          <button type="button" onClick={() => setHexView(false)} className={chip(!hexView)}>
            文本
          </button>
          <button type="button" onClick={() => setHexView(true)} className={chip(hexView)}>
            HEX
          </button>
        </span>
      </div>

      <div
        ref={scrollRef}
        onScroll={event => {
          const box = event.currentTarget;
          stick.current = box.scrollHeight - box.scrollTop - box.clientHeight < 24;
        }}
        className="min-h-0 flex-1 overflow-y-auto px-2.5 py-1.5 font-mono text-[11.5px] leading-[19px] text-subtext1"
      >
        {lines.length > RENDER_LINES ? (
          <div className="text-overlay0">… 已滚过 {lines.length - RENDER_LINES} 行</div>
        ) : null}
        {shown.map((line, index) => (
          <div key={`${line.at}-${index}`} className="whitespace-pre-wrap break-all">
            <span className="text-overlay0">{stampOf(line.at)} </span>
            {hexView
              ? Array.from(line.bytes)
                  .map(byte => byte.toString(16).padStart(2, "0"))
                  .join(" ")
              : decoder.decode(line.bytes)}
          </div>
        ))}
      </div>

      <div className="flex h-[26px] flex-none items-center gap-2 border-t border-surface0 bg-crust px-2.5 font-mono text-[11px]">
        <span className="text-green">&gt;</span>
        <input
          className="min-w-0 flex-1 bg-transparent text-fg outline-none"
          placeholder="输入后回车发送"
          value={input}
          onChange={event => setInput(event.target.value)}
          onKeyDown={event => {
            if (event.key !== "Enter") return;
            event.preventDefault();
            void send();
          }}
        />
        <button type="button" onClick={() => setHexSend(value => !value)} className={chip(hexSend)}>
          HEX 发送
        </button>
        <button type="button" onClick={() => setCrlf(value => !value)} className={chip(crlf)}>
          CR LF
        </button>
        <span className="text-overlay0">Enter 发送</span>
      </div>
    </>
  );

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden rounded-md border border-surface0 bg-base">
      {header}
      {open ? stream : picker}
    </div>
  );
}

export default SerialPane;
