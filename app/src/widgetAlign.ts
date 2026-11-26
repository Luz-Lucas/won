/** Alinhamento dos widgets: encaixe nas bordas/centros dos vizinhos, alinhar e distribuir. */

export interface Box {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

export type AlignMode = "left" | "center" | "right" | "top" | "middle" | "bottom";

/** Distância (px reais) em que a borda "gruda" na do vizinho. */
export const NEAR = 12;

export interface Snapped {
  x: number;
  y: number;
  /** Linhas-guia a desenhar (x ou y em px reais). */
  guidesX: number[];
  guidesY: number[];
}

/** Candidatos de uma caixa num eixo: começo, meio e fim. */
const marks = (pos: number, size: number) => [pos, pos + size / 2, pos + size];

function nearest(
  pos: number,
  size: number,
  targets: number[],
  near: number,
): { delta: number; guide: number } | null {
  let best: { delta: number; guide: number } | null = null;
  for (const m of marks(pos, size)) {
    for (const t of targets) {
      const d = t - m;
      if (Math.abs(d) <= near && (best === null || Math.abs(d) < Math.abs(best.delta))) {
        best = { delta: d, guide: t };
      }
    }
  }
  return best;
}

/**
 * Encaixa a caixa movida nas bordas e no centro dos outros widgets e da tela. Sem vizinho por
 * perto, devolve a posição pedida (o chamador já aplicou a grade).
 */
export function snapToOthers(
  moving: Box,
  others: Box[],
  screen: { w: number; h: number },
  near = NEAR,
): Snapped {
  const xs = [0, screen.w / 2, screen.w, ...others.flatMap((o) => marks(o.x, o.width))];
  const ys = [0, screen.h / 2, screen.h, ...others.flatMap((o) => marks(o.y, o.height))];
  const sx = nearest(moving.x, moving.width, xs, near);
  const sy = nearest(moving.y, moving.height, ys, near);
  return {
    x: Math.round(moving.x + (sx?.delta ?? 0)),
    y: Math.round(moving.y + (sy?.delta ?? 0)),
    guidesX: sx ? [sx.guide] : [],
    guidesY: sy ? [sy.guide] : [],
  };
}

/** Alinha os widgets de `ids` ao conjunto (esquerda, centro, direita, topo, meio, base). */
export function alignBoxes<T extends Box>(all: T[], ids: string[], mode: AlignMode): T[] {
  const sel = all.filter((b) => ids.includes(b.id));
  if (sel.length < 2) return all;
  const left = Math.min(...sel.map((b) => b.x));
  const right = Math.max(...sel.map((b) => b.x + b.width));
  const top = Math.min(...sel.map((b) => b.y));
  const bottom = Math.max(...sel.map((b) => b.y + b.height));
  return all.map((b) => {
    if (!ids.includes(b.id)) return b;
    switch (mode) {
      case "left":
        return { ...b, x: left };
      case "right":
        return { ...b, x: right - b.width };
      case "center":
        return { ...b, x: Math.round((left + right) / 2 - b.width / 2) };
      case "top":
        return { ...b, y: top };
      case "bottom":
        return { ...b, y: bottom - b.height };
      case "middle":
        return { ...b, y: Math.round((top + bottom) / 2 - b.height / 2) };
    }
  });
}

/** Distribui 3 ou mais widgets com o mesmo espaço entre eles (os das pontas ficam onde estão). */
export function distributeBoxes<T extends Box>(all: T[], ids: string[], axis: "x" | "y"): T[] {
  const sel = all.filter((b) => ids.includes(b.id));
  if (sel.length < 3) return all;
  const size = axis === "x" ? "width" : "height";
  const sorted = [...sel].sort((a, b) => a[axis] - b[axis]);
  const first = sorted[0]!;
  const last = sorted[sorted.length - 1]!;
  const span = last[axis] + last[size] - first[axis];
  const total = sorted.reduce((s, b) => s + b[size], 0);
  const gap = (span - total) / (sorted.length - 1);
  const pos = new Map<string, number>();
  let cur = first[axis];
  for (const b of sorted) {
    pos.set(b.id, Math.round(cur));
    cur += b[size] + gap;
  }
  return all.map((b) => (pos.has(b.id) ? { ...b, [axis]: pos.get(b.id)! } : b));
}
