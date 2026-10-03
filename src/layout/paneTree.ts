/**
 * @file paneTree.ts
 * @brief Pane layout tree: the model behind splitting panes
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-01
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 *
 * A node is either a leaf (one pane) or a split of two nodes. Binary splits are
 * enough for any layout: three panes are ((A|B)|C). Ratios are fractions of the
 * parent box, so the layout survives window resizing without any JS.
 */

/** Pane kinds: terminals and serial ports. */
export type PaneType = "term" | "serial";

/** "row" splits left|right, "col" splits top/bottom. */
export type SplitDir = "row" | "col";

/** A single pane. Its id doubles as the terminal session id. */
export interface PaneLeaf {
  kind: "pane";
  id: string;
  type: PaneType;
}

/** Two nodes side by side, with `ratio` of the box going to `first`. */
export interface SplitNode {
  kind: "split";
  dir: SplitDir;
  ratio: number;
  first: PaneNode;
  second: PaneNode;
}

export type PaneNode = PaneLeaf | SplitNode;

/** Unique pane id; also the id the terminal session is registered under. */
export function newPaneId(): string {
  return `pane-${Math.random().toString(36).slice(2, 8)}`;
}

/** A fresh terminal pane. */
export function createTermPane(): PaneLeaf {
  return { kind: "pane", id: newPaneId(), type: "term" };
}

/** A fresh serial port pane. */
export function createSerialPane(): PaneLeaf {
  return { kind: "pane", id: newPaneId(), type: "serial" };
}

/** A fresh pane of the given kind. */
export function createPane(type: PaneType): PaneLeaf {
  return type === "serial" ? createSerialPane() : createTermPane();
}

/** All panes of a tree, left to right / top to bottom. */
export function collectPanes(node: PaneNode, out: PaneLeaf[] = []): PaneLeaf[] {
  if (node.kind === "pane") {
    out.push(node);
    return out;
  }
  collectPanes(node.first, out);
  collectPanes(node.second, out);
  return out;
}

/** All pane ids of a tree. */
export function leafIds(node: PaneNode): string[] {
  return collectPanes(node).map(pane => pane.id);
}

/**
 * How many columns (side by side groups) the layout has: the grid width of the tree.
 * A row split adds the widths of its halves, a column split takes the wider of them.
 * The status bar reports this number.
 */
export function columnCount(node: PaneNode): number {
  if (node.kind === "pane") return 1;
  if (node.dir === "row") return columnCount(node.first) + columnCount(node.second);
  return Math.max(columnCount(node.first), columnCount(node.second));
}

/**
 * Split the pane with `targetId` in two, keeping the old pane as `first` and
 * putting `fresh` next to it. Unknown ids are left untouched.
 */
export function splitPane(node: PaneNode, targetId: string, dir: SplitDir, fresh: PaneLeaf): PaneNode {
  if (node.kind === "pane") {
    if (node.id !== targetId) return node;
    return { kind: "split", dir, ratio: 0.5, first: node, second: fresh };
  }
  return {
    ...node,
    first: splitPane(node.first, targetId, dir, fresh),
    second: splitPane(node.second, targetId, dir, fresh),
  };
}
