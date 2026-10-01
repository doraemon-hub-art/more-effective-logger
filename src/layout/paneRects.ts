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
