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

/** Pane kinds: terminals, serial ports and logs. */
export type PaneType = "term" | "serial" | "log";

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

/** Where a split sits in the tree: one step per level, 0 = first half, 1 = second. */
export type BranchPath = number[];

/** Ratios stay off the ends, so neither half of a split can collapse to nothing. */
export const MIN_RATIO = 0.05;
export const MAX_RATIO = 0.95;

/** Keep a ratio usable; anything that is not a finite number falls back to an even split. */
export function clampRatio(ratio: number): number {
  if (!Number.isFinite(ratio)) return 0.5;
  return Math.min(Math.max(ratio, MIN_RATIO), MAX_RATIO);
}

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

/** A fresh log pane. */
export function createLogPane(): PaneLeaf {
  return { kind: "pane", id: newPaneId(), type: "log" };
}

/** A fresh pane of the given kind. */
export function createPane(type: PaneType): PaneLeaf {
  if (type === "serial") return createSerialPane();
  if (type === "log") return createLogPane();
  return createTermPane();
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

/**
 * Move the divider of the split at `path`: the root itself for [], otherwise one step
 * per level (0 = first half, 1 = second). Rebuilds the nodes along the path only, so
 * panes keep their identity — a box changes, nothing is remounted, no shell is killed.
 * Unknown paths, and a ratio that changes nothing, give the tree back untouched.
 */
export function setSplitRatio(node: PaneNode, path: BranchPath, ratio: number): PaneNode {
  if (node.kind === "pane") return node;
  if (path.length === 0) {
    const next = clampRatio(ratio);
    return next === node.ratio ? node : { ...node, ratio: next };
  }
  const [step, ...rest] = path;
  const child = step === 0 ? node.first : node.second;
  const moved = setSplitRatio(child, rest, ratio);
  if (moved === child) return node;
  return step === 0 ? { ...node, first: moved } : { ...node, second: moved };
}

/** Which half a removed leaf was: the survivor takes its parent's place. */
type RemoveResult = { tree: PaneNode; removed: boolean };

/**
 * Take the leaf with `targetId` out of the tree. Its sibling is promoted into the
 * parent's place — that is what makes a two-way split close back up instead of leaving
 * a hole. Unknown ids, and a lone root leaf, come back untouched.
 */
export function removePane(node: PaneNode, targetId: string): RemoveResult {
  if (node.kind === "pane") return { tree: node, removed: node.id === targetId };
  // Try the first half; if the leaf was there, the second half takes over whole.
  const fromFirst = removePane(node.first, targetId);
  if (fromFirst.removed) return { tree: node.second, removed: true };
  const fromSecond = removePane(node.second, targetId);
  if (fromSecond.removed) return { tree: node.first, removed: true };
  // The leaf lives deeper: rebuild only the branch that changed.
  if (fromFirst.tree !== node.first) return { tree: { ...node, first: fromFirst.tree }, removed: false };
  if (fromSecond.tree !== node.second) return { tree: { ...node, second: fromSecond.tree }, removed: false };
  return { tree: node, removed: false };
}
