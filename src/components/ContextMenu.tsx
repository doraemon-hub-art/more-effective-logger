/**
 * @file ContextMenu.tsx
 * @brief Small context menu with one level of submenus
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-01
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 *
 * Items without `items` act on click; items with `items` open a submenu on hover,
 * which is how "split this pane -> what kind of pane" is presented.
 */
import { useEffect, useRef, useState } from "react";

/** One menu entry. Entries carrying `items` open a submenu instead of acting. */
export interface MenuItem {
  label: string;
  items?: MenuItem[];
  onSelect?: () => void;
}

export interface ContextMenuProps {
  /** Where the menu was summoned, in viewport coordinates */
  x: number;
  y: number;
  items: MenuItem[];
  /** Called after an entry was chosen, and on Escape or a click outside */
  onClose: () => void;
}

const MENU_WIDTH = 172;
const SUBMENU_WIDTH = 148;
const ITEM_HEIGHT = 27;
const MARGIN = 6;

function ContextMenu({ x, y, items, onClose }: ContextMenuProps) {
  const [openIndex, setOpenIndex] = useState<number | null>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    const onMouseDown = (event: MouseEvent) => {
      // The click that opened the menu happened before this listener existed, so
      // anything landing inside the menu afterwards is a real menu click.
      if (!rootRef.current?.contains(event.target as Node)) onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("mousedown", onMouseDown, true);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("mousedown", onMouseDown, true);
    };
  }, [onClose]);

  const left = Math.min(x, window.innerWidth - MENU_WIDTH - MARGIN);
  const top = Math.min(y, window.innerHeight - items.length * ITEM_HEIGHT - MARGIN);
  // A submenu must fit too; flip it to the left of the menu when it would not.
  const submenuRight = left + MENU_WIDTH + SUBMENU_WIDTH + MARGIN <= window.innerWidth;

  return (
    <div
      ref={rootRef}
      className="fixed z-50 rounded-md border border-surface0 bg-mantle py-1 font-mono text-[11.5px] text-subtext0 shadow-[0_6px_20px_rgba(0,0,0,0.45)]"
      style={{ left, top, width: MENU_WIDTH }}
    >
      {items.map((item, index) => (
        <div key={item.label} className="relative" onMouseEnter={() => setOpenIndex(item.items ? index : null)}>
          <button
            type="button"
            className="flex w-full items-center justify-between gap-4 px-2.5 py-[5px] text-left hover:bg-surface0"
            onClick={() => {
              if (item.items) return;
              item.onSelect?.();
              onClose();
            }}
          >
            <span>{item.label}</span>
            {item.items ? <span className="text-overlay0">▸</span> : null}
          </button>

          {item.items && openIndex === index ? (
            <div
              className="absolute -top-1 z-50 rounded-md border border-surface0 bg-mantle py-1 shadow-[0_6px_20px_rgba(0,0,0,0.45)]"
              style={
                submenuRight
                  ? { left: MENU_WIDTH - 2, width: SUBMENU_WIDTH }
                  : { right: MENU_WIDTH - 2, width: SUBMENU_WIDTH }
              }
            >
              {item.items.map(sub => (
                <button
                  key={sub.label}
                  type="button"
                  className="flex w-full items-center px-2.5 py-[5px] text-left hover:bg-surface0"
                  onClick={() => {
                    sub.onSelect?.();
                    onClose();
                  }}
                >
                  {sub.label}
                </button>
              ))}
            </div>
          ) : null}
        </div>
      ))}
    </div>
  );
}

export default ContextMenu;
