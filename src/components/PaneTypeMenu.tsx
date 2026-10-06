/**
 * @file PaneTypeMenu.tsx
 * @brief The keyboard split picker: pick what the new half of a split will be
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-03
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 *
 * Ctrl+Shift+arrows split the focused pane; the direction is already decided by the key,
 * so this menu only asks one question — what goes in the new half. It appears where the
 * split will land, the first entry is preselected, ↑/↓ move and Enter confirms, Esc
 * backs out without splitting.
 */
import { useEffect, useState } from "react";
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

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        onCancel();
        return;
      }
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        event.stopPropagation();
        setIndex(current => (current + CHOICES.length + (event.key === "ArrowDown" ? 1 : -1)) % CHOICES.length);
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
      className="fixed z-50 rounded-md border border-surface0 bg-mantle py-1 font-mono text-[11.5px] text-subtext0 shadow-[0_6px_20px_rgba(0,0,0,0.45)]"
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
