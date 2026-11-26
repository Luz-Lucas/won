import { parse } from "smol-toml";

/**
 * Leitura e escrita das seções [bar], [tiling] e [launcher] do desktop.toml.
 * O resto do arquivo (atalhos, regras, bordas e comentários) passa intacto;
 * o programa continua validando tudo ao salvar.
 */

export type Module = "title" | "clock" | "cpu" | "ram" | "volume" | "network" | "media";
export type Zone = "left" | "center" | "right";
export type Layout = "master-stack" | "columns" | "grid" | "monocle" | "rows";

export interface BarModel {
  edge: "top" | "bottom";
  height: number;
  background: string;
  foreground: string;
  font: string;
  fontSize: number;
  /** 100 a 900: 400 normal, 700 negrito. */
  fontWeight: number;
  /** Com este número de janelas ou mais a ilha mostra só ícones (0 = nunca). */
  iconsOnlyFrom: number;
  /** Esconde a barra com uma janela maximizada em primeiro plano. */
  hideWhenMaximized: boolean;
  /** Sem nenhum app aberto, a barra fica sempre visível. */
  alwaysVisibleWhenEmpty: boolean;
  /** Clicar na área de trabalho não esconde a barra. */
  visibleOnDesktop: boolean;
  /** Contorno em volta do ícone do app ativo. */
  activeOutline: boolean;
  hideNative: boolean;
  left: Module[];
  center: Module[];
  right: Module[];
  clockFormat: string;
  /** Duas ilhas: janelas abertas (largura ajustada) e módulos à direita. */
  islands: boolean;
  islandRadius: number;
  islandMargin: number;
  islandAlign: "center" | "left";
}

/** Regra de um monitor: 1 = o mais à esquerda. */
export interface MonitorRule {
  index: number;
  layout: Layout | "";
  maxWindows: number | null;
}

export interface TilingModel {
  layout: Layout;
  gap: number;
  outerGap: number;
  masterRatio: number;
  /** Layout dos monitores em retrato (mais altos que largos). */
  portraitLayout: Layout;
  monitors: MonitorRule[];
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

export interface ClipboardModel {
  stripTracking: boolean;
  trim: boolean;
  unwrap: boolean;
  casing: "keep" | "lower" | "upper" | "title";
  paths: "keep" | "double_backslash" | "forward";
  extract: "off" | "urls" | "emails";
  replace: { from: string; to: string }[];
}

export type WidgetKind = "clock" | "cpu" | "ram" | "network" | "media" | "weather" | "agenda" | "tasks" | "calendar";
export const WIDGET_KINDS: WidgetKind[] = ["clock", "cpu", "ram", "network", "media", "weather", "agenda", "tasks", "calendar"];
export const WIDGET_NAMES: Record<WidgetKind, string> = {
  clock: "Relógio",
  cpu: "CPU",
  ram: "Memória",
  network: "Rede",
  media: "Mídia tocando",
  weather: "Clima",
  agenda: "Próximos eventos (Organize)",
  tasks: "Tarefas (Organize)",
  calendar: "Calendário (Organize)",
};

export interface WidgetModel {
  id: string;
  kind: WidgetKind;
  x: number;
  y: number;
  width: number;
  height: number;
  background: string;
  foreground: string;
  fontSize: number;
  radius: number;
  /** Opacidade do fundo, 0 (só o texto) a 100. */
  opacity: number;
  locked: boolean;
  format: string;
  /** Só no clima: cidade da previsão. */
  city: string;
}

const LIST_SIZES: Partial<Record<WidgetKind, [number, number]>> = {
  agenda: [300, 170],
  tasks: [300, 170],
  calendar: [270, 190],
};

export function newWidget(kind: WidgetKind, existing: WidgetModel[]): WidgetModel {
  let n = existing.length + 1;
  while (existing.some((w) => w.id === `${kind}${n}`)) n += 1;
  return {
    id: `${kind}${n}`,
    kind,
    x: 80 + 30 * existing.length,
    y: 80 + 30 * existing.length,
    width: LIST_SIZES[kind]?.[0] ?? (kind === "media" ? 300 : 170),
    height: LIST_SIZES[kind]?.[1] ?? (kind === "media" ? 90 : 100),
    background: "#14161A",
    foreground: "#FFFFFF",
    fontSize: LIST_SIZES[kind] ? 13 : 24,
    radius: 16,
    opacity: 100,
    locked: false,
    format: "%H:%M",
    city: "São Paulo",
  };
}

export interface Sections {
  widgets?: WidgetModel[];
  bar: BarModel | null;
  tiling: TilingModel | null;
  launcher: LauncherModel | null;
  clipboard?: ClipboardModel | null;
  /** `[taskbar] middle_click_close`. */
  middleClose?: boolean;
}

export const DEFAULT_CLIPBOARD: ClipboardModel = {
  stripTracking: true,
  trim: true,
  unwrap: false,
  casing: "keep",
  paths: "keep",
  extract: "off",
  replace: [],
};

export const MODULES: Module[] = ["title", "network", "volume", "media", "cpu", "ram", "clock"];
export const LAYOUTS: Layout[] = ["master-stack", "columns", "grid", "monocle", "rows"];

export const DEFAULT_BAR: BarModel = {
  edge: "top",
  height: 32,
  background: "#1E1E22",
  foreground: "#E6E8EB",
  font: "Segoe UI",
  fontSize: 12,
  fontWeight: 700,
  iconsOnlyFrom: 3,
  hideWhenMaximized: true,
  alwaysVisibleWhenEmpty: true,
  visibleOnDesktop: true,
  activeOutline: true,
  hideNative: false,
  left: ["title"],
  center: [],
  right: ["cpu", "ram", "clock"],
  clockFormat: "%H:%M",
  islands: false,
  islandRadius: 12,
  islandMargin: 6,
  islandAlign: "center",
};
export const DEFAULT_TILING: TilingModel = {
  layout: "master-stack",
  gap: 8,
  outerGap: 8,
  masterRatio: 0.55,
  portraitLayout: "rows",
  monitors: [],
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
  const c = rec(doc.clipboard);
  const tb = rec(doc.taskbar);
  const widgets = (Array.isArray(doc.widget) ? doc.widget : []).flatMap((w, i): WidgetModel[] => {
    const r = rec(w);
    if (!r || !WIDGET_KINDS.includes(r.kind as WidgetKind)) return [];
    const kind = r.kind as WidgetKind;
    const d = newWidget(kind, []);
    return [
      {
        id: str(r.id, `widget${i + 1}`),
        kind,
        x: num(r.x, 80 + 30 * i),
        y: num(r.y, 80 + 30 * i),
        width: num(r.width, d.width),
        height: num(r.height, d.height),
        background: str(r.background, d.background),
        foreground: str(r.foreground, d.foreground),
        fontSize: num(r.font_size, d.fontSize),
        radius: num(r.radius, d.radius),
        opacity: Math.min(100, Math.max(0, num(r.opacity, d.opacity))),
        locked: r.locked === true,
        format: str(r.format, d.format),
        city: str(r.city, d.city),
      },
    ];
  });
  return {
    widgets,
    middleClose: tb?.middle_click_close === true,
    bar: b && {
      edge: b.edge === "bottom" ? "bottom" : "top",
      height: num(b.height, DEFAULT_BAR.height),
      background: str(b.background, DEFAULT_BAR.background),
      foreground: str(b.foreground, DEFAULT_BAR.foreground),
      font: str(b.font, DEFAULT_BAR.font),
      fontSize: num(b.font_size, DEFAULT_BAR.fontSize),
      fontWeight: num(b.font_weight, DEFAULT_BAR.fontWeight),
      iconsOnlyFrom: num(b.icons_only_from, DEFAULT_BAR.iconsOnlyFrom),
      hideWhenMaximized: b.hide_when_maximized !== false,
      alwaysVisibleWhenEmpty: b.always_visible_when_empty !== false,
      visibleOnDesktop: b.visible_on_desktop !== false,
      activeOutline: b.active_outline !== false,
      hideNative: b.hide_native_taskbar === true,
      left: mods(b.left, DEFAULT_BAR.left),
      center: mods(b.center, DEFAULT_BAR.center),
      right: mods(b.right, DEFAULT_BAR.right),
      clockFormat: str(b.clock_format, DEFAULT_BAR.clockFormat),
      islands: b.islands === true,
      islandRadius: num(b.island_radius, DEFAULT_BAR.islandRadius),
      islandMargin: num(b.island_margin, DEFAULT_BAR.islandMargin),
      islandAlign: b.island_align === "left" ? "left" : "center",
    },
    // `enabled = false` no arquivo equivale a tiling desligado.
    tiling:
      t && t.enabled !== false
        ? {
            layout: LAYOUTS.includes(t.layout as Layout) ? (t.layout as Layout) : "master-stack",
            gap: num(t.gap, DEFAULT_TILING.gap),
            outerGap: num(t.outer_gap, DEFAULT_TILING.outerGap),
            masterRatio: num(t.master_ratio, DEFAULT_TILING.masterRatio),
            portraitLayout: LAYOUTS.includes(t.portrait_layout as Layout)
              ? (t.portrait_layout as Layout)
              : "rows",
            monitors: (Array.isArray(t.monitor) ? t.monitor : []).flatMap((m): MonitorRule[] => {
              const r = rec(m);
              return r && typeof r.index === "number"
                ? [
                    {
                      index: r.index,
                      layout: LAYOUTS.includes(r.layout as Layout) ? (r.layout as Layout) : "",
                      maxWindows: typeof r.max_windows === "number" ? r.max_windows : null,
                    },
                  ]
                : [];
            }),
          }
        : null,
    clipboard:
      c && {
        stripTracking: c.strip_tracking === true,
        trim: c.trim === true,
        unwrap: c.unwrap === true,
        casing: (["lower", "upper", "title"] as const).find((x) => x === c.casing) ?? "keep",
        paths: (["double_backslash", "forward"] as const).find((x) => x === c.paths) ?? "keep",
        extract: (["urls", "emails"] as const).find((x) => x === c.extract) ?? "off",
        replace: (Array.isArray(c.replace) ? c.replace : []).flatMap((r) => {
          const x = rec(r);
          return x && typeof x.from === "string" && typeof x.to === "string" ? [{ from: x.from, to: x.to }] : [];
        }),
      },
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
    `font_weight = ${Math.round(b.fontWeight)}`,
    `icons_only_from = ${Math.round(b.iconsOnlyFrom)}`,
    `hide_when_maximized = ${b.hideWhenMaximized}`,
    `always_visible_when_empty = ${b.alwaysVisibleWhenEmpty}`,
    `visible_on_desktop = ${b.visibleOnDesktop}`,
    `active_outline = ${b.activeOutline}`,
    `hide_native_taskbar = ${b.hideNative}`,
    `left = ${list(b.left)}`,
    `center = ${list(b.center)}`,
    `right = ${list(b.right)}`,
    `clock_format = ${q(b.clockFormat)}`,
    `islands = ${b.islands}`,
    `island_radius = ${Math.round(b.islandRadius)}`,
    `island_margin = ${Math.round(b.islandMargin)}`,
    `island_align = ${q(b.islandAlign)}`,
  ].join("\n");
}

const tilingToml = (t: TilingModel) =>
  [
    "[tiling]",
    `layout = ${q(t.layout)}`,
    `gap = ${Math.round(t.gap)}`,
    `outer_gap = ${Math.round(t.outerGap)}`,
    `master_ratio = ${t.masterRatio.toFixed(2)}`,
    `portrait_layout = ${q(t.portraitLayout)}`,
    ...t.monitors.map((m) =>
      [
        "\n[[tiling.monitor]]",
        `index = ${Math.round(m.index)}`,
        ...(m.layout ? [`layout = ${q(m.layout)}`] : []),
        ...(m.maxWindows !== null ? [`max_windows = ${Math.round(m.maxWindows)}`] : []),
      ].join("\n"),
    ),
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

const widgetToml = (w: WidgetModel) =>
  [
    "[[widget]]",
    `id = ${q(w.id)}`,
    `kind = ${q(w.kind)}`,
    `x = ${Math.round(w.x)}`,
    `y = ${Math.round(w.y)}`,
    `width = ${Math.round(w.width)}`,
    `height = ${Math.round(w.height)}`,
    `background = ${q(w.background)}`,
    `foreground = ${q(w.foreground)}`,
    `font_size = ${Math.round(w.fontSize)}`,
    `radius = ${Math.round(w.radius)}`,
    `opacity = ${Math.round(Math.min(100, Math.max(0, w.opacity)))}`,
    `locked = ${w.locked}`,
    ...(w.kind === "clock" ? [`format = ${q(w.format)}`] : []),
    ...(w.kind === "weather" ? [`city = ${q(w.city)}`] : []),
  ].join("\n");

const clipboardToml = (c: ClipboardModel) =>
  [
    "[clipboard]",
    `strip_tracking = ${c.stripTracking}`,
    `trim = ${c.trim}`,
    `unwrap = ${c.unwrap}`,
    `casing = ${q(c.casing)}`,
    `paths = ${q(c.paths)}`,
    `extract = ${q(c.extract)}`,
    ...c.replace.map((r) => `\n[[clipboard.replace]]\nfrom = ${q(r.from)}\nto = ${q(r.to)}`),
  ].join("\n");

const MANAGED = new Set(["bar", "tiling", "tiling.monitor", "launcher", "launcher.app", "clipboard", "clipboard.replace", "taskbar", "widget"]);
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
    s.clipboard && clipboardToml(s.clipboard),
    ...(s.widgets ?? []).map(widgetToml),
    s.middleClose ? "[taskbar]\nmiddle_click_close = true" : null,
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
  if (t.layout === "rows") return split(o, ah, n).map(([y, sh]) => ({ x: o, y, w: aw, h: sh }));
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
