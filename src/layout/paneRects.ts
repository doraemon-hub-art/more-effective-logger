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
import type { PaneNode } from "./paneTree";

/** Position and size of one pane, as percentages of the layout container. */
export interface PaneBox {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** Walk the tree, halving the box until every leaf has one. */
function walk(node: PaneNode, box: PaneBox, out: Map<string, PaneBox>) {
  if (node.kind === "pane") {
    out.set(node.id, box);
    return;
  }
  const ratio = Math.min(Math.max(node.ratio, 0.05), 0.95);
  if (node.dir === "row") {
    const firstWidth = box.width * ratio;
    walk(node.first, { ...box, width: firstWidth }, out);
    walk(node.second, { ...box, left: box.left + firstWidth, width: box.width - firstWidth }, out);
  } else {
    const firstHeight = box.height * ratio;
    walk(node.first, { ...box, height: firstHeight }, out);
    walk(node.second, { ...box, top: box.top + firstHeight, height: box.height - firstHeight }, out);
  }
}

/** Boxes of every pane in the tree, keyed by pane id. */
export function paneBoxes(tree: PaneNode): Map<string, PaneBox> {
  const boxes = new Map<string, PaneBox>();
  walk(tree, { left: 0, top: 0, width: 100, height: 100 }, boxes);
  return boxes;
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
