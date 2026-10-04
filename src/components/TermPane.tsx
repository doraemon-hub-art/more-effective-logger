/**
 * @file TermPane.tsx
 * @brief Terminal pane: pane chrome (frame + head) around a TerminalView
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-01
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 *
 * Sizing and positioning belong to the layout (Layout.tsx gives every pane a box);
 * this component only fills the box it is handed. The head follows the reference
 * terminal: `user@host: path` with the geometry at the right end, and the active
 * pane is the one with the blue bar.
 */
import { useCallback, useState } from "react";
import TerminalView, { type TerminalStatus } from "./TerminalView";
import type { ThemeName } from "../theme";

export interface TermPaneProps {
  /** Pane id, also used as the terminal session id */
  paneId: string;
  /** Active pane: blue head */
  focused?: boolean;
  /** Font size of the terminal screen, in px (settings base × pane zoom) */
  fontSize?: number;
  /** Monospace family the terminal screen renders in (from settings) */
  fontFamily?: string;
  /** Catppuccin flavor of the terminal screen (from settings) */
  theme?: ThemeName;
  /** Directory the shell starts in; set for a pane restored from the last session */
  cwd?: string;
  /** Reporting upwards: the page needs this for the top bar */
  onStatus?: (paneId: string, status: TerminalStatus) => void;
}

function TermPane({ paneId, focused = false, fontSize = 12.5, fontFamily, theme, cwd, onStatus }: TermPaneProps) {
  const [status, setStatus] = useState<TerminalStatus | null>(null);
  const failed = status?.failure != null;

  const handleStatus = useCallback(
    (next: TerminalStatus) => {
      setStatus(next);
      onStatus?.(paneId, next);
    },
    [paneId, onStatus],
  );

  return (
    <div className="flex h-full min-h-0 flex-col overflow-hidden rounded-md border border-surface0 bg-base">
      {/* pane head: user@host: path on the left, geometry on the right */}
      <div
        className={`flex h-[26px] flex-none items-center gap-2 border-b border-surface0 px-2 font-mono text-[11.5px] ${
          focused ? "bg-blue text-base" : "bg-mantle text-subtext0"
        }`}
      >
        <span
          className={`flex-none rounded-sm px-[5px] py-px font-mono text-[10px] tracking-wide ${
            focused ? "bg-base/25 text-base" : "bg-blue/15 text-blue"
          }`}
        >
          TERM
        </span>
        <span className="min-w-0 truncate">
          {status?.running ? `${status.user}@${status.host}: ${status.cwd ?? "…"}` : "…"}
        </span>
        <span className="flex-1" />
        {failed && <span className="flex-none text-[10.5px]">启动失败</span>}
        <span className="flex-none text-[10.5px] opacity-75">
          {status?.cols ? `${status.cols}×${status.rows}` : "—"}
        </span>
      </div>

      {/* pane body: the terminal widget fills it */}
      <div className="min-h-0 flex-1 overflow-hidden bg-base p-2">
        <TerminalView
          id={paneId}
          focused={focused}
          fontSize={fontSize}
          fontFamily={fontFamily}
          theme={theme}
          cwd={cwd}
          onStatus={handleStatus}
        />
      </div>
    </div>
  );
}

export default TermPane;
