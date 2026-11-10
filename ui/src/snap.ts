// 智能参考线 + 等间距吸附（对标 PPT）：
//   1) 移动/缩放时吸附到相邻元素与画布的对齐线；
//   2) 同一行（列）里已有两块时，把新块放到它们旁边，让三块的间距一致（1 _ 2 _ 3）。
// 设置页的预览编辑和桌面编辑模式共用这一份。

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** 一条参考线：vertical=true 是竖线；at 是内容坐标；from/to 是线段范围 */
export interface Guide {
  vertical: boolean;
  at: number;
  from: number;
  to: number;
}

interface SnapTarget {
  at: number;
  from: number;
  to: number;
}

export interface SnapTargets {
  xs: SnapTarget[];
  ys: SnapTarget[];
}

/** 被排除的下标：单个数字或集合（融合块整体移动时要排除整块） */
type Exclude = number | Set<number>;

function excluded(ex: Exclude, i: number): boolean {
  return typeof ex === 'number' ? i === ex : ex.has(i);
}

/** 收集吸附基准：画布左/中/右（上/中/下）+ 其它卡片的左/中/右（上/中/下） */
export function buildSnapTargets(
  rects: Rect[],
  excludeIndex: Exclude,
  canvas: { w: number; h: number },
): SnapTargets {
  const xs: SnapTarget[] = [
    { at: 0, from: 0, to: canvas.h },
    { at: canvas.w / 2, from: 0, to: canvas.h },
    { at: canvas.w, from: 0, to: canvas.h },
  ];
  const ys: SnapTarget[] = [
    { at: 0, from: 0, to: canvas.w },
    { at: canvas.h / 2, from: 0, to: canvas.w },
    { at: canvas.h, from: 0, to: canvas.w },
  ];
  rects.forEach((r, i) => {
    if (excluded(excludeIndex, i)) return;
    xs.push(
      { at: r.x, from: r.y, to: r.y + r.h },
      { at: r.x + r.w / 2, from: r.y, to: r.y + r.h },
      { at: r.x + r.w, from: r.y, to: r.y + r.h },
    );
    ys.push(
      { at: r.y, from: r.x, to: r.x + r.w },
      { at: r.y + r.h / 2, from: r.x, to: r.x + r.w },
      { at: r.y + r.h, from: r.x, to: r.x + r.w },
    );
  });
  return { xs, ys };
}

function snapAxis(
  cands: number[],
  targets: SnapTarget[],
  tol: number,
): { delta: number; target: SnapTarget } | null {
  let best: { delta: number; target: SnapTarget } | null = null;
  for (const cand of cands) {
    for (const t of targets) {
      const delta = t.at - cand;
      if (Math.abs(delta) > tol) continue;
      if (!best || Math.abs(delta) < Math.abs(best.delta)) best = { delta, target: t };
    }
  }
  return best;
}

/**
 * 等间距吸附：在同一行（列）里找相邻两块 a、b，算出它们的间距 gap，
 * 然后给出「贴到 b 右侧」和「贴到 a 左侧」两个候选位置，让间距保持一致。
 */
function spacingSnap(
  rect: Rect,
  rects: Rect[],
  tol: number,
  axis: "x" | "y",
): { delta: number; guides: Guide[] } | null {
  const horizontal = axis === "x";
  // 只考虑在垂直方向（水平方向）和移动块有重叠的块 → 同一行 / 同一列
  const row = rects.filter((r) =>
    horizontal
      ? Math.min(r.y + r.h, rect.y + rect.h) - Math.max(r.y, rect.y) > 0
      : Math.min(r.x + r.w, rect.x + rect.w) - Math.max(r.x, rect.x) > 0,
  );
  if (row.length < 2) return null;

  const sorted = [...row].sort((a, b) => (horizontal ? a.x - b.x : a.y - b.y));
  let best: { delta: number; guides: Guide[] } | null = null;

  for (let i = 0; i < sorted.length - 1; i++) {
    const a = sorted[i];
    const b = sorted[i + 1];
    const gap = horizontal ? b.x - (a.x + a.w) : b.y - (a.y + a.h);
    if (gap <= 0.5 || gap > 600) continue;

    // 参考线画在「块与块的边界」上，横跨这一行的范围
    const from = horizontal
      ? Math.min(a.y, rect.y)
      : Math.min(a.x, rect.x);
    const to = horizontal
      ? Math.max(b.y + b.h, rect.y + rect.h)
      : Math.max(b.x + b.w, rect.x + rect.w);

    const makeGuide = (at: number): Guide =>
      horizontal ? { vertical: true, at, from, to } : { vertical: false, at, from, to };

    // 贴到 b 的右侧：新块左边界 = b.right + gap
    const rightAt = horizontal ? b.x + b.w + gap : b.y + b.h + gap;
    const rightDelta = rightAt - (horizontal ? rect.x : rect.y);
    if (Math.abs(rightDelta) <= tol && (!best || Math.abs(rightDelta) < Math.abs(best.delta))) {
      const edge = horizontal ? b.x + b.w : b.y + b.h;
      best = { delta: rightDelta, guides: [makeGuide(edge), makeGuide(rightAt)] };
    }

    // 贴到 a 的左侧：新块右边界 = a.left - gap
    const leftAt = horizontal ? a.x - gap - rect.w : a.y - gap - rect.h;
    const leftDelta = leftAt - (horizontal ? rect.x : rect.y);
    if (Math.abs(leftDelta) <= tol && (!best || Math.abs(leftDelta) < Math.abs(best.delta))) {
      const edge = horizontal ? a.x : a.y;
      best = { delta: leftDelta, guides: [makeGuide(edge), makeGuide(leftAt)] };
    }
  }
  return best;
}

export interface SnapResult {
  x: number;
  y: number;
  guides: Guide[];
  snappedX: boolean;
  snappedY: boolean;
  /** 这次吸附是不是「等间距」触发的（方便调用方显示不同提示） */
  spacedX: boolean;
  spacedY: boolean;
}

/**
 * 算出吸附后的位置。
 * mode=move：左/中/右（上/中/下）都参与比对；mode=resize：只吸被拖的那条边。
 * tol 用内容坐标（屏幕像素要除以缩放比例）。
 */
export function snapRect(
  rect: Rect,
  targets: SnapTargets,
  tol: number,
  mode: "move" | "resize",
  rects: Rect[] = [],
  excludeIndex: Exclude = -1,
): SnapResult {
  const guides: Guide[] = [];
  let x = rect.x;
  let y = rect.y;
  let snappedX = false;
  let snappedY = false;
  let spacedX = false;
  let spacedY = false;

  const others = rects.filter((_, i) => !excluded(excludeIndex, i));

  // ---- X 轴 ----
  const xCands = mode === "move" ? [rect.x, rect.x + rect.w / 2, rect.x + rect.w] : [rect.x + rect.w];
  const sx = snapAxis(xCands, targets.xs, tol);
  const spaced = mode === "move" ? spacingSnap(rect, others, tol, "x") : null;
  if (spaced && (!sx || Math.abs(spaced.delta) < Math.abs(sx.delta))) {
    x += spaced.delta;
    guides.push(...spaced.guides);
    snappedX = true;
    spacedX = true;
  } else if (sx) {
    x += sx.delta;
    guides.push({
      vertical: true,
      at: sx.target.at,
      from: Math.min(sx.target.from, rect.y),
      to: Math.max(sx.target.to, rect.y + rect.h),
    });
    snappedX = true;
  }

  // ---- Y 轴 ----
  const yCands = mode === "move" ? [rect.y, rect.y + rect.h / 2, rect.y + rect.h] : [rect.y + rect.h];
  const sy = snapAxis(yCands, targets.ys, tol);
  const spacedYRes = mode === "move" ? spacingSnap(rect, others, tol, "y") : null;
  if (spacedYRes && (!sy || Math.abs(spacedYRes.delta) < Math.abs(sy.delta))) {
    y += spacedYRes.delta;
    guides.push(...spacedYRes.guides);
    snappedY = true;
    spacedY = true;
  } else if (sy) {
    y += sy.delta;
    guides.push({
      vertical: false,
      at: sy.target.at,
      from: Math.min(sy.target.from, rect.x),
      to: Math.max(sy.target.to, rect.x + rect.w),
    });
    snappedY = true;
  }

  return { x, y, guides, snappedX, snappedY, spacedX, spacedY };
}
