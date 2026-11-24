import { parse } from "smol-toml";

/**
 * Leitura e escrita de [[hotkey]], [[rule]] e [borders] do desktop.toml, para o editor visual.
 * As outras seções e os comentários passam intactos; o programa valida tudo ao salvar.
 */

export const ACTIONS = [
  "run",
  "toggle-topmost",
  "opacity-up",
  "opacity-down",
  "center",
  "toggle-maximize",
  "tile-toggle",
  "tile-next-layout",
  "focus-next",
  "focus-prev",
  "swap-master",
  "toggle-float",
  "master-grow",
  "master-shrink",
  "snap-layouts",
] as const;
export type ActionName = (typeof ACTIONS)[number];

export const ACTION_LABEL: Record<ActionName, string> = {
  run: "Abrir um programa",
  "toggle-topmost": "Manter a janela sempre no topo",
  "opacity-up": "Menos transparência",
  "opacity-down": "Mais transparência",
  center: "Centralizar a janela",
  "toggle-maximize": "Maximizar ou restaurar",
  "tile-toggle": "Ligar ou desligar o tiling",
  "tile-next-layout": "Próximo layout do tiling",
  "focus-next": "Próxima janela do tiling",
  "focus-prev": "Janela anterior do tiling",
  "swap-master": "Trocar com a janela mestra",
  "toggle-float": "Soltar ou prender a janela no tiling",
  "master-grow": "Aumentar a janela mestra",
  "master-shrink": "Diminuir a janela mestra",
  "snap-layouts": "Abrir os layouts de encaixe do Windows",
};

export interface HotkeyModel {
  keys: string;
  action: ActionName;
  command: string;
  args: string;
}

export type Corners = "" | "round" | "small" | "square";
export type Backdrop = "" | "mica" | "acrylic" | "tabbed" | "none";

export interface RuleModel {
  exe: string;
  title: string;
  class: string;
  topmost: "" | "true" | "false";
  opacity: string;
  noTitlebar: boolean;
  float: boolean;
  corners: Corners;
  backdrop: Backdrop;
}

export interface BordersModel {
  active: string;
  inactive: string;
}

export interface Extras {
  hotkeys: HotkeyModel[];
  rules: RuleModel[];
  borders: BordersModel | null;
}

export const DEFAULT_BORDERS: BordersModel = { active: "#4C9AFF", inactive: "#3A3A44" };
export const EMPTY_RULE: RuleModel = {
  exe: "",
  title: "",
  class: "",
  topmost: "",
  opacity: "",
  noTitlebar: false,
  float: false,
  corners: "",
  backdrop: "",
};

const rec = (v: unknown): Record<string, unknown> | null =>
  typeof v === "object" && v !== null && !Array.isArray(v) ? (v as Record<string, unknown>) : null;
const str = (v: unknown) => (typeof v === "string" ? v : "");

/** `null` quando o arquivo não é TOML válido. */
export function readExtras(text: string): Extras | null {
  let doc: Record<string, unknown>;
  try {
    doc = parse(text) as Record<string, unknown>;
  } catch {
    return null;
  }
  const hotkeys = (Array.isArray(doc.hotkey) ? doc.hotkey : []).flatMap((h) => {
    const r = rec(h);
    if (!r || typeof r.keys !== "string" || !ACTIONS.includes(r.action as ActionName)) return [];
    return [
      {
        keys: r.keys,
        action: r.action as ActionName,
        command: str(r.command),
        args: Array.isArray(r.args) ? r.args.filter((x) => typeof x === "string").join(" ") : "",
      },
    ];
  });
  const rules = (Array.isArray(doc.rule) ? doc.rule : []).flatMap((x) => {
    const r = rec(x);
    if (!r) return [];
    return [
      {
        exe: str(r.exe),
        title: str(r.title),
        class: str(r.class),
        topmost: r.topmost === true ? "true" : r.topmost === false ? "false" : "",
        opacity: typeof r.opacity === "number" ? String(r.opacity) : "",
        noTitlebar: r.no_titlebar === true,
        float: r.float === true,
        corners: (["round", "small", "square"].includes(str(r.corners)) ? str(r.corners) : "") as Corners,
        backdrop: (["mica", "acrylic", "tabbed", "none"].includes(str(r.backdrop))
          ? str(r.backdrop)
          : "") as Backdrop,
      } satisfies RuleModel,
    ];
  });
  const b = rec(doc.borders);
  return {
    hotkeys,
    rules,
    borders: b && {
      active: str(b.active) || DEFAULT_BORDERS.active,
      inactive: str(b.inactive) || DEFAULT_BORDERS.inactive,
    },
  };
}

const q = JSON.stringify;

function hotkeyToml(h: HotkeyModel): string {
  const lines = ["[[hotkey]]", `keys = ${q(h.keys)}`, `action = ${q(h.action)}`];
  if (h.action === "run") {
    lines.push(`command = ${q(h.command)}`);
    const args = h.args.split(/\s+/).filter(Boolean);
    if (args.length) lines.push(`args = [${args.map((a) => q(a)).join(", ")}]`);
  }
  return lines.join("\n");
}

function ruleToml(r: RuleModel): string {
  const lines = ["[[rule]]"];
  if (r.exe) lines.push(`exe = ${q(r.exe)}`);
  if (r.title) lines.push(`title = ${q(r.title)}`);
  if (r.class) lines.push(`class = ${q(r.class)}`);
  if (r.topmost) lines.push(`topmost = ${r.topmost}`);
  if (r.opacity.trim() !== "" && Number.isFinite(Number(r.opacity)))
    lines.push(`opacity = ${Math.round(Number(r.opacity))}`);
  if (r.noTitlebar) lines.push("no_titlebar = true");
  if (r.float) lines.push("float = true");
  if (r.corners) lines.push(`corners = ${q(r.corners)}`);
  if (r.backdrop) lines.push(`backdrop = ${q(r.backdrop)}`);
  return lines.join("\n");
}

const MANAGED = new Set(["hotkey", "rule", "borders"]);
const HEADER = /^\s*\[\[?\s*([A-Za-z0-9_.-]+)\s*\]\]?\s*(#.*)?$/;

/** Troca atalhos, regras e bordas pelas do modelo; o resto do texto fica como está. */
export function applyExtras(text: string, e: Extras): string {
  const kept: string[] = [];
  let skipping = false;
  for (const line of text.split(/\r?\n/)) {
    const h = HEADER.exec(line);
    if (h) skipping = MANAGED.has(h[1]!);
    if (!skipping) kept.push(line);
  }
  const blocks = [
    ...e.hotkeys.map(hotkeyToml),
    ...e.rules.map(ruleToml),
    ...(e.borders ? [`[borders]\nactive = ${q(e.borders.active)}\ninactive = ${q(e.borders.inactive)}`] : []),
  ];
  const base = kept.join("\n").replace(/\s+$/, "");
  return `${base}${base ? "\n\n" : ""}${blocks.join("\n\n")}${blocks.length ? "\n" : base ? "\n" : ""}`;
}
