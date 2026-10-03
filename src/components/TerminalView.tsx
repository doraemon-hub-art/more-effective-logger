/**
 * @file TerminalView.tsx
 * @brief Minimal xterm.js terminal widget: one shell behind one xterm view
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-01
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 *
 * Wiring: xterm.js renders and collects keystrokes, the Rust side owns the pty.
 *   keys   -> term.onData      -> pty_input
 *   output -> pty-output event -> term.write
 *   size   -> ResizeObserver   -> pty_resize (once the box settles)
 *   spawn  -> spawn_terminal on mount, pty_kill on unmount
 *
 * The widget deliberately knows nothing about the pane chrome: the parent gives
 * it a box (it fills it), and it reports what it learned about the shell through
 * `onStatus`, so a pane shell can be built around it later without touching this
 * file. The id is a prop so several terminals can live side by side.
 */
import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";

/** Catppuccin Mocha, so the shell matches the rest of the UI. */
const TERM_THEME = {
  background: "#1e1e2e",
  foreground: "#cdd6f4",
  cursor: "#cdd6f4",
  cursorAccent: "#1e1e2e",
  selectionBackground: "#45475a",
  black: "#45475a",
  red: "#f38ba8",
  green: "#a6e3a1",
  yellow: "#f9e2af",
  blue: "#89b4fa",
  magenta: "#cba6f7",
  cyan: "#89dceb",
  white: "#bac2de",
  brightBlack: "#585b70",
  brightRed: "#f38ba8",
  brightGreen: "#a6e3a1",
  brightYellow: "#f9e2af",
  brightBlue: "#89b4fa",
  brightMagenta: "#cba6f7",
  brightCyan: "#89dceb",
  brightWhite: "#a6adc8",
};

/**
 * How long the box has to hold still before the shell is told about a new size.
 * Dragging a divider resizes the box every frame; the screen follows at once, but
 * the shell only needs the size it ends up with (a resize costs a SIGWINCH redraw).
 */
const RESIZE_SETTLE_MS = 60;

/** Everything the widget knows about its shell. */
export interface TerminalStatus {
  id: string;
  pid: number | null;
  user: string;
  host: string;
  /** Where the shell currently is, shortened to `~`; null until known */
  cwd: string | null;
  cols: number;
  rows: number;
  /** True between a successful spawn and the shell exiting. */
  running: boolean;
  /** Set when spawning failed; the shell never started. */
  failure: string | null;
}

/** What `spawn_terminal` reports back. */
interface SessionInfo {
  id: string;
  pid: number | null;
  cols: number;
  rows: number;
  user: string;
  host: string;
}

interface OutputPayload {
  id: string;
  data: string;
}

interface ExitPayload {
  id: string;
}

export interface TerminalViewProps {
  /** Session id; a random one is used when omitted. */
  id?: string;
  /** Called whenever something about the shell changes. */
  onStatus?: (status: TerminalStatus) => void;
  /** Called when the shell exited on its own. */
  onExit?: (id: string) => void;
  /**
   * Active pane: takes keyboard focus. Moving pane focus with the keyboard has to
   * move the keystrokes too, otherwise you keep typing into the previous pane.
   */
  focused?: boolean;
  /** Font size of this terminal's screen, in px (base from settings × pane zoom). */
  fontSize?: number;
  /** Box classes; defaults to filling the parent. */
  className?: string;
}

function TerminalView({ id, onStatus, onExit, focused, fontSize = 12.5, className }: TerminalViewProps) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const termRef = useRef<Terminal | null>(null);
  // Survives between effects: the font-size effect refits through the same addon
  // instance the mount effect created, without reaching into xterm's internals.
  const fitRef = useRef<FitAddon | null>(null);
  // The session id lives inside the mount effect; zoom changes need it to tell the
  // shell about the new size, so the latest one is mirrored here on each report.
  const sessionIdRef = useRef<string | null>(null);
  // Kept in refs so a re-render with fresh closures never tears the pty down.
  const statusRef = useRef(onStatus);
  const exitRef = useRef(onExit);
  statusRef.current = onStatus;
  exitRef.current = onExit;

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    // The id prop names the pane; the session id adds a suffix per mount, so a
    // remount (React runs effects twice in development) never collides with the
    // session that is still being torn down.
    const sessionId = `${id ?? "term"}-${Math.random().toString(36).slice(2, 8)}`;
    sessionIdRef.current = sessionId;
    let status: TerminalStatus = {
      id: sessionId,
      pid: null,
      user: "",
      host: "",
      cwd: null,
      cols: 0,
      rows: 0,
      running: false,
      failure: null,
    };
    const report = (patch: Partial<TerminalStatus>) => {
      status = { ...status, ...patch };
      statusRef.current?.(status);
    };

    const term = new Terminal({
      fontFamily: '"JetBrains Mono", "Fira Code", "DejaVu Sans Mono", monospace',
      fontSize,
      lineHeight: 1.2,
      cursorBlink: true,
      scrollback: 5000,
      theme: TERM_THEME,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    fitRef.current = fit;
    term.open(host);
    termRef.current = term;

    const listeners: Array<() => void> = [];
    let disposed = false;
    let live = false;
    let resizeTimer = 0;

    // Shell output -> screen.
    void (async () => {
      listeners.push(
        await listen<OutputPayload>("pty-output", event => {
          if (event.payload.id === sessionId) term.write(event.payload.data);
        }),
      );
      listeners.push(
        await listen<ExitPayload>("pty-exit", event => {
          if (event.payload.id !== sessionId) return;
          term.write("\r\n\x1b[38;5;245m[shell exited]\x1b[0m\r\n");
          report({ running: false });
          exitRef.current?.(sessionId);
        }),
      );
    })();

    // Keystrokes -> shell.
    term.onData(data => {
      void invoke("pty_input", { id: sessionId, data });
    });

    // Pane size -> pty size (the shell gets SIGWINCH and redraws).
    const observer = new ResizeObserver(() => {
      // A hidden page has no size: leave that shell alone until it is shown again.
      if (host.clientWidth === 0 || host.clientHeight === 0) return;
      fit.fit();
      report({ cols: term.cols, rows: term.rows });
      // The screen is already the new size; the shell is told once the box settles.
      if (!live) return;
      window.clearTimeout(resizeTimer);
      resizeTimer = window.setTimeout(() => {
        void invoke("pty_resize", { id: sessionId, cols: term.cols, rows: term.rows });
      }, RESIZE_SETTLE_MS);
    });
    observer.observe(host);

    // The pane mirror in the top bar shows where the shell is. `cd` happens inside
    // the shell, so ask the backend (which reads /proc/<pid>/cwd) now and then
    // instead of trying to track it here.
    const cwdTimer = window.setInterval(() => {
      if (!live) return;
      void invoke<string | null>("terminal_cwd", { id: sessionId })
        .then(cwd => {
          if (cwd) report({ cwd });
        })
        .catch(() => {});
    }, 2000);

    // Spawn the shell once the box has real dimensions.
    void (async () => {
      fit.fit();
      try {
        const spawned = await invoke<SessionInfo>("spawn_terminal", {
          id: sessionId,
          cols: Math.max(term.cols, 20),
          rows: Math.max(term.rows, 5),
        });
        if (disposed) {
          void invoke("pty_kill", { id: sessionId });
          return;
        }
        live = true;
        report({
          pid: spawned.pid,
          user: spawned.user,
          host: spawned.host,
          cols: spawned.cols,
          rows: spawned.rows,
          running: true,
          failure: null,
        });
        term.focus();
        void invoke("pty_resize", { id: sessionId, cols: term.cols, rows: term.rows }).catch(() => {});
      } catch (error) {
        report({ running: false, failure: String(error) });
        term.write(`\r\n\x1b[31m${String(error)}\x1b[0m\r\n`);
      }
    })();

    return () => {
      disposed = true;
      window.clearInterval(cwdTimer);
      window.clearTimeout(resizeTimer);
      observer.disconnect();
      listeners.forEach(off => off());
      void invoke("pty_kill", { id: sessionId });
      term.dispose();
      termRef.current = null;
      fitRef.current = null;
      sessionIdRef.current = null;
    };
  }, [id]);

  // Pane focus -> keyboard focus. Spawning focuses a fresh terminal on its own
  // (see above); this covers the moves that happen later.
  useEffect(() => {
    if (focused) termRef.current?.focus();
  }, [focused]);

  // Zoom / settings change -> a new screen size. The fit observer only fires on box
  // geometry, which a pure font-size change does not touch, so the refit happens here.
  // The shell learns the new cols/rows through a direct pty_resize: the observer sees
  // no box change and would stay silent.
  useEffect(() => {
    const term = termRef.current;
    if (!term) return;
    term.options.fontSize = fontSize;
    fitRef.current?.fit();
    if (hostRef.current?.clientWidth) {
      const sessionId = sessionIdRef.current;
      if (sessionId) {
        void invoke("pty_resize", { id: sessionId, cols: term.cols, rows: term.rows }).catch(() => {});
      }
    }
  }, [fontSize]);

  return <div ref={hostRef} className={className ?? "h-full w-full"} />;
}

export default TerminalView;
