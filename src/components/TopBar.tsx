/**
 * @file TopBar.tsx
 * @brief Top bar: one equal cell per page (the page strip)
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-01
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 *
 * One cell per page, split evenly, no page numbering: a cell shows what the page's
 * focused pane is, in the shape the reference terminal uses — `user@host: path` with
 * the pane geometry at the right end. The active page's cell is the blue bar.
 * Clicking a cell switches to that page (the keyboard does the same); the ✕ at its end
 * closes that page, so every page carries its own way out.
 */
export interface PageCell {
  id: string;
  /** `user@host: ~/path` of that page's focused pane */
  title: string;
  /** Pane geometry, e.g. `94×39`; null when the page has none to report */
  geometry: string | null;
  active: boolean;
}

export interface TopBarProps {
  cells: PageCell[];
  onSelectPage: (id: string) => void;
  /** Close one page; the app turns it down when it is the last one. */
  onClosePage: (id: string) => void;
}

function TopBar({ cells, onSelectPage, onClosePage }: TopBarProps) {
  // The last page has no way out: closing it would leave the window with nothing in it.
  const closeable = cells.length > 1;
  return (
    <div
      className="grid h-[34px] flex-none border-b border-surface0 bg-crust"
      style={{ gridTemplateColumns: `repeat(${Math.max(cells.length, 1)}, minmax(0, 1fr))` }}
    >
      {cells.map((cell, index) => (
        <div
          key={cell.id}
          onClick={() => onSelectPage(cell.id)}
          className={`flex min-w-0 cursor-pointer items-center gap-3 px-3 font-mono text-[11.5px] ${
            cell.active ? "bg-blue text-base" : "text-overlay0 hover:text-subtext0"
          } ${index < cells.length - 1 ? "border-r border-r-surface0/45" : ""}`}
        >
          <span className="min-w-0 flex-1 truncate text-left">{cell.title}</span>
          {/* Nothing to report means nothing drawn: a placeholder would read as clutter. */}
          {cell.geometry ? <span className="flex-none text-[10.5px] opacity-75">{cell.geometry}</span> : null}
          {closeable ? (
            <button
              type="button"
              title="关闭页面"
              onClick={event => {
                // The ✕ closes this page; it must not also switch to it.
                event.stopPropagation();
                onClosePage(cell.id);
              }}
              className={`flex h-[20px] w-[20px] flex-none items-center justify-center rounded-sm text-[12px] ${
                cell.active
                  ? "text-base/60 hover:bg-base/25 hover:text-base"
                  : "text-overlay0 hover:bg-surface0 hover:text-red"
              }`}
            >
              ✕
            </button>
          ) : null}
        </div>
      ))}
    </div>
  );
}

export default TopBar;
