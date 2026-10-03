/**
 * @file Layout.tsx
 * @brief Pane layout: turns the pane tree into absolutely positioned boxes and dividers
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-01
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 *
 * Every pane lives in one flat container and gets a box computed from the tree
 * (see paneRects.ts). That matters: with nested containers, splitting would change
 * the DOM depth and React would remount the pane — which disposes the terminal and
 * kills the shell. With flat boxes, a split only changes numbers.
 *
 * The dividers are built the same way: one flat bar per split node, so dragging one
 * only rewrites that node's ratio. The bars are positioned against the same container
 * the boxes are, which is why the drag can measure it and never has to guess.
 */
import { useMemo, useState, type MouseEvent, type ReactNode } from "react";
import { collectPanes, type BranchPath, type PaneLeaf, type PaneNode } from "./paneTree";
import { paneBoxes, splitBoxes, type SplitBox } from "./paneRects";

/**
 * Smallest box a divider may leave behind, in px. A floor in pixels, not in ratio:
 * a tiny pane is unusable no matter how much of the window it is.
 */
const MIN_PANE_W = 180;
const MIN_PANE_H = 90;
/** Hit area of a bar: a little narrower than the 8px gap the pane boxes leave open. */
const GRAB_PX = 5;

export interface LayoutProps {
  tree: PaneNode;
  /** Pane that currently holds the focus, i.e. the one drawn as active */
  activeId: string | null;
  onFocusPane: (id: string) => void;
  onPaneContextMenu: (event: MouseEvent, id: string) => void;
  /** A bar was dragged: the ratio the split at `path` should take. */
  onResizeSplit: (path: BranchPath, ratio: number) => void;
  /** Draws the content of one pane; the box and the pane chrome come from here */
  renderPane: (pane: PaneLeaf, focused: boolean) => ReactNode;
}

/** Stable name for a branch path, so React can tell the bars apart. */
function pathKey(path: BranchPath): string {
  return path.length ? path.join(".") : "root";
}

function Layout({ tree, activeId, onFocusPane, onPaneContextMenu, onResizeSplit, renderPane }: LayoutProps) {
  const panes = useMemo(() => collectPanes(tree), [tree]);
  const boxes = useMemo(() => paneBoxes(tree), [tree]);
  const splits = useMemo(() => splitBoxes(tree), [tree]);
  const [dragging, setDragging] = useState<string | null>(null);

  /**
   * Grab a bar and follow the mouse until it is let go. A split node's own box and
   * edges cannot move while one of its descendants is dragged, so the geometry is
   * measured once here instead of on every move. The parent element is the container
   * the boxes are percentages of — measuring it is what keeps the two in step.
   */
  const grab = (event: MouseEvent, split: SplitBox) => {
    event.preventDefault();
    const host = event.currentTarget.parentElement;
    if (!host) return;
    const rect = host.getBoundingClientRect();
    const horizontal = split.dir === "row";
    // The split node's own box, in px: the frame the ratio is measured against.
    const span = horizontal ? (rect.width * split.width) / 100 : (rect.height * split.height) / 100;
    const origin = horizontal
      ? rect.left + (rect.width * split.left) / 100
      : rect.top + (rect.height * split.top) / 100;
    if (span <= 0) return;
    // Both halves get the same floor, so a box too small to hold two panes simply
    // pins the bar in the middle instead of collapsing one side.
    const floor = Math.min((horizontal ? MIN_PANE_W : MIN_PANE_H) / span, 0.5);

    const onMove = (move: globalThis.MouseEvent) => {
      const at = (horizontal ? move.clientX : move.clientY) - origin;
      const ratio = Math.min(Math.max(at / span, floor), 1 - floor);
      onResizeSplit(split.path, ratio);
    };
    const release = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", release);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      setDragging(null);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", release);
    // The pointer leaves the bar as soon as it moves, so hold the cursor and keep
    // the drag from selecting text across the panes.
    document.body.style.cursor = horizontal ? "col-resize" : "row-resize";
    document.body.style.userSelect = "none";
    setDragging(pathKey(split.path));
  };

  const barStyle = (split: SplitBox) =>
    split.dir === "row"
      ? {
          left: `${split.line}%`,
          top: `${split.top}%`,
          width: `${GRAB_PX}px`,
          height: `${split.height}%`,
          transform: "translateX(-50%)",
        }
      : {
          top: `${split.line}%`,
          left: `${split.left}%`,
          height: `${GRAB_PX}px`,
          width: `${split.width}%`,
          transform: "translateY(-50%)",
        };

  return (
    <>
      {panes.map(pane => {
        const box = boxes.get(pane.id);
        if (!box) return null;
        const focused = pane.id === activeId;
        return (
          <div
            key={pane.id}
            className="absolute p-1"
            style={{
              left: `${box.left}%`,
              top: `${box.top}%`,
              width: `${box.width}%`,
              height: `${box.height}%`,
            }}
            onMouseDown={() => onFocusPane(pane.id)}
            onContextMenu={event => {
              event.preventDefault();
              onPaneContextMenu(event, pane.id);
            }}
          >
            {renderPane(pane, focused)}
          </div>
        );
      })}

      {splits.map(split => {
        const horizontal = split.dir === "row";
        const key = pathKey(split.path);
        return (
          <div
            key={`divider-${key}`}
            className={`group absolute z-10 ${horizontal ? "cursor-col-resize" : "cursor-row-resize"}`}
            style={barStyle(split)}
            onMouseDown={event => grab(event, split)}
          >
            {/* The grab area is invisible and wider than the hairline it draws. */}
            <div
              className={`${horizontal ? "mx-auto h-full w-px" : "my-auto h-px w-full"} ${
                dragging === key ? "bg-blue" : "group-hover:bg-blue/50"
              }`}
            />
          </div>
        );
      })}
    </>
  );
}

export default Layout;
