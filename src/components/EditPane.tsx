/**
 * @file EditPane.tsx
 * @brief Edit pane: ask for a directory or a host, then files on the left and one editor
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-24
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 *
 * The pane opens on a card that asks how to reach the files: a directory on this machine,
 * or a host over SSH. Once that is answered the card folds away and the pane is a file list
 * on the left and an editor on the right; closing the pane drops the link, and coming back
 * asks again.
 *
 * One editor instance per pane, one text model per open file: switching tabs swaps the
 * model, so undo history, cursor and scroll survive the switch. Rebuilding the editor per
 * file would throw all three away.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import * as monaco from "monaco-editor/editor/editor.api.js";
import "monaco-editor/basic-languages/monaco.contribution.js";
import { invoke } from "../ipc";
import { log } from "../log";
import { paletteOf, type ThemeName } from "../theme";
import { fontStack } from "./TerminalView";

/** Which way the files get in. */
type Way = "local" | "remote";

/** One row of a directory. */
interface Entry {
  name: string;
  dir: boolean;
  size: number;
}

/** What a directory held when it was read. */
interface Listing {
  path: string;
  entries: Entry[];
}

/** One open file; the text itself lives in a Monaco model. */
interface OpenFile {
  path: string;
  name: string;
  dirty: boolean;
}

/** The link this pane is reading through, once the card has been answered. */
interface Link {
  way: Way;
  /** `user@host` for a remote link, empty for a local one. */
  who: string;
  /** The directory the tree is rooted at; typing another path in the tree head moves it. */
  root: string;
}

/** Candidates the picker walks. */
interface Candidate {
  path: string;
  name: string;
  where: string;
}

export interface EditPaneProps {
  /** Pane id, also the id the backend keeps the link under */
  paneId: string;
  /** Active pane: blue head */
  focused?: boolean;
  /** Reporting upwards: the page shows this in the top bar */
  onLabel?: (paneId: string, label: string) => void;
  /** Editor font, from the settings page */
  fontSize?: number;
  /** Family name, as the settings page holds it; the CJK fallback is added here */
  fontFamily: string;
  /** Catppuccin flavor, so the editor matches the rest */
  theme?: ThemeName;
}

/** The flavor the pane is drawn in until the settings arrive. */
const DEFAULT_THEME: ThemeName = "mocha";

/** Language handed to Monaco for a path; anything unknown is plain text. */
function languageOf(path: string): string {
  const name = path.toLowerCase();
  const ext = name.includes(".") ? name.slice(name.lastIndexOf(".") + 1) : "";
  switch (ext) {
    case "sh":
    case "bash":
      return "shell";
    case "md":
      return "markdown";
    case "json":
      return "json";
    case "rs":
      return "rust";
    case "c":
    case "h":
      return "c";
    case "cc":
    case "cpp":
    case "hpp":
      return "cpp";
    case "py":
      return "python";
    case "js":
      return "javascript";
    case "ts":
      return "typescript";
    case "toml":
    case "ini":
    case "conf":
      return "ini";
    case "yml":
    case "yaml":
      return "yaml";
    default:
      return "plaintext";
  }
}

/** The last piece of a path, which is what a tab shows. */
function baseName(path: string): string {
  const at = path.lastIndexOf("/");
  return at < 0 ? path : path.slice(at + 1);
}

/** A child of a directory, whichever way the link reads it. */
function under(dir: string, name: string): string {
  return `${dir.replace(/\/+$/, "")}/${name}`;
}

/** Monaco theme built from the interface palette, defined once per flavor. */
function defineTheme(theme: ThemeName): void {
  const palette = paletteOf(theme);
  monaco.editor.defineTheme("melogger", {
    base: theme === "latte" ? "vs" : "vs-dark",
    inherit: true,
    rules: [
      { token: "comment", foreground: palette.overlay0.slice(1), fontStyle: "italic" },
      { token: "keyword", foreground: palette.mauve.slice(1) },
      { token: "string", foreground: palette.green.slice(1) },
      { token: "number", foreground: palette.yellow.slice(1) },
      { token: "type", foreground: palette.sky.slice(1) },
      { token: "delimiter", foreground: palette.subtext0.slice(1) },
    ],
    colors: {
      "editor.background": palette.base,
      "editor.foreground": palette.fg,
      "editorLineNumber.foreground": palette.overlay0,
      "editorLineNumber.activeForeground": palette.subtext0,
      "editor.lineHighlightBackground": palette.surface0 + "55",
      "editor.selectionBackground": palette.blue + "44",
      "editorCursor.foreground": palette.blue,
      "editorIndentGuide.background1": palette.surface0,
    },
  });
}

/** One completion candidate for a directory being typed. */
interface PathCandidate {
  path: string;
  dir: boolean;
}

/**
 * A directory field that completes as it is typed: the candidates hang under the input,
 * the arrows walk them, Tab takes the highlighted one (directories keep their slash, so
 * Tab walks deeper), Enter commits what is in the box.
 */
function PathInput({
  paneId,
  value,
  onChange,
  onCommit,
  placeholder,
  className,
}: {
  paneId: string;
  value: string;
  onChange: (next: string) => void;
  onCommit: (next: string) => void;
  placeholder?: string;
  className?: string;
}) {
  const [candidates, setCandidates] = useState<PathCandidate[]>([]);
  const [picked, setPicked] = useState(0);
  const [open, setOpen] = useState(false);

  const ask = async (prefix: string) => {
    try {
      const list = await invoke<PathCandidate[]>("edit_complete", { id: paneId, prefix });
      const dirs = list.filter(candidate => candidate.dir);
      setCandidates(dirs);
      setPicked(0);
      setOpen(dirs.length > 0);
    } catch (error) {
      log.warn("edit", `${paneId} complete failed: ${String(error)}`);
      setCandidates([]);
      setOpen(false);
    }
  };

  return (
    <span className="relative min-w-0 flex-1">
      <input
        value={value}
        placeholder={placeholder}
        spellCheck={false}
        onChange={event => {
          onChange(event.target.value);
          void ask(event.target.value);
        }}
        onKeyDown={event => {
          if (event.key === "ArrowDown" || (event.key === "Tab" && !event.shiftKey)) {
            if (!open || candidates.length === 0) return;
            event.preventDefault();
            if (event.key === "Tab") {
              const take = candidates[picked].path;
              onChange(take);
              void ask(take);
            } else {
              setPicked(current => (current + 1) % candidates.length);
            }
            return;
          }
          if (event.key === "ArrowUp") {
            if (!open || candidates.length === 0) return;
            event.preventDefault();
            setPicked(current => (current - 1 + candidates.length) % candidates.length);
            return;
          }
          if (event.key === "Enter") {
            event.preventDefault();
            setOpen(false);
            onCommit(value);
            return;
          }
          if (event.key === "Escape") setOpen(false);
        }}
        className={className}
      />

      {open ? (
        <div className="absolute left-0 top-full z-20 mt-1 max-h-[168px] w-full overflow-y-auto rounded-sm border border-surface0 bg-mantle py-1 shadow-[0_6px_20px_rgba(0,0,0,0.45)]">
          {candidates.map((candidate, index) => (
            <div
              key={candidate.path}
              onMouseDown={event => {
                event.preventDefault();
                onChange(candidate.path);
                void ask(candidate.path);
              }}
              className={`flex h-[21px] cursor-pointer items-center px-2 font-mono text-[11px] ${
                index === picked ? "bg-blue/15 text-fg" : "text-subtext0"
              }`}
            >
              <span className="truncate">{candidate.path}</span>
            </div>
          ))}
        </div>
      ) : null}
    </span>
  );
}

function EditPane({
  paneId,
  focused = false,
  onLabel,
  fontSize = 12,
  fontFamily,
  theme = DEFAULT_THEME,
}: EditPaneProps) {
  // The card: which way, and what has been typed into it.
  const [way, setWay] = useState<Way>("remote");
  const [host, setHost] = useState("");
  const [user, setUser] = useState("");
  const [password, setPassword] = useState("");
  const [port, setPort] = useState("22");
  const [dir, setDir] = useState("");
  const [link, setLink] = useState<Link | null>(null);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");

  // The tree: what has been read, and which directories are open.
  const [dirs, setDirs] = useState<Map<string, Entry[]>>(new Map());
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [treeShown, setTreeShown] = useState(true);
  /** The path in the tree head; Enter there moves the tree to it. */
  const [browse, setBrowse] = useState("");

  // The editor: one instance, one model per file.
  const [tabs, setTabs] = useState<OpenFile[]>([]);
  const [active, setActive] = useState<string | null>(null);
  const [cursor, setCursor] = useState("1:1");
  const [saveNote, setSaveNote] = useState("");

  // The picker.
  const [picker, setPicker] = useState(false);
  const [query, setQuery] = useState("");
  const [picked, setPicked] = useState(0);

  const hostRef = useRef<HTMLDivElement | null>(null);
  const editorRef = useRef<monaco.editor.IStandaloneCodeEditor | null>(null);
  const models = useRef(new Map<string, monaco.editor.ITextModel>());
  const pickerRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    onLabel?.(paneId, link ? (link.way === "local" ? link.root : `${link.who}:${link.root}`) : "编辑");
  }, [paneId, link, onLabel]);

  // The link goes away with the pane: nothing is kept for a later visit.
  useEffect(() => {
    return () => {
      void invoke("edit_close", { id: paneId }).catch(() => {});
      for (const model of models.current.values()) model.dispose();
      models.current.clear();
    };
  }, [paneId]);

  // Theme changes reach the editor as a re-defined theme, not a rebuild.
  useEffect(() => {
    defineTheme(theme);
    monaco.editor.setTheme("melogger");
  }, [theme]);

  /** One directory into the tree. */
  const readDir = useCallback(
    async (path: string) => {
      const listing = await invoke<Listing>("edit_list", { id: paneId, path });
      setDirs(current => new Map(current).set(listing.path, listing.entries));
      return listing;
    },
    [paneId],
  );

  /** Answer the card: open the directory, or log in and land in the login directory. */
  const open = async () => {
    setBusy(true);
    setNote("");
    try {
      const listing =
        way === "local"
          ? await invoke<Listing>("edit_open_local", { id: paneId, dir })
          : await invoke<Listing>("edit_open_remote", {
              id: paneId,
              host,
              user,
              password,
              port: Number(port) || 22,
              dir: "",
            });
      setDirs(new Map([[listing.path, listing.entries]]));
      setExpanded(new Set([listing.path]));
      setBrowse(listing.path);
      setLink({
        way,
        who: way === "local" ? "" : `${user}@${host}`,
        root: listing.path,
      });
      log.info("edit", `${paneId} linked ${way} ${listing.path}`);
    } catch (error) {
      setNote(String(error));
      log.warn("edit", `${paneId} open failed: ${String(error)}`);
    } finally {
      setBusy(false);
    }
  };

  /** Move the tree to another directory, typed into its head. */
  const jumpTo = async (path: string) => {
    const asked = path.trim();
    if (!asked) return;
    try {
      const listing = await readDir(asked);
      setLink(current => (current ? { ...current, root: listing.path } : current));
      setExpanded(current => new Set(current).add(listing.path));
      setBrowse(listing.path);
      setNote("");
    } catch (error) {
      setNote(String(error));
      log.warn("edit", `${paneId} list ${asked} failed: ${String(error)}`);
    }
  };

  /** Clicking a directory loads it the first time and folds it afterwards. */
  const toggleDir = async (path: string) => {
    if (!expanded.has(path)) {
      try {
        if (!dirs.has(path)) await readDir(path);
        setExpanded(current => new Set(current).add(path));
      } catch (error) {
        setNote(String(error));
        log.warn("edit", `${paneId} list ${path} failed: ${String(error)}`);
      }
      return;
    }
    setExpanded(current => {
      const next = new Set(current);
      next.delete(path);
      return next;
    });
  };

  /** Open a file: reuse its model when it was opened before. */
  const openFile = async (path: string) => {
    try {
      let model = models.current.get(path);
      if (!model) {
        const text = await invoke<string>("edit_read", { id: paneId, path });
        model = monaco.editor.createModel(text, languageOf(path), monaco.Uri.file(path));
        model.onDidChangeContent(() => {
          setTabs(current => current.map(file => (file.path === path ? { ...file, dirty: true } : file)));
        });
        models.current.set(path, model);
      }
      setTabs(current =>
        current.some(file => file.path === path) ? current : [...current, { path, name: baseName(path), dirty: false }],
      );
      setActive(path);
    } catch (error) {
      setNote(String(error));
      log.warn("edit", `${paneId} read ${path} failed: ${String(error)}`);
    }
  };

  /** Put the editor on the active file, creating the editor the first time one opens. */
  useEffect(() => {
    const host = hostRef.current;
    if (!host || !link) return;
    let editor = editorRef.current;
    if (!editor) {
      defineTheme(theme);
      editor = monaco.editor.create(host, {
        theme: "melogger",
        value: "",
        language: "plaintext",
        fontSize,
        // The same stack the terminal and the interface use, so all three read alike.
        fontFamily: fontStack(fontFamily),
        lineHeight: Math.round(fontSize * 1.2),
        minimap: { enabled: false },
        scrollBeyondLastLine: false,
        automaticLayout: false,
        renderLineHighlight: "line",
        tabSize: 4,
      });
      editor.onDidChangeCursorPosition(event => {
        setCursor(`${event.position.lineNumber}:${event.position.column}`);
      });
      editorRef.current = editor;
    }
    // The pane is a percentage box that changes without telling anyone.
    const observer = new ResizeObserver(() => editorRef.current?.layout());
    observer.observe(host);
    return () => observer.disconnect();
  }, [link, fontSize, fontFamily, theme]);

  useEffect(() => {
    if (!editorRef.current || !active) return;
    const model = models.current.get(active);
    if (!model || model.isDisposed()) return;
    if (editorRef.current.getModel() !== model) editorRef.current.setModel(model);
    editorRef.current.focus();
  }, [active]);

  // Font settings change without rebuilding the editor.
  useEffect(() => {
    editorRef.current?.updateOptions({
      fontSize,
      fontFamily: fontStack(fontFamily),
      lineHeight: Math.round(fontSize * 1.2),
    });
  }, [fontSize, fontFamily]);

  /** Write the active file back through the link. */
  const save = async () => {
    if (!active) return;
    const model = models.current.get(active);
    if (!model) return;
    try {
      await invoke("edit_save", { id: paneId, path: active, text: model.getValue() });
      setTabs(current => current.map(file => (file.path === active ? { ...file, dirty: false } : file)));
      setSaveNote("");
    } catch (error) {
      setSaveNote(String(error));
      log.error("edit", `${paneId} save ${active} failed: ${String(error)}`);
    }
  };

  /** Close one tab; the last one leaves the editor empty. */
  const closeTab = (path: string) => {
    setTabs(current => {
      const next = current.filter(file => file.path !== path);
      if (path === active) setActive(next.length ? next[next.length - 1].path : null);
      return next;
    });
    const model = models.current.get(path);
    if (model) {
      model.dispose();
      models.current.delete(path);
    }
  };

  /** Everything the picker can offer: every file read so far, in the directories opened. */
  const candidates = useMemo<Candidate[]>(() => {
    const out: Candidate[] = [];
    for (const [path, entries] of dirs) {
      for (const entry of entries) {
        if (entry.dir) continue;
        out.push({ path: under(path, entry.name), name: entry.name, where: path });
      }
    }
    return out;
  }, [dirs]);

  const matches = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return candidates;
    return candidates.filter(
      candidate => candidate.name.toLowerCase().includes(needle) || candidate.path.toLowerCase().includes(needle),
    );
  }, [candidates, query]);

  /** The keys this pane answers; the card's fields and the picker type into themselves. */
  useEffect(() => {
    if (!focused) return;
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const typing = target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA");
      if (!event.ctrlKey || event.altKey || event.metaKey) return;
      if (event.key === "`" || event.key === "~") {
        event.preventDefault();
        setTreeShown(current => !current);
        return;
      }
      if (typing) return;
      if (event.key.toLowerCase() === "p" && link) {
        event.preventDefault();
        setPicker(true);
        setQuery("");
        setPicked(0);
        return;
      }
      if (event.key.toLowerCase() === "s" && link) {
        event.preventDefault();
        void save();
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [focused, link, active, query]);

  // The picker takes the keyboard while it is up.
  useEffect(() => {
    if (picker) pickerRef.current?.focus();
  }, [picker]);

  // Fold the tree away when it is hidden.
  const rows = useMemo(() => {
    const out: Array<{ key: string; depth: number; entry: Entry; path: string }> = [];
    if (!link) return out;
    const walk = (path: string, depth: number) => {
      for (const entry of dirs.get(path) ?? []) {
        const child = under(path, entry.name);
        out.push({ key: child, depth, entry, path: child });
        if (entry.dir && expanded.has(child)) walk(child, depth + 1);
      }
    };
    walk(link.root, 0);
    return out;
  }, [link, dirs, expanded]);

  /** Files with unsaved changes: the tab and its tree row both carry a dot. */
  const dirtyPaths = useMemo(() => new Set(tabs.filter(file => file.dirty).map(file => file.path)), [tabs]);

  if (!link) {
    return (
      <div className="flex h-full w-full flex-col overflow-hidden bg-base">
        <div className="flex h-[26px] flex-none items-center gap-2 border-b border-surface0 bg-[#1b2133] px-2 font-mono text-[10px]">
          <span className="rounded-sm bg-teal/15 px-1.5 py-px tracking-[0.5px] text-teal">EDIT</span>
        </div>
        <div className="relative flex flex-1 items-center justify-center bg-crust/55">
          <div className="w-[440px] max-w-[calc(100%-40px)] rounded-md border border-surface0 bg-mantle shadow-[0_18px_44px_rgba(0,0,0,0.55)]">
            <div className="flex items-center gap-2 border-b border-surface0 px-2.5 py-2">
              <button
                type="button"
                onClick={() => setWay("local")}
                className={`rounded-sm border px-2 py-px font-mono text-[10.5px] ${
                  way === "local"
                    ? "border-green bg-green/10 text-green"
                    : "border-surface0 text-subtext0 hover:text-fg"
                }`}
              >
                本地目录
              </button>
              <button
                type="button"
                onClick={() => setWay("remote")}
                className={`rounded-sm border px-2 py-px font-mono text-[10.5px] ${
                  way === "remote"
                    ? "border-green bg-green/10 text-green"
                    : "border-surface0 text-subtext0 hover:text-fg"
                }`}
              >
                远程主机
              </button>
            </div>

            <div className="flex flex-wrap items-center gap-2 px-2.5 py-2.5">
              {way === "remote" ? (
                <>
                  <span className="font-mono text-[10.5px] text-overlay0">IP</span>
                  <input
                    value={host}
                    onChange={event => setHost(event.target.value)}
                    className="w-[128px] rounded-sm border border-surface0 bg-base/60 px-2 py-px font-mono text-[11px] text-subtext1 outline-none"
                  />
                  <span className="font-mono text-[10.5px] text-overlay0">用户名</span>
                  <input
                    value={user}
                    onChange={event => setUser(event.target.value)}
                    className="w-[100px] rounded-sm border border-surface0 bg-base/60 px-2 py-px font-mono text-[11px] text-subtext1 outline-none"
                  />
                  <span className="font-mono text-[10.5px] text-overlay0">密码</span>
                  <input
                    value={password}
                    onChange={event => setPassword(event.target.value)}
                    type="text"
                    className="w-[112px] rounded-sm border border-surface0 bg-base/60 px-2 py-px font-mono text-[11px] text-subtext1 outline-none"
                  />
                  <span className="font-mono text-[10.5px] text-overlay0">端口</span>
                  <input
                    value={port}
                    onChange={event => setPort(event.target.value)}
                    className="w-[56px] rounded-sm border border-surface0 bg-base/60 px-2 py-px font-mono text-[11px] text-subtext1 outline-none"
                  />
                </>
              ) : (
                <>
                  <span className="font-mono text-[10.5px] text-overlay0">目录</span>
                  <PathInput
                    paneId={paneId}
                    value={dir}
                    onChange={setDir}
                    onCommit={() => void open()}
                    placeholder="/var/log"
                    className="w-full rounded-sm border border-surface0 bg-base/60 px-2 py-px font-mono text-[11px] text-subtext1 outline-none"
                  />
                </>
              )}
            </div>

            {note ? <div className="px-2.5 pb-2 font-mono text-[10.5px] text-red">{note}</div> : null}

            <div className="flex items-center gap-2 border-t border-surface0 px-2.5 py-2">
              <span className="flex-1" />
              <button
                type="button"
                onClick={() => void open()}
                disabled={busy || (way === "local" ? dir.trim() === "" : host.trim() === "" || user.trim() === "")}
                className="rounded-sm border border-green bg-green/10 px-3.5 py-px font-mono text-[11px] text-green disabled:opacity-50"
              >
                {busy ? "…" : way === "local" ? "打开" : "连接"}
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div
      className={`flex h-full w-full flex-col overflow-hidden bg-base ${focused ? "ring-1 ring-inset ring-blue" : ""}`}
    >
      <div
        className={`flex h-[26px] flex-none items-center gap-2 border-b border-surface0 px-2 font-mono text-[10px] ${
          focused ? "bg-blue" : "bg-mantle"
        }`}
      >
        <span className="rounded-sm bg-teal/15 px-1.5 py-px tracking-[0.5px] text-teal">EDIT</span>
        <span className="rounded-sm bg-sky/15 px-1.5 py-px text-sky">{link.way === "local" ? "本地" : "远程"}</span>
        <span className={`min-w-0 flex-1 truncate text-[11px] ${focused ? "text-base" : "text-fg"}`}>
          {link.way === "local" ? link.root : `${link.who}:${link.root}`}
        </span>
        {note ? <span className="text-[10px] text-red">{note}</span> : null}
        {saveNote ? <span className="text-[10px] text-red">{saveNote}</span> : null}
      </div>

      <div className="flex min-h-0 flex-1">
        {treeShown ? (
          <div className="flex w-[196px] flex-none flex-col border-r border-surface0 bg-mantle">
            <div className="flex h-[22px] flex-none items-center border-b border-surface0 px-2.5">
              <PathInput
                paneId={paneId}
                value={browse}
                onChange={setBrowse}
                onCommit={next => void jumpTo(next)}
                className="w-full bg-transparent font-mono text-[10.5px] text-overlay0 outline-none focus:text-subtext0"
              />
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto py-1">
              {rows.map(row => (
                <div
                  key={row.key}
                  onClick={() => {
                    if (row.entry.dir) void toggleDir(row.path);
                    else void openFile(row.path);
                  }}
                  style={{ paddingLeft: 10 + row.depth * 12 }}
                  className={`flex h-[21px] cursor-pointer items-center gap-1.5 pr-2.5 font-mono text-[11.5px] ${
                    row.entry.dir
                      ? "text-subtext0"
                      : row.path === active
                        ? "bg-blue/15 text-fg"
                        : "text-subtext1 hover:bg-surface0/40"
                  }`}
                >
                  <span className="w-2 flex-none text-[9px] text-overlay0">
                    {row.entry.dir ? (expanded.has(row.path) ? "▾" : "▸") : ""}
                  </span>
                  <span className="truncate">{row.entry.name}</span>
                  {dirtyPaths.has(row.path) ? (
                    <span className="ml-auto h-[5px] w-[5px] flex-none rounded-full bg-fg" />
                  ) : null}
                </div>
              ))}
            </div>
          </div>
        ) : null}

        <div className="relative flex min-w-0 flex-1 flex-col">
          <div className="flex h-[26px] flex-none items-stretch overflow-x-auto border-b border-surface0 bg-mantle">
            {tabs.map(file => (
              <div
                key={file.path}
                onClick={() => setActive(file.path)}
                className={`flex cursor-pointer items-center gap-1.5 border-r border-surface0 px-2.5 font-mono text-[11px] ${
                  file.path === active
                    ? "bg-base text-fg shadow-[inset_0_2px_0_var(--color-blue)]"
                    : "text-overlay0 hover:text-subtext1"
                }`}
              >
                <span className={`h-[5px] w-[5px] flex-none rounded-full bg-fg ${file.dirty ? "" : "hidden"}`} />
                <span>{file.name}</span>
                <span
                  onClick={event => {
                    event.stopPropagation();
                    closeTab(file.path);
                  }}
                  className="text-overlay0 hover:text-red"
                >
                  ✕
                </span>
              </div>
            ))}
          </div>

          {picker ? (
            <div className="absolute left-2 top-[30px] z-10 w-[360px] overflow-hidden rounded-sm border border-surface0 bg-mantle shadow-[0_12px_30px_rgba(0,0,0,0.5)]">
              <input
                ref={pickerRef}
                value={query}
                onChange={event => {
                  setQuery(event.target.value);
                  setPicked(0);
                }}
                onKeyDown={event => {
                  if (event.key === "Tab") {
                    event.preventDefault();
                    if (matches.length) {
                      const step = event.shiftKey ? -1 : 1;
                      setPicked(current => (current + step + matches.length) % matches.length);
                    }
                    return;
                  }
                  if (event.key === "Enter") {
                    event.preventDefault();
                    const pick = matches[picked];
                    if (pick) {
                      void openFile(pick.path);
                      setPicker(false);
                    }
                    return;
                  }
                  if (event.key === "Escape") {
                    event.preventDefault();
                    setPicker(false);
                  }
                }}
                spellCheck={false}
                className="w-full border-b border-surface0 bg-base px-2 py-1 font-mono text-[11.5px] text-fg outline-none"
              />
              <div className="max-h-[168px] overflow-y-auto py-1">
                {matches.length === 0 ? (
                  <div className="px-2 py-1.5 font-mono text-[10.5px] text-overlay0">无匹配</div>
                ) : (
                  matches.map((candidate, index) => (
                    <div
                      key={candidate.path}
                      onMouseDown={event => {
                        event.preventDefault();
                        void openFile(candidate.path);
                        setPicker(false);
                      }}
                      className={`flex h-[21px] cursor-pointer items-center gap-2 px-2 font-mono text-[11.5px] ${
                        index === picked ? "bg-blue/15 text-fg" : "text-subtext1"
                      }`}
                    >
                      <span>{candidate.name}</span>
                      <span className="ml-auto text-[10.5px] text-overlay0">{candidate.where}</span>
                    </div>
                  ))
                )}
              </div>
            </div>
          ) : null}

          <div ref={hostRef} className="min-h-0 flex-1" />

          <div className="flex h-[22px] flex-none items-center gap-2.5 border-t border-surface0 bg-mantle px-2.5 font-mono text-[10.5px] text-overlay0">
            <span className="truncate text-subtext0">{active ?? ""}</span>
            <span>{cursor}</span>
            <span className="flex-1" />
            <span>{active ? languageOf(active) : ""}</span>
            <span>UTF-8</span>
            <span>LF</span>
          </div>
        </div>
      </div>
    </div>
  );
}

export default EditPane;
