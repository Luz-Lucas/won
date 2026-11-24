import { parse } from "smol-toml";

/**
 * Leitura e escrita das seções [bar], [tiling] e [launcher] do desktop.toml.
 * O resto do arquivo (atalhos, regras, bordas e comentários) passa intacto;
 * o programa continua validando tudo ao salvar.
 */

export type Module = "title" | "clock" | "cpu" | "ram";
export type Zone = "left" | "center" | "right";
export type Layout = "master-stack" | "columns" | "grid" | "monocle";

export interface BarModel {
  edge: "top" | "bottom";
  height: number;
  background: string;
  foreground: string;
  font: string;
  fontSize: number;
  hideNative: boolean;
  left: Module[];
  center: Module[];
  right: Module[];
  clockFormat: string;
}

export interface TilingModel {
  layout: Layout;
  gap: number;
  outerGap: number;
  masterRatio: number;
}

export interface LauncherApp {
  name: string;
  command: string;
  args: string[];
}

export interface LauncherModel {
  hotkey: string;
  maxResults: number;
  apps: LauncherApp[];
}

export interface Sections {
  bar: BarModel | null;
  tiling: TilingModel | null;
  launcher: LauncherModel | null;
}

export const MODULES: Module[] = ["title", "cpu", "ram", "clock"];
export const LAYOUTS: Layout[] = ["master-stack", "columns", "grid", "monocle"];

export const DEFAULT_BAR: BarModel = {
  edge: "top",
  height: 32,
  background: "#1E1E22",
  foreground: "#E6E8EB",
  font: "Segoe UI",
  fontSize: 12,
  hideNative: false,
  left: ["title"],
  center: [],
  right: ["cpu", "ram", "clock"],
  clockFormat: "%H:%M",
};
export const DEFAULT_TILING: TilingModel = {
  layout: "master-stack",
  gap: 8,
  outerGap: 8,
  masterRatio: 0.55,
};
export const DEFAULT_LAUNCHER: LauncherModel = { hotkey: "Alt+Space", maxResults: 8, apps: [] };

const num = (v: unknown, d: number) => (typeof v === "number" && Number.isFinite(v) ? v : d);
const str = (v: unknown, d: string) => (typeof v === "string" ? v : d);
const mods = (v: unknown, d: Module[]): Module[] =>
  Array.isArray(v) ? v.filter((m): m is Module => MODULES.includes(m as Module)) : d;
const rec = (v: unknown): Record<string, unknown> | null =>
  typeof v === "object" && v !== null && !Array.isArray(v) ? (v as Record<string, unknown>) : null;

/** `null` quando o arquivo não é TOML válido. */
export function readSections(text: string): Sections | null {
  let doc: Record<string, unknown>;
  try {
    doc = parse(text) as Record<string, unknown>;
  } catch {
    return null;
  }
  const b = rec(doc.bar);
  const t = rec(doc.tiling);
  const l = rec(doc.launcher);
  return {
    bar: b && {
      edge: b.edge === "bottom" ? "bottom" : "top",
      height: num(b.height, DEFAULT_BAR.height),
      background: str(b.background, DEFAULT_BAR.background),
      foreground: str(b.foreground, DEFAULT_BAR.foreground),
      font: str(b.font, DEFAULT_BAR.font),
      fontSize: num(b.font_size, DEFAULT_BAR.fontSize),
      hideNative: b.hide_native_taskbar === true,
      left: mods(b.left, DEFAULT_BAR.left),
      center: mods(b.center, DEFAULT_BAR.center),
      right: mods(b.right, DEFAULT_BAR.right),
      clockFormat: str(b.clock_format, DEFAULT_BAR.clockFormat),
    },
    // `enabled = false` no arquivo equivale a tiling desligado.
    tiling:
      t && t.enabled !== false
        ? {
            layout: LAYOUTS.includes(t.layout as Layout) ? (t.layout as Layout) : "master-stack",
            gap: num(t.gap, DEFAULT_TILING.gap),
            outerGap: num(t.outer_gap, DEFAULT_TILING.outerGap),
            masterRatio: num(t.master_ratio, DEFAULT_TILING.masterRatio),
          }
        : null,
    launcher: l && {
      hotkey: str(l.hotkey, DEFAULT_LAUNCHER.hotkey),
      maxResults: num(l.max_results, DEFAULT_LAUNCHER.maxResults),
      apps: (Array.isArray(l.app) ? l.app : []).flatMap((a) => {
        const r = rec(a);
        return r && typeof r.name === "string" && typeof r.command === "string"
          ? [
              {
                name: r.name,
                command: r.command,
                args: Array.isArray(r.args) ? r.args.filter((x): x is string => typeof x === "string") : [],
              },
            ]
          : [];
      }),
    },
  };
}

const q = JSON.stringify;
const list = (m: Module[]) => `[${m.map((x) => q(x)).join(", ")}]`;

function barToml(b: BarModel): string {
  return [
    "[bar]",
    `edge = ${q(b.edge)}`,
    `height = ${Math.round(b.height)}`,
    `background = ${q(b.background)}`,
    `foreground = ${q(b.foreground)}`,
    `font = ${q(b.font)}`,
    `font_size = ${Math.round(b.fontSize)}`,
    `hide_native_taskbar = ${b.hideNative}`,
    `left = ${list(b.left)}`,
    `center = ${list(b.center)}`,
    `right = ${list(b.right)}`,
    `clock_format = ${q(b.clockFormat)}`,
  ].join("\n");
}

const tilingToml = (t: TilingModel) =>
  [
    "[tiling]",
    `layout = ${q(t.layout)}`,
    `gap = ${Math.round(t.gap)}`,
    `outer_gap = ${Math.round(t.outerGap)}`,
    `master_ratio = ${t.masterRatio.toFixed(2)}`,
  ].join("\n");

const launcherToml = (l: LauncherModel) =>
  [
    "[launcher]",
    `hotkey = ${q(l.hotkey)}`,
    `max_results = ${Math.round(l.maxResults)}`,
    ...l.apps.map(
      (a) =>
        `\n[[launcher.app]]\nname = ${q(a.name)}\ncommand = ${q(a.command)}\nargs = [${a.args.map((x) => q(x)).join(", ")}]`,
    ),
  ].join("\n");

const MANAGED = new Set(["bar", "tiling", "launcher", "launcher.app"]);
const HEADER = /^\s*\[\[?\s*([A-Za-z0-9_.-]+)\s*\]\]?\s*(#.*)?$/;

/** Troca as seções gerenciadas pelas do modelo; o resto do texto fica como está. */
export function applySections(text: string, s: Sections): string {
  const kept: string[] = [];
  let skipping = false;
  for (const line of text.split(/\r?\n/)) {
    const h = HEADER.exec(line);
    if (h) skipping = MANAGED.has(h[1]!);
    if (!skipping) kept.push(line);
  }
  const blocks = [
    s.bar && barToml(s.bar),
    s.tiling && tilingToml(s.tiling),
    s.launcher && launcherToml(s.launcher),
  ].filter((b): b is string => !!b);
  const base = kept.join("\n").replace(/\s+$/, "");
  return `${base}${base ? "\n\n" : ""}${blocks.join("\n\n")}${blocks.length ? "\n" : base ? "\n" : ""}`;
}

export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Retângulos de `n` janelas numa área w×h (mesma lógica do tiling do programa, em escala). */
export function tileBoxes(t: TilingModel, n: number, w: number, h: number, scale: number): Box[] {
  const g = t.gap / scale;
  const o = t.outerGap / scale;
  const aw = w - 2 * o;
  const ah = h - 2 * o;
  const split = (start: number, len: number, parts: number) => {
    const size = (len - g * (parts - 1)) / parts;
    return Array.from({ length: parts }, (_, i) => [start + i * (size + g), size] as const);
  };
  if (t.layout === "monocle") return [{ x: o, y: o, w: aw, h: ah }];
  if (t.layout === "columns")
    return split(o, aw, n).map(([x, sw]) => ({ x, y: o, w: sw, h: ah }));
  if (t.layout === "grid") {
    const cols = Math.ceil(Math.sqrt(n));
    const rows = Math.ceil(n / cols);
    const ys = split(o, ah, rows);
    const out: Box[] = [];
    for (let i = 0; i < n; i++) {
      const r = Math.floor(i / cols);
      const inRow = Math.min(cols, n - r * cols);
      const xs = split(o, aw, inRow);
      const [x, sw] = xs[i % cols]!;
      const [y, sh] = ys[r]!;
      out.push({ x, y, w: sw, h: sh });
    }
    return out;
  }
  if (n === 1) return [{ x: o, y: o, w: aw, h: ah }];
  const mw = (aw - g) * t.masterRatio;
  const out: Box[] = [{ x: o, y: o, w: mw, h: ah }];
  for (const [y, sh] of split(o, ah, n - 1))
    out.push({ x: o + mw + g, y, w: aw - g - mw, h: sh });
  return out;
}
