/**
 * @file PaneTypeMenu.tsx
 * @brief The keyboard split picker: pick what the new half of a split will be
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-03
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 *
 * Ctrl+Alt+j / Ctrl+Alt+l split the focused pane; the direction is already decided by the
 * key, so this menu only asks one question — what goes in the new half. It appears where
 * the split will land, the first entry is preselected, j/k move (vim style) and Enter
 * confirms, Esc backs out without splitting.
 */
import { useEffect, useRef, useState } from "react";
import type { PaneType } from "../layout/paneTree";

/** What the new half can be; the labels match the context menu's submenu. */
const CHOICES: Array<{ id: PaneType; label: string }> = [
  { id: "term", label: "终端" },
  { id: "serial", label: "串口" },
  { id: "log", label: "日志" },
];

export interface PaneTypeMenuProps {
  /** Where the split will land, in viewport coordinates */
  x: number;
  y: number;
  /** The type was picked: the app performs the split. */
  onPick: (type: PaneType) => void;
  /** Esc, or a click outside: no split happens. */
  onCancel: () => void;
}

function PaneTypeMenu({ x, y, onPick, onCancel }: PaneTypeMenuProps) {
  const [index, setIndex] = useState(0);
  const rootRef = useRef<HTMLDivElement | null>(null);

  // The picker takes the keyboard for itself. Left on the pane's terminal, a plain j/k
  // never reaches the page: the focused textarea hands it to the input method, which
  // turns it into composition input instead of a keydown. A plain box has no such path,
  // so the letters arrive as keys; whatever had the focus gets it back afterwards.
  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    rootRef.current?.focus();
    return () => {
      if (previous?.isConnected) previous.focus();
    };
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        onCancel();
        return;
      }
      // j / k: vim style up and down, wrapping at both ends.
      const key = event.key.toLowerCase();
      if (key === "j" || key === "k") {
        event.preventDefault();
        event.stopPropagation();
        setIndex(current => (current + CHOICES.length + (key === "j" ? 1 : -1)) % CHOICES.length);
        return;
      }
      if (event.key === "Enter") {
        event.preventDefault();
        event.stopPropagation();
        onPick(CHOICES[index].id);
      }
    };
    const onMouseDown = (event: globalThis.MouseEvent) => {
      // The click that triggered the split is gone by now, so anything outside is a
      // plain click that means "never mind".
      onCancel();
      void event;
    };
    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("mousedown", onMouseDown, true);
    return () => {
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("mousedown", onMouseDown, true);
    };
  }, [index, onPick, onCancel]);

  return (
    <div
      ref={rootRef}
      // Focusable but not in the tab order: this is where the keys land while it is open.
      tabIndex={-1}
      className="fixed z-50 rounded-md border border-surface0 bg-mantle py-1 font-mono text-[11.5px] text-subtext0 shadow-[0_6px_20px_rgba(0,0,0,0.45)] outline-none"
      style={{ left: x, top: y, width: 148 }}
    >
      {CHOICES.map((choice, choiceIndex) => (
        <div key={choice.id} onMouseEnter={() => setIndex(choiceIndex)}>
          <button
            type="button"
            className={`flex w-full items-center px-2.5 py-[5px] text-left ${
              choiceIndex === index ? "bg-surface0 text-fg" : "hover:text-fg"
            }`}
            onClick={() => onPick(choice.id)}
          >
            {choice.label}
          </button>
        </div>
      ))}
    </div>
  );
}

export default PaneTypeMenu;
