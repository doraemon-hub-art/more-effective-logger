/**
 * @file Layout.tsx
 * @brief Pane layout: turns the pane tree into absolutely positioned boxes
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-01
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 *
 * Every pane lives in one flat container and gets a box computed from the tree
 * (see paneRects.ts). That matters: with nested containers, splitting would change
 * the DOM depth and React would remount the pane — which disposes the terminal and
 * kills the shell. With flat boxes, a split only changes numbers.
 */
import { useMemo, type MouseEvent, type ReactNode } from "react";
import { collectPanes, type PaneLeaf, type PaneNode } from "./paneTree";
import { paneBoxes } from "./paneRects";

export interface LayoutProps {
  tree: PaneNode;
  /** Pane that currently holds the focus, i.e. the one drawn as active */
  activeId: string | null;
  onFocusPane: (id: string) => void;
  onPaneContextMenu: (event: MouseEvent, id: string) => void;
  /** Draws the content of one pane; the box and the pane chrome come from here */
  renderPane: (pane: PaneLeaf, focused: boolean) => ReactNode;
}

function Layout({ tree, activeId, onFocusPane, onPaneContextMenu, renderPane }: LayoutProps) {
  const panes = useMemo(() => collectPanes(tree), [tree]);
  const boxes = useMemo(() => paneBoxes(tree), [tree]);

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
    </>
  );
}

export default Layout;
