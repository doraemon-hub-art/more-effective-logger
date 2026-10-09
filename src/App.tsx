/**
 * @file App.tsx
 * @brief App root: pages, top bar, pane tree, context menu, status bar
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-08-23
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 *
 * A page is a workspace: its own pane tree, its own focus, its own shells. Pages are
 * keyboard-only (the design draws no page tabs): Ctrl+Shift+T adds one, Ctrl+Shift+W
 * closes it, Alt+PageUp/PageDown switch. Inactive pages stay mounted and are only
 * hidden — unmounting a page would dispose its terminals and kill the running shells,
 * which is not what switching means.
 * Alt+arrows move the focus between the panes of the current page (geometric
 * neighbours, see neighborInDirection).
 * Panes report their state upwards (onStatus); the page is the only place that knows
 * about pages, focus, the top bar and the status bar.
 * Splitting is mouse-driven for now (right click -> direction -> pane type).
 */
import { useEffect, useMemo, useRef, useState, type MouseEvent } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import TermPane from "./components/TermPane";
import SerialPane from "./components/SerialPane";
import LogPane from "./components/LogPane";
import { fontStack, type TerminalStatus } from "./components/TerminalView";
import StatusBar, { type SysStats } from "./components/StatusBar";
import ContextMenu, { type MenuItem } from "./components/ContextMenu";
import PaneTypeMenu from "./components/PaneTypeMenu";
import TopBar, { type PageCell } from "./components/TopBar";
import SettingsPage, { BASE_FONT_SIZE, DEFAULT_SETTINGS, type AppSettings } from "./components/SettingsPage";
import { paletteOf } from "./theme";
import Layout from "./layout/Layout";
import {
  clampRatio,
  collectPanes,
  columnCount,
  createPane,
  createTermPane,
  leafIds,
  removePane,
  setSplitRatio,
  splitPane,
  type BranchPath,
  type PaneLeaf,
  type PaneNode,
  type PaneType,
  type SplitDir,
  type SplitNode,
} from "./layout/paneTree";
import { neighborInDirection, paneBoxes, type Direction, type PaneBox } from "./layout/paneRects";
import { log } from "./log";

/**
 * A page is either a workspace (its own pane tree, its own focus) or the settings page.
 * The settings page holds no panes, so it carries no tree — which is also what keeps it
 * out of everything that works on panes.
 */
type Page =
  { id: string; title: string; kind: "workspace"; tree: PaneNode } | { id: string; title: string; kind: "settings" };

/** Where a context menu is open, and which pane of which page it belongs to. */
interface MenuState {
  x: number;
  y: number;
  pageId: string;
  paneId: string;
}

/**
 * A keyboard split in progress: the direction is fixed (it came from the arrow), the
 * menu only picks what the new half will be.
 */
interface SplitMenuState {
  pageId: string;
  paneId: string;
  dir: SplitDir;
  x: number;
  y: number;
}

function newPageId(): string {
  return `page-${Math.random().toString(36).slice(2, 8)}`;
}

/** Alt+arrows move pane focus; these are the arrow spellings in `event.key`. */
const ARROW_DIRECTION: Record<string, Direction | undefined> = {
  arrowleft: "left",
  arrowright: "right",
  arrowup: "up",
  arrowdown: "down",
};

/**
 * The values read from the settings file, kept only where the type matches the default:
 * a hand-edited file with a wrong type falls back instead of breaking the app, and an
 * unknown key is dropped rather than carried around.
 */
function mergeSettings(saved: Record<string, unknown>): Partial<AppSettings> {
  const patch: Record<string, unknown> = {};
  for (const key of Object.keys(DEFAULT_SETTINGS) as Array<keyof AppSettings>) {
    const value = saved[key];
    if (typeof value === typeof DEFAULT_SETTINGS[key]) patch[key] = value;
  }
  return patch as Partial<AppSettings>;
}

/** What the app comes back to after a start: the pages, the focus, the shell directories. */
interface Session {
  pages: Page[];
  activePageId: string | null;
  /** Focused pane of each page, by page id. */
  focusByPage: Record<string, string | null>;
  /** Directory each terminal pane's shell was sitting in, by pane id. */
  cwdByPane: Record<string, string>;
}

/** A tree the app can draw again: the right shape, and ratios inside the usable range. */
function restoreTree(node: unknown): PaneNode | null {
  if (!node || typeof node !== "object") return null;
  const branch = node as SplitNode;
  if (branch.kind === "split") {
    if (branch.dir !== "row" && branch.dir !== "col") return null;
    const first = restoreTree(branch.first);
    const second = restoreTree(branch.second);
    if (!first || !second) return null;
    return { kind: "split", dir: branch.dir, ratio: clampRatio(branch.ratio), first, second };
  }
  const leaf = node as PaneLeaf;
  if (leaf.kind !== "pane" || typeof leaf.id !== "string") return null;
  if (leaf.type !== "term" && leaf.type !== "serial" && leaf.type !== "log") return null;
  return { kind: "pane", id: leaf.id, type: leaf.type };
}

/**
 * The session the file holds, kept only as far as it is something the app can draw: a
 * stranger's file, or one from an older version, must not take the window down with it.
 */
function restoreSession(saved: unknown): Session | null {
  const value = saved as Partial<Session> | null;
  if (!value || !Array.isArray(value.pages)) return null;

  const pages: Page[] = [];
  for (const page of value.pages as Page[]) {
    if (!page || typeof page.id !== "string") continue;
    const title = typeof page.title === "string" ? page.title : "";
    if (page.kind === "settings") {
      pages.push({ id: page.id, title, kind: "settings" });
      continue;
    }
    if (page.kind !== "workspace") continue;
    const tree = restoreTree((page as { tree?: unknown }).tree);
    if (tree) pages.push({ id: page.id, title, kind: "workspace", tree });
  }
  if (pages.length === 0) return null;

  const wanted = value.activePageId;
  const activePageId = pages.some(page => page.id === wanted) ? (wanted as string) : pages[0].id;

  const focusByPage: Record<string, string | null> = {};
  const cwdByPane: Record<string, string> = {};
  // Only a terminal has a directory to come back to: a serial pane, or any pane kind added
  // later, brings nothing but itself.
  const termIds = new Set<string>();
  for (const page of pages) {
    if (page.kind !== "workspace") continue;
    const panes = collectPanes(page.tree);
    panes.forEach(pane => {
      if (pane.type === "term") termIds.add(pane.id);
    });
    const focused = (value.focusByPage ?? {})[page.id];
    if (typeof focused === "string" && panes.some(pane => pane.id === focused)) {
      focusByPage[page.id] = focused;
    }
  }
  for (const [paneId, dir] of Object.entries(value.cwdByPane ?? {})) {
    if (termIds.has(paneId) && typeof dir === "string") cwdByPane[paneId] = dir;
  }
  return { pages, activePageId, focusByPage, cwdByPane };
}

/** The number in a title like "页 3", so a restored page number is never handed out twice. */
function pageNumber(title: string): number {
  const match = /^页\s*(\d+)$/.exec(title);
  return match ? Number(match[1]) : 0;
}

function App() {
  const [stats, setStats] = useState<SysStats | null>(null);
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  /** True once the settings file and the session have been read (or given up on). */
  const [booted, setBooted] = useState(false);
  /** JSON of the settings as last written, so nothing is written back unchanged. */
  const writtenSettings = useRef<string | null>(null);

  // Whole-interface zoom: the webview's own page zoom, browser Ctrl+wheel semantics —
  // UI and terminal screens scale together and stay sharp, no cols/rows change.
  useEffect(() => {
    void invoke("set_zoom", { scale: settings.zoom }).catch(error => {
      log.error("ui", `set_zoom(${settings.zoom}) failed: ${String(error)}`);
    });
  }, [settings.zoom]);

  /** Installed monospace families; the settings page picks from these, not from a list
   * we ship. Empty until the scan answers, and on machines without fontconfig. */
  const [fonts, setFonts] = useState<string[]>([]);
  useEffect(() => {
    void invoke<string[]>("font_list")
      .then(list => {
        log.debug("font", `${list.length} families from the scan`);
        setFonts(list);
      })
      .catch(error => {
        log.warn("font", `font_list failed: ${String(error)}`);
      });
  }, []);

  // A family that is not installed cannot be used: when the scan comes back without
  // the designed default, the first installed family takes its place.
  useEffect(() => {
    if (fonts.length && !fonts.includes(settings.fontFamily)) {
      setSettings(current => ({ ...current, fontFamily: fonts[0] }));
    }
  }, [fonts, settings.fontFamily]);

  // The family reaches the interface through the `--font-mono` token (every font-mono
  // class reads it) and each terminal screen through the prop handed down below.
  useEffect(() => {
    document.documentElement.style.setProperty("--font-mono", fontStack(settings.fontFamily));
  }, [settings.fontFamily]);

  // The flavor reaches the interface by rewriting the --color-* tokens index.css
  // registers; every bg-base / text-fg / border-surface0 follows on its own. Terminals
  // get the same palette through the prop below.
  useEffect(() => {
    // Object.entries widens the values to unknown on a plain interface, hence the cast.
    for (const [token, value] of Object.entries(paletteOf(settings.theme)) as Array<[string, string]>) {
      document.documentElement.style.setProperty(`--color-${token}`, value);
    }
  }, [settings.theme]);

  const [pages, setPages] = useState<Page[]>(() => [
    { id: newPageId(), title: "页 1", kind: "workspace", tree: createTermPane() },
  ]);
  const [activePageId, setActivePageId] = useState<string | null>(null);
  const [focusByPage, setFocusByPage] = useState<Record<string, string | null>>({});
  const [paneStatus, setPaneStatus] = useState<Record<string, TerminalStatus>>({});
  /** Directory a restored pane starts its shell in, by pane id; fresh panes have none. */
  const [startCwd, setStartCwd] = useState<Record<string, string>>({});
  /** Non-terminal panes describe themselves (serial: device + rate). */
  const [paneLabel, setPaneLabel] = useState<Record<string, string>>({});
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [splitMenu, setSplitMenu] = useState<SplitMenuState | null>(null);
  const titleSeq = useRef(1);

  /**
   * Boot: read the settings file, then — when the startup setting asks for it — the layout
   * left behind last time. Drawing waits for both: no default page is spawned only to be
   * thrown away, and the window never flashes the default flavor on its way to the saved one.
   */
  useEffect(() => {
    void (async () => {
      let restored: Session | null = null;
      try {
        const saved = await invoke<Record<string, unknown> | null>("settings_load");
        const patch = saved ? mergeSettings(saved) : {};
        if (saved) setSettings(current => ({ ...current, ...patch }));
        log.info("app", saved ? "settings read" : "no settings file, using the defaults");
        if ((patch.startup ?? DEFAULT_SETTINGS.startup) === "restore") {
          restored = restoreSession(await invoke("session_load"));
          log.info("app", restored ? `session restored: ${restored.pages.length} page(s)` : "no session to restore");
        } else {
          log.info("app", "startup setting asks for a fresh page");
        }
      } catch (error) {
        // A missing or unreadable file is not a failure: the defaults are always usable.
        log.warn("app", `boot could not read the files: ${String(error)}`);
      }
      if (restored) {
        setPages(restored.pages);
        setActivePageId(restored.activePageId);
        setFocusByPage(restored.focusByPage);
        setStartCwd(restored.cwdByPane);
        // Page numbers keep counting from the highest one that came back.
        titleSeq.current = restored.pages.reduce((max, page) => Math.max(max, pageNumber(page.title)), 1);
      }
      setBooted(true);
    })();
  }, []);

  // Every settings change goes straight back to the file — there is no save button. The
  // first write also creates the file, which is how the defaults land on disk on a fresh run.
  useEffect(() => {
    if (!booted) return;
    const json = JSON.stringify(settings);
    if (json === writtenSettings.current) return;
    writtenSettings.current = json;
    void invoke("settings_save", { settings }).catch(error => {
      log.error("store", `settings_save failed: ${String(error)}`);
    });
  }, [booted, settings]);

  /**
   * What the next start comes back to: the pages as they are now, the focus, and where every
   * live shell is sitting. The directories are read from the kernel at this moment rather
   * than taken from the pane head's poll, which can be seconds behind.
   */
  const writeSession = useRef<() => Promise<void>>(async () => {});
  writeSession.current = async () => {
    if (settings.startup !== "restore") return;
    const cwdByPane: Record<string, string> = {};
    for (const [paneId, status] of Object.entries(paneStatus)) {
      if (!status.running) continue;
      const dir = await invoke<string | null>("terminal_cwd", { id: status.id }).catch(() => null);
      if (dir) cwdByPane[paneId] = dir;
    }
    try {
      await invoke("session_save", { session: { pages, activePageId, focusByPage, cwdByPane } });
      log.info("session", `saved ${pages.length} page(s)`);
    } catch (error) {
      log.error("session", `session_save failed: ${String(error)}`);
    }
  };

  // The session is written on the way out and nowhere else: the layout only ever matters at
  // the moment a start would come back to it, so there is nothing to keep in step until then.
  // The close is held back for it — losing the write would cost the whole working layout.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void getCurrentWindow()
      .onCloseRequested(async event => {
        event.preventDefault();
        try {
          await writeSession.current();
        } catch (error) {
          log.error("session", `session write on close failed: ${String(error)}`);
        } finally {
          log.info("app", "closing");
          // The window goes in a moment, so what is still queued leaves first.
          await log.flush();
          await getCurrentWindow().destroy();
        }
      })
      .then(off => {
        unlisten = off;
      });
    return () => unlisten?.();
  }, []);

  const activePage = pages.find(page => page.id === activePageId) ?? pages[0];
  /** The settings page, when it is open at all; there is never more than one. */
  const settingsPage = pages.find(page => page.kind === "settings");
  // Only a workspace has boxes to reason about; the settings page has no panes at all.
  const activeTree = activePage.kind === "workspace" ? activePage.tree : null;
  const activeIds = useMemo(() => (activeTree ? leafIds(activeTree) : []), [activeTree]);
  const columns = useMemo(() => (activeTree ? columnCount(activeTree) : 0), [activeTree]);
  const boxes = useMemo(() => (activeTree ? paneBoxes(activeTree) : new Map<string, PaneBox>()), [activeTree]);
  // Focus follows clicks, new panes and keyboard moves; the first pane is the fallback.
  const focusedPane = focusByPage[activePage.id] ?? null;
  const activePaneId = focusedPane && activeIds.includes(focusedPane) ? focusedPane : (activeIds[0] ?? null);

  useEffect(() => {
    const subscription = listen<SysStats>("sys-stats", event => setStats(event.payload));
    return () => {
      void subscription.then(unlisten => unlisten());
    };
  }, []);

  const addPage = () => {
    const page: Page = {
      id: newPageId(),
      title: `页 ${titleSeq.current + 1}`,
      kind: "workspace",
      tree: createTermPane(),
    };
    titleSeq.current += 1;
    setPages(list => [...list, page]);
    setActivePageId(page.id);
  };

  /** The settings page writes through here; the app is the only owner of the values. */
  const updateSettings = (patch: Partial<AppSettings>) => setSettings(current => ({ ...current, ...patch }));

  /**
   * The settings page is a page, so opening it makes one — but only ever one: asking again
   * just switches to the one that is already there.
   */
  const openSettings = () => {
    if (settingsPage) {
      setActivePageId(settingsPage.id);
      return;
    }
    const page: Page = { id: newPageId(), title: "系统设置", kind: "settings" };
    setPages(list => [...list, page]);
    setActivePageId(page.id);
  };

  const closePage = (pageId: string) => {
    if (pages.length <= 1) return;
    const remaining = pages.filter(page => page.id !== pageId);
    setPages(remaining);
    if (activePage.id === pageId) setActivePageId(remaining[0].id);
  };

  const switchPage = (delta: number) => {
    if (pages.length <= 1) return;
    const index = pages.findIndex(page => page.id === activePage.id);
    const next = (index + delta + pages.length) % pages.length;
    setActivePageId(pages[next].id);
  };

  const split = (pageId: string, paneId: string, dir: SplitDir, type: PaneType) => {
    const fresh = createPane(type);
    setPages(list =>
      list.map(page =>
        page.id === pageId && page.kind === "workspace"
          ? { ...page, tree: splitPane(page.tree, paneId, dir, fresh) }
          : page,
      ),
    );
    setFocusByPage(current => ({ ...current, [pageId]: fresh.id }));
  };

  /**
   * Ctrl+Shift+Q: take the focused pane out. Its sibling takes the split's place, and
   * the shell dies with the pane (the component unmounts and closes the session).
   */
  const closePane = (pageId: string, paneId: string) => {
    setPages(list =>
      list.map(page => {
        if (page.id !== pageId || page.kind !== "workspace") return page;
        const result = removePane(page.tree, paneId);
        return result.removed ? { ...page, tree: result.tree } : page;
      }),
    );
    // Focus falls to whatever the page's first pane is after the removal.
    setFocusByPage(current => {
      const kept = { ...current };
      delete kept[paneId];
      return kept;
    });
  };

  /**
   * Ctrl+Shift+arrow pressed: open the type picker at the spot where the new half will
   * land — to the right of the pane for a row split, below it for a column split.
   */
  const startSplit = (pageId: string, paneId: string, dir: SplitDir) => {
    const box = boxes.get(paneId);
    if (!box) return;
    const area = document.querySelector(".relative.min-h-0.flex-1")?.getBoundingClientRect();
    if (!area) return;
    // Percentages are of the pane area; the menu sits inside the new half, not on it.
    const x = area.left + (area.width * (box.left + box.width * (dir === "row" ? 0.75 : 0.5))) / 100;
    const y = area.top + (area.height * (box.top + box.height * (dir === "row" ? 0.5 : 0.75))) / 100;
    setSplitMenu({ pageId, paneId, dir, x, y });
  };

  /**
   * A keyboard split was confirmed: same path as the context menu's split.
   */
  const confirmSplit = (state: SplitMenuState, type: PaneType) => {
    split(state.pageId, state.paneId, state.dir, type);
    setSplitMenu(null);
  };

  const [zoomByPane, setZoomByPane] = useState<Record<string, number>>({});

  /** Effective terminal font size of the focused pane: settings base × pane zoom. */
  const focusedFontSize = (activePaneId ? (zoomByPane[activePaneId] ?? 1) : 1) * settings.fontSize;

  /**
   * Ctrl+= / Ctrl+- : zoom the focused terminal pane; Ctrl+0: back to the base size.
   * Zoom is per pane and dies with it. The step is a multiplication so repeated keys
   * keep feeling even, and both ends are hard stops.
   */
  const zoomPane = (paneId: string, factor: number) => {
    setZoomByPane(current => {
      const next = Math.min(Math.max((current[paneId] ?? 1) * factor, 0.5), 2);
      return next === 1 ? { ...current, [paneId]: 1 } : { ...current, [paneId]: next };
    });
  };

  /** A divider was dragged: rewrite that one ratio. Panes keep their identity. */
  const resizeSplit = (pageId: string, path: BranchPath, ratio: number) => {
    setPages(list =>
      list.map(page => {
        if (page.id !== pageId || page.kind !== "workspace") return page;
        const tree = setSplitRatio(page.tree, path, ratio);
        return tree === page.tree ? page : { ...page, tree };
      }),
    );
  };

  const reportPaneStatus = (paneId: string, status: TerminalStatus) => {
    setPaneStatus(current => ({ ...current, [paneId]: status }));
  };

  /** Panes that are not terminals say what they are in their own words. */
  const reportPaneLabel = (paneId: string, label: string) => {
    setPaneLabel(current => (current[paneId] === label ? current : { ...current, [paneId]: label }));
  };

  /** Alt+arrows: hand the focus to the pane next to the current one. */
  const movePaneFocus = (dir: Direction) => {
    if (!activePaneId) return;
    const next = neighborInDirection(boxes, activePaneId, dir);
    if (next) setFocusByPage(current => ({ ...current, [activePage.id]: next }));
  };

  // Keyboard: the design has no page tabs, so pages live on the keyboard. The
  // capture phase matters — the focused terminal would otherwise swallow the combo.
  // Only the combinations claimed here are taken; everything else (Ctrl+C, Ctrl+D,
  // Alt+<letter>, ...) stays with the shell.
  const actions = useRef({
    addPage,
    closePage,
    switchPage,
    movePaneFocus,
    closePane,
    startSplit,
    zoomPane,
    activePageId: activePage.id,
    activePaneId,
  });
  actions.current = {
    addPage,
    closePage,
    switchPage,
    movePaneFocus,
    closePane,
    startSplit,
    zoomPane,
    activePageId: activePage.id,
    activePaneId,
  };
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const key = event.key.toLowerCase();
      const handle = (run: () => void) => {
        event.preventDefault();
        event.stopPropagation();
        run();
      };
      // Alt+arrows: focus moves between the panes of the current page.
      // Alt+PageUp/PageDown: the page before / after this one.
      if (event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey) {
        const dir = ARROW_DIRECTION[key];
        if (dir) handle(() => actions.current.movePaneFocus(dir));
        else if (key === "pageup") handle(() => actions.current.switchPage(-1));
        else if (key === "pagedown") handle(() => actions.current.switchPage(1));
        return;
      }
      if (!event.ctrlKey || event.altKey || event.metaKey) return;
      // Ctrl+= / Ctrl+- / Ctrl+0: zoom the focused terminal pane. The physical plus key
      // sends "=", so both spellings are taken.
      if (key === "=" || key === "+") {
        const paneId = actions.current.activePaneId;
        if (paneId) handle(() => actions.current.zoomPane(paneId, 1.1));
        return;
      }
      if (key === "-") {
        const paneId = actions.current.activePaneId;
        if (paneId) handle(() => actions.current.zoomPane(paneId, 1 / 1.1));
        return;
      }
      if (key === "0") {
        const paneId = actions.current.activePaneId;
        if (paneId) handle(() => actions.current.zoomPane(paneId, 0));
        return;
      }
      if (!event.shiftKey) return;
      // Ctrl+Shift+arrows: split the focused pane. The menu lands where the new half
      // will appear — to the right / below the divider.
      const dir = ARROW_DIRECTION[key];
      if (dir === "right" || dir === "down") {
        const { activePageId: pageId, activePaneId: paneId, startSplit: begin } = actions.current;
        if (pageId && paneId) begin(pageId, paneId, dir === "right" ? "row" : "col");
        return;
      }
      if (key === "q") {
        const { activePageId: pageId, activePaneId: paneId } = actions.current;
        if (pageId && paneId) handle(() => actions.current.closePane(pageId, paneId));
      } else if (key === "t") handle(() => actions.current.addPage());
      else if (key === "w") handle(() => actions.current.closePage(actions.current.activePageId));
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, []);

  // Top bar: one cell per page, showing what that page's focused pane is.
  const pageCells: PageCell[] = pages.map(page => {
    if (page.kind === "settings") {
      return { id: page.id, title: page.title, geometry: null, active: page.id === activePage.id };
    }
    const ids = leafIds(page.tree);
    const pageFocus = focusByPage[page.id] ?? null;
    const paneId = pageFocus && ids.includes(pageFocus) ? pageFocus : (ids[0] ?? null);
    const status = paneId ? paneStatus[paneId] : undefined;
    // A terminal reports user@host: cwd; anything else reports its own label.
    const label = paneId ? paneLabel[paneId] : undefined;
    return {
      id: page.id,
      title: status?.running ? `${status.user}@${status.host}: ${status.cwd ?? "…"}` : (label ?? "…"),
      geometry: status?.cols ? `${status.cols}×${status.rows}` : null,
      active: page.id === activePage.id,
    };
  });

  const menuItems: MenuItem[] = menu
    ? [
        {
          label: "水平分裂",
          items: [
            { label: "终端", onSelect: () => split(menu.pageId, menu.paneId, "row", "term") },
            { label: "串口", onSelect: () => split(menu.pageId, menu.paneId, "row", "serial") },
            { label: "日志", onSelect: () => split(menu.pageId, menu.paneId, "row", "log") },
          ],
        },
        {
          label: "垂直分裂",
          items: [
            { label: "终端", onSelect: () => split(menu.pageId, menu.paneId, "col", "term") },
            { label: "串口", onSelect: () => split(menu.pageId, menu.paneId, "col", "serial") },
            { label: "日志", onSelect: () => split(menu.pageId, menu.paneId, "col", "log") },
          ],
        },
        // The entry never changes shape: picking it again lands on the settings page instead
        // of making a second one.
        { label: "系统设置", onSelect: openSettings },
      ]
    : [];

  // Nothing is drawn until the files have been read, so the window never flashes the default
  // flavor on its way to the saved one, and a restored layout never mounts a throwaway page.
  if (!booted) return <div className="h-full w-full bg-crust" />;

  return (
    <div className="flex h-screen flex-col bg-crust">
      <TopBar cells={pageCells} onSelectPage={setActivePageId} onClosePage={closePage} />

      {/* pane area: grows to fill whatever the status bar leaves */}
      <div className="relative min-h-0 flex-1 p-1">
        {pages.map(page => {
          // The settings page has no panes: it draws itself and is otherwise untouched.
          if (page.kind === "settings") {
            return (
              <div
                key={page.id}
                className="absolute inset-0 flex p-16"
                style={{ display: page.id === activePage.id ? "block" : "none" }}
              >
                <SettingsPage settings={settings} onChange={updateSettings} fonts={fonts} />
              </div>
            );
          }
          const ids = leafIds(page.tree);
          const pageFocus = focusByPage[page.id] ?? null;
          const pageActiveId = pageFocus && ids.includes(pageFocus) ? pageFocus : (ids[0] ?? null);
          return (
            <div
              key={page.id}
              className="absolute inset-0"
              style={{ display: page.id === activePage.id ? "block" : "none" }}
            >
              <Layout
                tree={page.tree}
                activeId={pageActiveId}
                onFocusPane={paneId => setFocusByPage(current => ({ ...current, [page.id]: paneId }))}
                onPaneContextMenu={(event: MouseEvent, paneId: string) =>
                  setMenu({ x: event.clientX, y: event.clientY, pageId: page.id, paneId })
                }
                onResizeSplit={(path, ratio) => resizeSplit(page.id, path, ratio)}
                renderPane={(pane, isFocused) => {
                  // A terminal reports user@host: cwd; the other kinds say what they are.
                  if (pane.type === "serial") {
                    return <SerialPane paneId={pane.id} focused={isFocused} onLabel={reportPaneLabel} />;
                  }
                  if (pane.type === "log") {
                    return <LogPane paneId={pane.id} focused={isFocused} onLabel={reportPaneLabel} />;
                  }
                  return (
                    <TermPane
                      paneId={pane.id}
                      focused={isFocused}
                      fontSize={settings.fontSize * (zoomByPane[pane.id] ?? 1)}
                      fontFamily={settings.fontFamily}
                      theme={settings.theme}
                      cwd={startCwd[pane.id]}
                      onStatus={reportPaneStatus}
                    />
                  );
                }}
              />
            </div>
          );
        })}
      </div>

      <StatusBar
        stats={stats}
        columns={columns}
        panes={activeIds.length}
        fontSizePercent={Math.round((focusedFontSize / BASE_FONT_SIZE) * 100)}
      />
      {menu ? <ContextMenu x={menu.x} y={menu.y} items={menuItems} onClose={() => setMenu(null)} /> : null}
      {splitMenu ? (
        <PaneTypeMenu
          x={splitMenu.x}
          y={splitMenu.y}
          onPick={type => confirmSplit(splitMenu, type)}
          onCancel={() => setSplitMenu(null)}
        />
      ) : null}
    </div>
  );
}

export default App;
