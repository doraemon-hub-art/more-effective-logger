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
import { listen } from "@tauri-apps/api/event";
import TermPane from "./components/TermPane";
import SerialPane from "./components/SerialPane";
import { type TerminalStatus } from "./components/TerminalView";
import StatusBar, { type SysStats } from "./components/StatusBar";
import ContextMenu, { type MenuItem } from "./components/ContextMenu";
import TopBar, { type PageCell } from "./components/TopBar";
import Layout from "./layout/Layout";
import {
  columnCount,
  createPane,
  createTermPane,
  leafIds,
  splitPane,
  type PaneNode,
  type PaneType,
  type SplitDir,
} from "./layout/paneTree";
import { neighborInDirection, paneBoxes, type Direction } from "./layout/paneRects";

/** One workspace: a pane tree plus a title. */
interface Page {
  id: string;
  title: string;
  tree: PaneNode;
}

/** Where a context menu is open, and which pane of which page it belongs to. */
interface MenuState {
  x: number;
  y: number;
  pageId: string;
  paneId: string;
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

function App() {
  const [stats, setStats] = useState<SysStats | null>(null);
  const [pages, setPages] = useState<Page[]>(() => [{ id: newPageId(), title: "页 1", tree: createTermPane() }]);
  const [activePageId, setActivePageId] = useState<string | null>(null);
  const [focusByPage, setFocusByPage] = useState<Record<string, string | null>>({});
  const [paneStatus, setPaneStatus] = useState<Record<string, TerminalStatus>>({});
  /** Non-terminal panes describe themselves (serial: device + rate). */
  const [paneLabel, setPaneLabel] = useState<Record<string, string>>({});
  const [menu, setMenu] = useState<MenuState | null>(null);
  const titleSeq = useRef(1);

  const activePage = pages.find(page => page.id === activePageId) ?? pages[0];
  const activeIds = useMemo(() => leafIds(activePage.tree), [activePage.tree]);
  const columns = useMemo(() => columnCount(activePage.tree), [activePage.tree]);
  const boxes = useMemo(() => paneBoxes(activePage.tree), [activePage.tree]);
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
      tree: createTermPane(),
    };
    titleSeq.current += 1;
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
      list.map(page => (page.id === pageId ? { ...page, tree: splitPane(page.tree, paneId, dir, fresh) } : page)),
    );
    setFocusByPage(current => ({ ...current, [pageId]: fresh.id }));
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
  const actions = useRef({ addPage, closePage, switchPage, movePaneFocus, activePageId: activePage.id });
  actions.current = { addPage, closePage, switchPage, movePaneFocus, activePageId: activePage.id };
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
      if (event.shiftKey && key === "t") handle(() => actions.current.addPage());
      else if (event.shiftKey && key === "w") handle(() => actions.current.closePage(actions.current.activePageId));
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, []);

  // Top bar: one cell per page, showing what that page's focused pane is.
  const pageCells: PageCell[] = pages.map(page => {
    const ids = leafIds(page.tree);
    const pageFocus = focusByPage[page.id] ?? null;
    const paneId = pageFocus && ids.includes(pageFocus) ? pageFocus : (ids[0] ?? null);
    const status = paneId ? paneStatus[paneId] : undefined;
    // A terminal reports user@host: cwd; anything else reports its own label.
    const label = paneId ? paneLabel[paneId] : undefined;
    return {
      id: page.id,
      title: status?.running ? `${status.user}@${status.host}: ${status.cwd ?? "…"}` : (label ?? "…"),
      geometry: status?.cols ? `${status.cols}×${status.rows}` : "—",
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
          ],
        },
        {
          label: "垂直分裂",
          items: [
            { label: "终端", onSelect: () => split(menu.pageId, menu.paneId, "col", "term") },
            { label: "串口", onSelect: () => split(menu.pageId, menu.paneId, "col", "serial") },
          ],
        },
      ]
    : [];

  return (
    <div className="flex h-screen flex-col bg-crust">
      <TopBar cells={pageCells} onSelectPage={setActivePageId} />

      {/* pane area: grows to fill whatever the status bar leaves */}
      <div className="relative min-h-0 flex-1 p-1">
        {pages.map(page => {
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
                renderPane={(pane, isFocused) =>
                  pane.type === "serial" ? (
                    <SerialPane paneId={pane.id} focused={isFocused} onLabel={reportPaneLabel} />
                  ) : (
                    <TermPane paneId={pane.id} focused={isFocused} onStatus={reportPaneStatus} />
                  )
                }
              />
            </div>
          );
        })}
      </div>

      <StatusBar stats={stats} columns={columns} panes={activeIds.length} />
      {menu ? <ContextMenu x={menu.x} y={menu.y} items={menuItems} onClose={() => setMenu(null)} /> : null}
    </div>
  );
}

export default App;
