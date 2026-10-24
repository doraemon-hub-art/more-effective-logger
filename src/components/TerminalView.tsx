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
import { invoke, listen, readText, writeText } from "../ipc";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { terminalTheme, type ThemeName } from "../theme";
import { log } from "../log";
import "@xterm/xterm/css/xterm.css";

/**
 * The screen's font stack: the family picked in the settings, with the CJK fallback and
 * a generic behind it. The interface reads the same string through the `--font-mono`
 * token, so one setting moves both.
 */
export function fontStack(family: string): string {
  return `"${family}", "Noto Sans Mono CJK SC", monospace`;
}

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
  /** Monospace family this terminal renders in; the settings page picks it. */
  fontFamily?: string;
  /** Catppuccin flavor of the screen; the settings page picks it. */
  theme?: ThemeName;
  /** Directory the shell starts in; set for a pane restored from the last session. */
  cwd?: string;
  /** Box classes; defaults to filling the parent. */
  className?: string;
}

function TerminalView({
  id,
  onStatus,
  onExit,
  focused,
  fontSize = 12.5,
  fontFamily = "JetBrains Mono",
  theme = "mocha",
  cwd,
  className,
}: TerminalViewProps) {
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
      fontFamily: fontStack(fontFamily),
      fontSize,
      lineHeight: 1.2,
      cursorBlink: true,
      scrollback: 5000,
      theme: terminalTheme(theme),
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

    // WebKitGTK IME workaround (xtermjs #6078 / #6045): xterm only clears its hidden
    // textarea on Enter/Ctrl+C, so typed text accumulates there, and WebKitGTK's
    // insertText (which arrives after keyup) plus the keyCode=229 textarea diff keep
    // re-emitting the whole accumulated value — every later keystroke repeats more
    // of what was typed before. Two halves, mirroring the mitigation the issue
    // author runs downstream:
    // 1. clear the textarea after every settled keystroke (not during composition,
    //    and not under screenReaderMode — screen readers read the accumulated value);
    // 2. swallow the insertText that merely re-delivers text already sent through the
    //    composition path, so the committed string reaches the shell exactly once.
    let lastCommitted = "";
    let commitTimer = 0;
    const onCompositionEnd = (event: CompositionEvent) => {
      lastCommitted = event.data;
      window.clearTimeout(commitTimer);
      // The duplicate insertText lands after keyup; past that window the textarea
      // is swept so nothing accumulates for the next 229-triggered re-emit.
      commitTimer = window.setTimeout(() => {
        lastCommitted = "";
        const textarea = term.textarea;
        if (textarea && !term.options.screenReaderMode) textarea.value = "";
      }, 120);
    };
    const onInput = (event: Event) => {
      const input = event as InputEvent;
      if (input.inputType === "insertText" && lastCommitted && input.data === lastCommitted) {
        input.stopImmediatePropagation();
        input.preventDefault();
      }
    };
    host.addEventListener("compositionend", onCompositionEnd, true);
    host.addEventListener("input", onInput, true);

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

    // Copy / paste through the system clipboard. The view owns the selection and the
    // input path, so the shortcuts are claimed here rather than in the page's key
    // handler; Ctrl+Shift+<letter> reaches xterm and nothing else uses it, while plain
    // Ctrl+C stays the shell's interrupt. Returning false keeps xterm from turning the
    // combination into bytes. The webview cannot reach the clipboard on its own, so the
    // Rust side does it (tauri-plugin-clipboard-manager).
    term.attachCustomKeyEventHandler(event => {
      // xterm runs this for keydown, keypress and keyup: one keystroke, one action.
      if (event.type !== "keydown") return true;
      if (!event.ctrlKey || !event.shiftKey || event.altKey || event.metaKey) return true;
      const key = event.key.toLowerCase();
      if (key === "c") {
        // An empty selection would wipe what the clipboard already holds.
        const selection = term.getSelection();
        if (selection) {
          void writeText(selection).catch(error => log.error("term", `clipboard write failed: ${String(error)}`));
        }
        return false;
      }
      if (key === "v") {
        // term.paste, not pty_input: it normalizes line endings and brackets the text
        // when the shell asked for it, so a multi-line paste lands in vim in one piece.
        void readText()
          .then(text => {
            if (text) term.paste(text);
          })
          .catch(error => log.error("term", `clipboard read failed: ${String(error)}`));
        return false;
      }
      return true;
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
          cwd,
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
        log.error("pty", `spawn_terminal failed: ${String(error)}`);
        report({ running: false, failure: String(error) });
        term.write(`\r\n\x1b[31m${String(error)}\x1b[0m\r\n`);
      }
    })();

    return () => {
      disposed = true;
      window.clearInterval(cwdTimer);
      window.clearTimeout(resizeTimer);
      window.clearTimeout(commitTimer);
      host.removeEventListener("compositionend", onCompositionEnd, true);
      host.removeEventListener("input", onInput, true);
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
  // geometry, which a pure font change does not touch, so the refit happens here.
  // The shell learns the new cols/rows through a direct pty_resize: the observer sees
  // no box change and would stay silent.
  useEffect(() => {
    const term = termRef.current;
    if (!term) return;
    term.options.fontSize = fontSize;
    term.options.fontFamily = fontStack(fontFamily);
    fitRef.current?.fit();
    if (hostRef.current?.clientWidth) {
      const sessionId = sessionIdRef.current;
      if (sessionId) {
        void invoke("pty_resize", { id: sessionId, cols: term.cols, rows: term.rows }).catch(() => {});
      }
    }
  }, [fontSize, fontFamily]);

  // Flavor change -> re-theme the screen in place. xterm repaints the whole view when
  // its theme option changes, and the cell size is untouched, so no refit is needed.
  useEffect(() => {
    const term = termRef.current;
    if (term) term.options.theme = terminalTheme(theme);
  }, [theme]);

  return <div ref={hostRef} className={className ?? "h-full w-full"} />;
}

export default TerminalView;
