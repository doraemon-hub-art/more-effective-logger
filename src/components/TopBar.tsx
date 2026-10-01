/**
 * @file TopBar.tsx
 * @brief Top bar: one equal cell per page (the page strip)
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-01
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 *
 * One cell per page, split evenly, no page numbering: a cell shows what the page's
 * focused pane is, in the shape the reference terminal uses — `user@host: path` with
 * the pane geometry at the right end. The active page's cell is the red bar.
 * Clicking a cell switches to that page (the keyboard does the same).
 */
export interface PageCell {
  id: string;
  /** `user@host: ~/path` of that page's focused pane */
  title: string;
  /** Pane geometry, e.g. `94×39` */
  geometry: string;
  active: boolean;
}

export interface TopBarProps {
  cells: PageCell[];
  onSelectPage: (id: string) => void;
}

function TopBar({ cells, onSelectPage }: TopBarProps) {
  return (
    <div
      className="grid h-[34px] flex-none border-b border-surface0 bg-crust"
      style={{ gridTemplateColumns: `repeat(${Math.max(cells.length, 1)}, minmax(0, 1fr))` }}
    >
      {cells.map((cell, index) => (
        <button
          key={cell.id}
          type="button"
          onClick={() => onSelectPage(cell.id)}
          className={`flex min-w-0 items-center gap-3 px-3 font-mono text-[11.5px] ${
            cell.active ? "bg-red text-base" : "text-overlay0 hover:text-subtext0"
          } ${index < cells.length - 1 ? "border-r border-r-surface0/45" : ""}`}
        >
          <span className="min-w-0 flex-1 truncate text-left">{cell.title}</span>
          <span className="flex-none text-[10.5px] opacity-75">{cell.geometry}</span>
        </button>
      ))}
    </div>
  );
}

export default TopBar;
