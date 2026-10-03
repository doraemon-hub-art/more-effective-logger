/**
 * @file paneRects.ts
 * @brief Layout math: pane tree -> one box per pane, in percentages
 * @author doraemon-hub-art <1660219734@qq.com>
 * @date 2026-10-01
 * @copyright Copyright (c) 2026 doraemon-hub-art. All rights reserved.
 *
 * Percentages on purpose: the boxes then follow the window size for free, and a
 * split is nothing but new numbers — no DOM rearranging, so panes never remount.
 */
import { clampRatio, type BranchPath, type PaneNode, type SplitDir } from "./paneTree";

/** Position and size of one pane, as percentages of the layout container. */
export interface PaneBox {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** A draggable boundary: which split it moves, and where its bar sits. */
export interface SplitBox {
  /** The split this bar belongs to: steps from the root, 0 = first half. */
  path: BranchPath;
  /** "row" splits left|right, so its bar is a vertical line. */
  dir: SplitDir;
  /** Box of the whole split node, in percent of the container. */
  left: number;
  top: number;
  width: number;
  height: number;
  /** The line inside that box: an x for a row split, a y for a column split. */
  line: number;
}

/** The whole container, as a box. */
const ROOT: PaneBox = { left: 0, top: 0, width: 100, height: 100 };

/** Walk the tree, halving the box until every leaf has one; collect the dividers too. */
function walk(node: PaneNode, box: PaneBox, boxes: Map<string, PaneBox>, splits: SplitBox[], path: BranchPath) {
  if (node.kind === "pane") {
    boxes.set(node.id, box);
    return;
  }
  const ratio = clampRatio(node.ratio);
  if (node.dir === "row") {
    const firstWidth = box.width * ratio;
    splits.push({ path, dir: node.dir, ...box, line: box.left + firstWidth });
    walk(node.first, { ...box, width: firstWidth }, boxes, splits, [...path, 0]);
    walk(node.second, { ...box, left: box.left + firstWidth, width: box.width - firstWidth }, boxes, splits, [
      ...path,
      1,
    ]);
  } else {
    const firstHeight = box.height * ratio;
    splits.push({ path, dir: node.dir, ...box, line: box.top + firstHeight });
    walk(node.first, { ...box, height: firstHeight }, boxes, splits, [...path, 0]);
    walk(node.second, { ...box, top: box.top + firstHeight, height: box.height - firstHeight }, boxes, splits, [
      ...path,
      1,
    ]);
  }
}

/** Boxes of every pane in the tree, keyed by pane id. */
export function paneBoxes(tree: PaneNode): Map<string, PaneBox> {
  const boxes = new Map<string, PaneBox>();
  walk(tree, ROOT, boxes, [], []);
  return boxes;
}

/** Every divider in the tree, outermost first. */
export function splitBoxes(tree: PaneNode): SplitBox[] {
  const splits: SplitBox[] = [];
  walk(tree, ROOT, new Map(), splits, []);
  return splits;
}

/** Where a focus move goes. */
export type Direction = "left" | "right" | "up" | "down";

/** Small enough to absorb rounding, big enough to ignore a shared edge. */
const EDGE = 0.01;

/**
 * The pane a focus move in `dir` lands on, or null when there is none.
 *
 * Geometry, not the tree: a neighbour is any pane on that side whose centre is
 * beyond the current one. Panes that share a band across the move direction win
 * over the rest, which is what makes "left" from a lower right pane pick the pane
 * beside it instead of the one above it. Ties keep the first candidate, i.e. the
 * earlier pane, so repeated moves stay predictable.
 */
export function neighborInDirection(boxes: Map<string, PaneBox>, fromId: string, dir: Direction): string | null {
  const from = boxes.get(fromId);
  if (!from) return null;
  const fromCx = from.left + from.width / 2;
  const fromCy = from.top + from.height / 2;
  const horizontal = dir === "left" || dir === "right";

  let best: string | null = null;
  let bestShares = false;
  let bestScore = Infinity;

  for (const [id, box] of boxes) {
    if (id === fromId) continue;

    // Gap to the candidate along the move axis, signed: negative means it is on
    // the wrong side (or only overlapping), so it cannot be the neighbour.
    const gap =
      dir === "left"
        ? from.left - (box.left + box.width)
        : dir === "right"
          ? box.left - (from.left + from.width)
          : dir === "up"
            ? from.top - (box.top + box.height)
            : box.top - (from.top + from.height);
    if (gap < -EDGE) continue;

    // How far the two boxes overlap on the other axis: sharing a band means the
    // move feels straight (side by side, or one under the other).
    const shares =
      (horizontal
        ? Math.min(from.top + from.height, box.top + box.height) - Math.max(from.top, box.top)
        : Math.min(from.left + from.width, box.left + box.width) - Math.max(from.left, box.left)) > EDGE;

    const centreDelta = horizontal
      ? Math.abs(box.top + box.height / 2 - fromCy)
      : Math.abs(box.left + box.width / 2 - fromCx);
    const score = Math.max(gap, 0) + centreDelta * 0.5;

    const better = best === null || (shares && !bestShares) || (shares === bestShares && score < bestScore);
    if (better) {
      best = id;
      bestShares = shares;
      bestScore = score;
    }
  }

  return best;
}
