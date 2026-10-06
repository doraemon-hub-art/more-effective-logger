/**
 * @file LogPane.tsx
 * @brief Log pane: one pane, two ways in — a local file read once, or a remote file followed
 *        over SSH — over one search box and one line area
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-06
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 *
 * What differs between the two ways is only how the log gets here: a path typed into a
 * field that completes as you go (and reads the file the moment the path names one), or
 * host / user / password / port / file with a connect and a disconnect button. Everything
 * below that — the filter bar, the line area, clearing, following — is the same piece for
 * both, which is why the two ways live in one component instead of two.
 *
 * The bar under the form filters: only the lines it matches are drawn. A local read holds
 * the whole file, so clearing the filter brings everything back; a remote follow keeps only
 * what passes while the filter is on, so what it dropped in the meantime is gone — the same
 * way it never showed what was written before the connection came up.
 *
 * On top of that each pane answers `/` with a vim-like search line along its bottom edge:
 * a pattern, then n / N to walk the hits. That one searches what is on screen, so the
 * filter decides which lines n and N can land on.
 *
 * Lines arrive already split from the backend, so a line still being written simply shows
 * up when it is finished. A long line is not wrapped: it scrolls sideways, which is how a
 * log line is worth reading.
 */
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { log } from "../log";

/** Which way the log gets in. */
type Way = "local" | "remote";

/** Where a remote follow stands. `idle` is a pane that has not been told to connect yet. */
type Link = "idle" | "connected" | "retrying" | "closed";

/** Lines handed over by the backend, already split. */
interface DataPayload {
  id: string;
  lines: string[];
}

interface StatePayload {
  id: string;
  connected: boolean;
  note: string;
}

interface ExitPayload {
  id: string;
  reason: string;
}

/** A local file as it was when it was opened. */
interface Snapshot {
  lines: string[];
  bytes: number;
}

/** One completion candidate under the typed prefix. */
interface Candidate {
  path: string;
  dir: boolean;
}

/** Lines drawn at the bottom of a long log; the rest stay in memory and in the search. */
const RENDER_LINES = 2000;
/** Candidates drawn under the path field: it is a dropdown, not a listing. */
const SHOWN_CANDIDATES = 8;
/** How close to the bottom still counts as "at the bottom", for the follow switch. */
const BOTTOM_GAP = 24;
/** Row pitch: the 19px line plus the hairline under it. Used to place the view on a hit. */
const ROW_PITCH = 20;

const FIELD_INPUT =
  "min-w-0 flex-1 rounded-sm border bg-base/60 px-[7px] py-[3px] font-mono text-[11.5px] text-subtext1 outline-none";

function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

/** Toggle chips: dim when off, green when on (the serial pane's switch, reused). */
function chip(on: boolean): string {
  return `flex-none rounded-sm border px-[7px] py-px font-mono text-[10.5px] ${
    on ? "border-green bg-green/10 text-green" : "border-surface0 text-subtext0"
  }`;
}

/** One labelled field of the remote form; the caller sizes it so the row lines up. */
function Field({ label, className, children }: { label: string; className: string; children: ReactNode }) {
  return (
    <label className={`flex items-center gap-[7px] ${className}`}>
      <span className="flex-none font-mono text-[10.5px] text-overlay0">{label}</span>
      {children}
    </label>
  );
}

export interface LogPaneProps {
  /** Pane id, also used as the session id of a remote follow */
  paneId: string;
  /** Active pane: blue head */
  focused?: boolean;
  /** Reporting upwards: the page shows this in the top bar */
  onLabel?: (paneId: string, label: string) => void;
}

function LogPane({ paneId, focused = false, onLabel }: LogPaneProps) {
  const [way, setWay] = useState<Way>("local");

  // Local side: the path being typed, what it could be, and what was last read.
  const [path, setPath] = useState("");
  const [candidates, setCandidates] = useState<Candidate[]>([]);
  const [suggesting, setSuggesting] = useState(false);
  /** Which candidate Tab / Enter would take. */
  const [pick, setPick] = useState(0);
  const [loaded, setLoaded] = useState<string | null>(null);

  // Remote side: what the form holds.
  const [host, setHost] = useState("");
  const [user, setUser] = useState("");
  const [password, setPassword] = useState("");
  const [port, setPort] = useState("22");
  const [remotePath, setRemotePath] = useState("");

  // What both ways share.
  const [lines, setLines] = useState<string[]>([]);
  /** Lines this pane has been given, before the filter takes any of them away. */
  const [seen, setSeen] = useState(0);
  const [bytes, setBytes] = useState(0);
  /** The filter: only the lines it matches are drawn. */
  const [filter, setFilter] = useState("");
  // Off by default: the pane stays where the reader put it until following is asked for.
  const [follow, setFollow] = useState(false);
  const [link, setLink] = useState<Link>("idle");
  const [note, setNote] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [rate, setRate] = useState(0);

  // The search line: open while it is being typed in, then a pattern with n / N on it.
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchDraft, setSearchDraft] = useState("");
  const [searchPattern, setSearchPattern] = useState("");
  /** Which of the lines on screen the search is sitting on. */
  const [currentHit, setCurrentHit] = useState<number | null>(null);

  /** Lines that arrived since the last rate sample. */
  const received = useRef(0);
  /** The filter in force right now, for listeners that outlive a re-render. */
  const filterRef = useRef<RegExp | null>(null);
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const searchRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    onLabel?.(
      paneId,
      way === "remote" ? sourceLabel(way, loaded, user, host, remotePath) || "日志" : (loaded ?? "日志"),
    );
  }, [paneId, onLabel, way, loaded, user, host, remotePath]);

  /**
   * The filter is a regex; one that does not compile counts as no filter at all and turns
   * the text red, so a half-typed pattern never blanks the pane or throws lines away.
   */
  const [filterRegex, filterBad] = useMemo<[RegExp | null, boolean]>(() => {
    if (!filter) return [null, false];
    try {
      return [new RegExp(filter), false];
    } catch {
      return [null, true];
    }
  }, [filter]);

  useEffect(() => {
    filterRef.current = filterRegex;
  }, [filterRegex]);

  /** What is drawn: everything held, cut down to what the filter matches. */
  const visible = useMemo(
    () => (filterRegex ? lines.filter(line => filterRegex.test(line)) : lines),
    [lines, filterRegex],
  );

  /** The search pattern, compiled the same way. */
  const [searchRegex, searchBad] = useMemo<[RegExp | null, boolean]>(() => {
    if (!searchPattern) return [null, false];
    try {
      return [new RegExp(searchPattern), false];
    } catch {
      return [null, true];
    }
  }, [searchPattern]);

  /** Which of the lines on screen the search lands on, in the order they are drawn. */
  const hitLines = useMemo(() => {
    if (!searchRegex) return [];
    const found: number[] = [];
    visible.forEach((line, index) => {
      if (searchRegex.test(line)) found.push(index);
    });
    return found;
  }, [visible, searchRegex]);

  const hitSet = useMemo(() => new Set(hitLines), [hitLines]);
  /** Where the search sits among its hits, 1-based for the read-out. */
  const hitRank = currentHit === null ? 0 : hitLines.indexOf(currentHit) + 1;

  /** Everything the backend says about this pane's source. */
  useEffect(() => {
    const offs: Array<() => void> = [];
    let live = true;
    void (async () => {
      offs.push(
        await listen<DataPayload>("log-data", event => {
          if (event.payload.id !== paneId || event.payload.lines.length === 0) return;
          received.current += event.payload.lines.length;
          setSeen(count => count + event.payload.lines.length);
          // A follow keeps what the filter lets through at the moment it arrives; whatever
          // it drops is not coming back, unlike the local read, which holds everything.
          const pattern = filterRef.current;
          const keep = pattern ? event.payload.lines.filter(line => pattern.test(line)) : event.payload.lines;
          if (keep.length) setLines(list => list.concat(keep));
        }),
      );
      offs.push(
        await listen<StatePayload>("log-state", event => {
          if (event.payload.id !== paneId) return;
          setLink(event.payload.connected ? "connected" : "retrying");
          setNote(event.payload.note);
        }),
      );
      offs.push(
        await listen<ExitPayload>("log-exit", event => {
          if (event.payload.id !== paneId) return;
          setLink("closed");
          setNote(event.payload.reason);
        }),
      );
      if (!live) offs.forEach(off => off());
    })();
    return () => {
      live = false;
      offs.forEach(off => off());
    };
  }, [paneId]);

  /**
   * A remote follow only ever holds what passed the filter, so a new filter re-judges what
   * is held: what is on screen always answers to the filter in the box. A local read keeps
   * the whole snapshot and merely hides lines while a filter is on, so clearing it brings
   * everything back.
   */
  useEffect(() => {
    if (way !== "remote" || !filterRegex) return;
    setLines(list => list.filter(line => filterRegex.test(line)));
  }, [way, filterRegex]);

  /** The pane going away takes its follow with it. */
  useEffect(
    () => () => {
      void invoke("log_close", { id: paneId }).catch(() => {});
    },
    [paneId],
  );

  /**
   * Local: scan for completions on every keystroke. The path names a file often enough that
   * a guess would be worse than a directory listing, which is cheap.
   */
  useEffect(() => {
    if (way !== "local") return;
    let live = true;
    void invoke<Candidate[]>("log_complete", { prefix: path })
      .then(list => {
        if (live) setCandidates(list);
      })
      .catch(() => {
        if (live) setCandidates([]);
      });
    return () => {
      live = false;
    };
  }, [way, path]);

  /** The candidates the popup actually draws (the list can be longer than the box). */
  const shownCandidates = candidates.slice(0, SHOWN_CANDIDATES);

  /** Every new listing starts from the first line; the highlight follows the mouse too. */
  useEffect(() => {
    setPick(0);
  }, [candidates]);

  /**
   * Take a candidate. A directory keeps completing — Tab again walks deeper — a file is
   * the answer, so the popup has done its job and closes.
   */
  const take = useCallback(
    (index: number) => {
      const candidate = candidates[index];
      if (!candidate) return;
      if (candidate.dir) {
        setPath(candidate.path.endsWith("/") ? candidate.path : `${candidate.path}/`);
        setSuggesting(true);
      } else {
        setPath(candidate.path);
        setSuggesting(false);
      }
    },
    [candidates],
  );

  /**
   * Local: a path that can be read is read, with no button in the way. While the path is
   * still half typed the read fails, which is not worth reporting — the candidates are the
   * feedback at that point.
   */
  useEffect(() => {
    if (way !== "local" || !path) return;
    let live = true;
    void invoke<Snapshot>("log_open_file", { path })
      .then(snapshot => {
        if (!live) return;
        setLines(snapshot.lines);
        setSeen(snapshot.lines.length);
        setBytes(snapshot.bytes);
        setLoaded(path);
        setError(null);
      })
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [way, path]);

  /** Lines per second, sampled from a clock that moves (only while a follow is running). */
  useEffect(() => {
    if (way !== "remote" || link !== "connected") {
      setRate(0);
      return;
    }
    const timer = window.setInterval(() => {
      setRate(received.current);
      received.current = 0;
    }, 1000);
    return () => window.clearInterval(timer);
  }, [way, link]);

  // Keep the newest lines in view, but only while following: scrolling up lets go, coming
  // back to the bottom picks it up again.
  useEffect(() => {
    const box = scrollRef.current;
    if (box && follow) box.scrollTop = box.scrollHeight;
  }, [visible, follow]);

  const connect = useCallback(async () => {
    setError(null);
    setNote("");
    setLines([]);
    setSeen(0);
    setBytes(0);
    received.current = 0;
    setLink("idle");
    try {
      await invoke("log_connect", {
        id: paneId,
        host: host.trim(),
        user,
        password,
        // An empty port field means the usual one.
        port: Number(port) || 22,
        path: remotePath.trim(),
      });
    } catch (e) {
      log.error("log", `connect ${host} failed: ${String(e)}`);
      setError(String(e));
      setLink("closed");
    }
  }, [paneId, host, user, password, port, remotePath]);

  const disconnect = useCallback(async () => {
    await invoke("log_close", { id: paneId }).catch(() => {});
    setLink("closed");
    setNote("");
  }, [paneId]);

  /** Switching ways empties the pane: the lines of one source are not the other's. */
  const switchWay = useCallback(
    (next: Way) => {
      if (next === way) return;
      void invoke("log_close", { id: paneId }).catch(() => {});
      setWay(next);
      setLines([]);
      setBytes(0);
      setLoaded(null);
      setLink("idle");
      setNote("");
      setError(null);
      received.current = 0;
      setRate(0);
    },
    [paneId, way],
  );

  // A follow that exists is one that is either up or trying to get back up; only then does
  // cutting it make sense, and only then does the button offer to.
  const running = link === "connected" || link === "retrying";

  const clear = useCallback(() => {
    setLines([]);
    setSeen(0);
    setBytes(0);
    received.current = 0;
  }, []);

  // The window of lines that gets drawn: normally the tail. While the search is sitting on
  // a line, the window moves to hold that line — n / N can walk onto a line far above the
  // tail, and a line that was never drawn is a line the view cannot move to.
  const windowStart =
    currentHit === null
      ? Math.max(0, visible.length - RENDER_LINES)
      : Math.min(Math.max(0, currentHit - Math.floor(RENDER_LINES / 2)), Math.max(0, visible.length - RENDER_LINES));
  const shown = visible.slice(windowStart, windowStart + RENDER_LINES);
  const skipped = windowStart;
  /** Lines still below the window, which the bottom edge says out loud. */
  const below = visible.length - windowStart - shown.length;

  /** Enter on the search line: the pattern takes effect from here on. */
  const confirmSearch = useCallback(() => {
    setSearchOpen(false);
    setSearchPattern(searchDraft);
    // Reading a hit is not reading the tail: following would drag the view back off it.
    setFollow(false);
  }, [searchDraft]);

  /** Esc: the search line and its highlight go away; the text stays for the next `/`. */
  const closeSearch = useCallback(() => {
    setSearchOpen(false);
    setSearchPattern("");
    setCurrentHit(null);
  }, []);

  /** n / N: walk the hits, wrapping at either end. */
  const step = useCallback(
    (dir: 1 | -1) => {
      if (!hitLines.length) return;
      setCurrentHit(current => {
        if (current === null) return dir === 1 ? hitLines[0] : hitLines[hitLines.length - 1];
        const at = hitLines.indexOf(current);
        return hitLines[((at === -1 ? 0 : at) + dir + hitLines.length) % hitLines.length];
      });
    },
    [hitLines],
  );

  useEffect(() => {
    if (!searchOpen) return;
    searchRef.current?.focus();
    searchRef.current?.select();
  }, [searchOpen]);

  // The confirmed pattern lands on the first hit from where the reading is, the way vim
  // does — and again whenever the filter re-cuts the lines the search runs over. Keyed on
  // the pattern and the filter on purpose: lines arriving must not move it off its line.
  useEffect(() => {
    if (!searchRegex || !hitLines.length) {
      setCurrentHit(null);
      return;
    }
    const box = scrollRef.current;
    const top = box ? skipped + Math.floor(box.scrollTop / ROW_PITCH) - (skipped > 0 ? 1 : 0) : 0;
    setCurrentHit(hitLines.find(hit => hit >= Math.max(top, 0)) ?? hitLines[0]);
  }, [searchPattern, filterRegex, searchRegex]);

  // The line the search sits on stays where it can be read.
  useEffect(() => {
    if (currentHit === null) return;
    scrollRef.current?.querySelector(`[data-line="${currentHit}"]`)?.scrollIntoView({ block: "center" });
  }, [currentHit, visible]);

  /** `/` opens the search line, n / N walk the hits — never while typing in a field. */
  useEffect(() => {
    if (!focused) return;
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.ctrlKey || event.altKey || event.metaKey) return;
      const target = event.target as HTMLElement | null;
      const typing =
        !!target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.isContentEditable);
      if (event.key === "/" && !typing) {
        event.preventDefault();
        setSearchOpen(true);
        return;
      }
      if (typing) return;
      if (event.key === "n" || event.key === "N") {
        if (!searchRegex) return;
        event.preventDefault();
        step(event.key === "n" ? 1 : -1);
      } else if (event.key === "Escape" && searchPattern) {
        event.preventDefault();
        closeSearch();
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [focused, searchRegex, searchPattern, step, closeSearch]);

  const header = (
    <div
      className={`flex h-[22px] flex-none items-center gap-1.5 border-b border-surface0 px-1.5 font-mono text-[11px] ${
        focused ? "bg-blue text-base" : "bg-mantle text-subtext0"
      }`}
    >
      <span
        className={`flex-none rounded-sm px-1 py-px text-[9.5px] tracking-wide ${
          focused ? "bg-base/25 text-base" : "bg-mauve/15 text-mauve"
        }`}
      >
        LOG
      </span>
      <span
        className={`flex-none rounded-sm px-1 py-px text-[9.5px] tracking-wide ${way === "remote" ? "bg-sky/15 text-sky" : "bg-peach/15 text-peach"}`}
      >
        {way === "remote" ? "远程" : "文件"}
      </span>
      <span className="flex-none text-[10.5px]">{sourceLabel(way, loaded, user, host, remotePath)}</span>
      <span className="flex-1" />
      {(error ?? note) ? <span className="flex-none text-[10.5px] text-red">{error ?? note}</span> : null}
      {way === "remote" ? (
        <span className="flex flex-none items-center gap-[5px] text-[10px]">
          {link === "idle" ? null : (
            <>
              <span className={`h-[5px] w-[5px] flex-none rounded-full ${dotOf(link)}`} />
              {link === "connected" ? `${rate} 行/秒` : link === "retrying" ? "重连中" : "已断开"}
            </>
          )}
        </span>
      ) : (
        <span className="flex-none text-[10px]">{loaded ? `${lines.length} 行 · ${fmtBytes(bytes)}` : ""}</span>
      )}
    </div>
  );

  const field = (
    <div className="flex flex-none flex-col gap-[7px] border-b border-surface0 bg-mantle px-2.5 py-2">
      <div className="flex gap-1.5">
        <button type="button" onClick={() => switchWay("local")} className={sourceChip(way === "local")}>
          本地文件
        </button>
        <button type="button" onClick={() => switchWay("remote")} className={sourceChip(way === "remote")}>
          远程主机
        </button>
      </div>

      {/* Only one of the two is drawn: the other way's fields are not this way's business. */}
      <div className={way === "local" ? "" : "hidden"}>
        <div className="flex items-center gap-[7px]">
          <span className="flex-none font-mono text-[10.5px] text-overlay0">文件</span>
          <div className="relative min-w-0 flex-1">
            <input
              className={`${FIELD_INPUT} w-full ${suggesting ? "border-blue" : "border-surface0"}`}
              value={path}
              onChange={event => {
                setPath(event.target.value);
                setSuggesting(true);
              }}
              onFocus={() => setSuggesting(true)}
              onBlur={() => setSuggesting(false)}
              onKeyDown={event => {
                if (event.key === "Escape") {
                  setSuggesting(false);
                  return;
                }
                if (!suggesting || !shownCandidates.length) return;
                if (event.key === "ArrowDown" || event.key === "ArrowUp") {
                  event.preventDefault();
                  const step = event.key === "ArrowDown" ? 1 : -1;
                  setPick(at => (at + step + shownCandidates.length) % shownCandidates.length);
                  return;
                }
                // Tab completes instead of walking on to the next field. With nothing to
                // complete it stays a plain Tab, so the field is never a trap.
                if (event.key === "Tab" && !event.shiftKey) {
                  event.preventDefault();
                  take(pick);
                  return;
                }
                if (event.key === "Enter") {
                  event.preventDefault();
                  take(pick);
                }
              }}
            />
            {suggesting && shownCandidates.length ? (
              <div className="absolute inset-x-0 top-[25px] z-10 max-h-[132px] overflow-y-auto rounded-sm border border-surface0 bg-mantle py-[3px] shadow-[0_6px_20px_rgba(0,0,0,0.45)]">
                {shownCandidates.map((candidate, index) => (
                  <button
                    key={candidate.path}
                    type="button"
                    // Keeping the focus in the field is what lets the click land on the option.
                    onMouseDown={event => {
                      event.preventDefault();
                      take(index);
                    }}
                    onMouseEnter={() => setPick(index)}
                    className={`flex w-full items-center px-2 py-[3px] text-left font-mono text-[11px] ${
                      index === pick ? "bg-surface0/60" : "hover:bg-surface0/60"
                    }`}
                  >
                    <span className="min-w-0 truncate">
                      <span className="text-overlay0">{path}</span>
                      <span className="text-subtext1">{candidate.path.slice(path.length)}</span>
                    </span>
                    <span className="ml-auto pl-2 text-[9.5px] text-overlay0">{candidate.dir ? "目录" : "文件"}</span>
                  </button>
                ))}
              </div>
            ) : null}
          </div>
        </div>
      </div>

      <div className={way === "remote" ? "flex flex-col gap-[7px]" : "hidden"}>
        <div className="flex flex-wrap items-center gap-x-3.5 gap-y-[7px]">
          <Field label="IP" className="w-[132px] flex-none">
            <input
              className={`${FIELD_INPUT} border-surface0`}
              value={host}
              onChange={event => setHost(event.target.value)}
            />
          </Field>
          <Field label="用户名" className="w-[104px] flex-none">
            <input
              className={`${FIELD_INPUT} border-surface0`}
              value={user}
              onChange={event => setUser(event.target.value)}
            />
          </Field>
          <Field label="密码" className="w-[126px] flex-none">
            <input
              className={`${FIELD_INPUT} border-surface0`}
              value={password}
              onChange={event => setPassword(event.target.value)}
            />
          </Field>
          <Field label="端口" className="w-[84px] flex-none">
            <input
              className={`${FIELD_INPUT} border-surface0`}
              value={port}
              onChange={event => setPort(event.target.value)}
            />
          </Field>
        </div>
        <div className="flex flex-wrap items-center gap-x-3.5 gap-y-[7px]">
          <Field label="文件" className="min-w-[220px] flex-1">
            <input
              className={`${FIELD_INPUT} border-surface0`}
              placeholder="/var/log/app.log"
              value={remotePath}
              onChange={event => setRemotePath(event.target.value)}
            />
          </Field>
          {/* One button for both ends of the connection: while a follow runs it cuts it,
              otherwise it starts one. */}
          <button
            type="button"
            onClick={() => void (running ? disconnect() : connect())}
            className={`ml-auto flex-none rounded-sm border px-4 py-[3px] font-mono text-[11px] ${
              running
                ? "border-surface0 text-subtext0 hover:border-subtext0 hover:text-fg"
                : "border-green bg-green/10 text-green"
            }`}
          >
            {running ? "断开" : "连接"}
          </button>
        </div>
      </div>
    </div>
  );

  const filterBar = (
    <div className="flex h-[22px] flex-none items-center gap-[7px] border-b border-surface0 bg-mantle px-2 font-mono text-[10.5px] text-overlay0">
      <span className="flex-none">过滤</span>
      <input
        className={`min-w-0 flex-1 bg-transparent font-mono text-[10.5px] outline-none ${filterBad ? "text-red" : "text-yellow"}`}
        placeholder="正则"
        value={filter}
        onChange={event => setFilter(event.target.value)}
      />
      {filter ? (
        <span className="flex-none">
          显示 {visible.length}/{seen}
        </span>
      ) : null}
      <button type="button" onClick={clear} className={chip(false)}>
        清空
      </button>
      {/* The label says what a click does: follow the tail, or let go of it. */}
      <button type="button" onClick={() => setFollow(value => !value)} className={chip(follow)}>
        {follow ? "自由" : "跟随"}
      </button>
    </div>
  );

  // The search line along the bottom edge: an input while it is being typed in, the pattern
  // and where n / N stands once it is confirmed.
  const searchLine =
    searchOpen || searchPattern ? (
      <div className="flex h-[22px] flex-none items-center gap-[7px] border-t border-surface0 bg-mantle px-2 font-mono text-[10.5px] text-overlay0">
        <span className="flex-none">/</span>
        {searchOpen ? (
          <input
            ref={searchRef}
            className={`min-w-0 flex-1 bg-transparent font-mono text-[10.5px] outline-none ${
              searchBad ? "text-red" : "text-yellow"
            }`}
            value={searchDraft}
            onChange={event => setSearchDraft(event.target.value)}
            onKeyDown={event => {
              if (event.key === "Enter") {
                event.preventDefault();
                confirmSearch();
              } else if (event.key === "Escape") {
                event.preventDefault();
                closeSearch();
              }
            }}
          />
        ) : (
          <span className="min-w-0 flex-1 truncate text-yellow">{searchPattern}</span>
        )}
        {searchPattern ? (
          <span className="flex-none">{hitLines.length ? `${hitRank}/${hitLines.length}` : "无命中"}</span>
        ) : null}
      </div>
    ) : null;

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden rounded-md border border-surface0 bg-base">
      {header}
      {field}
      {filterBar}
      {/* Lines scroll sideways rather than wrapping: a wrapped log line reads as two. */}
      <div
        ref={scrollRef}
        onScroll={event => {
          const box = event.currentTarget;
          setFollow(box.scrollHeight - box.scrollTop - box.clientHeight < BOTTOM_GAP);
        }}
        className="min-h-0 flex-1 overflow-auto py-1.5 font-mono text-[11.5px] leading-[19px] text-subtext1"
      >
        {skipped > 0 ? (
          <div className="border-b border-surface0/60 px-2.5 text-overlay0">… 已滚过 {skipped} 行</div>
        ) : null}
        {shown.map((line, index) => {
          const at = skipped + index;
          const here = currentHit === at;
          return (
            <div
              key={at}
              data-line={at}
              className={`w-max min-w-full whitespace-pre border-b border-surface0/60 px-2.5 ${
                here ? "bg-yellow/[0.18]" : hitSet.has(at) ? "bg-yellow/[0.06]" : ""
              }`}
            >
              {line}
            </div>
          );
        })}
        {below > 0 ? (
          <div className="border-b border-surface0/60 px-2.5 text-overlay0">… 下面还有 {below} 行</div>
        ) : null}
      </div>
      {searchLine}
    </div>
  );
}

/** What the head calls the source: the file being shown, or where it is followed. */
function sourceLabel(way: Way, loaded: string | null, user: string, host: string, remotePath: string): string {
  if (way !== "remote") return loaded ?? "";
  if (!remotePath) return host;
  return `${user}@${host}:${remotePath}`;
}

function dotOf(link: Link): string {
  if (link === "connected") return "bg-green";
  if (link === "retrying") return "bg-yellow";
  return "bg-overlay0";
}

function sourceChip(on: boolean): string {
  return `flex-none rounded-sm border px-2.5 py-[2px] font-mono text-[11px] ${
    on ? "border-green bg-green/10 text-green" : "border-surface0 text-subtext0"
  }`;
}

export default LogPane;
