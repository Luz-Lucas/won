/** Leitura e escrita do `xaml-barra.cfg` (linhas `chave=valor`) usado pelos mods 6 e 7. */
export interface XamlStyle {
  fill: string; // #RRGGBB
  alpha: number; // 0 a 255
  radius: number;
  /** Margens (esquerda, topo, direita, base): encolher o fundo faz a "ilha". */
  margin: [number, number, number, number];
  hideBorder: boolean;
  startFill: string | null; // #RRGGBB ou null = não mexe
  startAlpha: number;
  startRadius: number | null;
  /** Duas ilhas na barra nativa (apps + bandeja). Usa a cor e o raio acima. */
  islands: boolean;
  islandPad: number;
  islandTrayPad: number;
  islandV: number;
  islandEdge: number;
}

export const DEFAULT_XAML: XamlStyle = {
  fill: "#1C1C24",
  alpha: 200,
  radius: 14,
  margin: [0, 0, 0, 0],
  hideBorder: false,
  startFill: null,
  startAlpha: 230,
  startRadius: null,
  islands: false,
  islandPad: 14,
  islandTrayPad: 10,
  islandV: 4,
  islandEdge: 8,
};

const hex2 = (n: number) =>
  Math.max(0, Math.min(255, Math.round(n))).toString(16).padStart(2, "0").toUpperCase();

function parseArgb(v: string): { rgb: string; a: number } | null {
  const h = v.trim().replace(/^#/, "");
  if (!/^[0-9a-fA-F]{6}$|^[0-9a-fA-F]{8}$/.test(h)) return null;
  return h.length === 8
    ? { a: parseInt(h.slice(0, 2), 16), rgb: `#${h.slice(2).toUpperCase()}` }
    : { a: 255, rgb: `#${h.toUpperCase()}` };
}

export function parseXamlCfg(text: string): XamlStyle {
  const s = { ...DEFAULT_XAML };
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith("#") || line.startsWith("//") || line.startsWith(";")) continue;
    const i = line.indexOf("=");
    if (i < 0) continue;
    const k = line.slice(0, i).trim();
    const v = line.slice(i + 1).trim();
    if (k === "fill") {
      const c = parseArgb(v);
      if (c) {
        s.fill = c.rgb;
        s.alpha = c.a;
      }
    } else if (k === "radius") {
      const n = Number(v);
      if (Number.isFinite(n)) s.radius = Math.max(0, Math.min(60, n));
    } else if (k === "margin") {
      const p = v.split(",").map((x) => Number(x.trim()));
      const c = (n: number) => Math.max(0, Math.min(200, n));
      if (p.length > 0 && p.every(Number.isFinite)) {
        if (p.length === 1) s.margin = [c(p[0]), c(p[0]), c(p[0]), c(p[0])];
        else if (p.length === 2) s.margin = [c(p[0]), c(p[1]), c(p[0]), c(p[1])];
        else if (p.length === 4) s.margin = [c(p[0]), c(p[1]), c(p[2]), c(p[3])];
      }
    } else if (k === "hide_border") {
      s.hideBorder = v === "true" || v === "1";
    } else if (k === "start_fill") {
      const c = parseArgb(v);
      if (c) {
        s.startFill = c.rgb;
        s.startAlpha = c.a;
      }
    } else if (k === "islands") {
      s.islands = v === "true" || v === "1";
    } else if (k === "island_pad" || k === "island_tray_pad" || k === "island_v" || k === "island_edge") {
      const n = Number(v);
      const key = { island_pad: "islandPad", island_tray_pad: "islandTrayPad", island_v: "islandV", island_edge: "islandEdge" }[k] as
        | "islandPad" | "islandTrayPad" | "islandV" | "islandEdge";
      if (Number.isFinite(n)) s[key] = Math.max(0, Math.min(200, n));
    } else if (k === "start_radius") {
      const n = Number(v);
      if (Number.isFinite(n)) s.startRadius = Math.max(0, Math.min(60, n));
    }
  }
  return s;
}

export function buildXamlCfg(s: XamlStyle): string {
  const argb = (rgb: string, a: number) => `#${hex2(a)}${rgb.replace("#", "").toUpperCase()}`;
  const lines = [
    "# Estilo da barra e do Iniciar (mods 6 e 7). Gerado pelo Sentinela.",
    "enabled=true",
    `fill=${argb(s.fill, s.alpha)}`,
    `radius=${s.radius}`,
    `margin=${s.margin.join(",")}`,
    `hide_border=${s.hideBorder}`,
  ];
  if (s.islands) {
    lines.push("islands=true", `island_pad=${s.islandPad}`, `island_tray_pad=${s.islandTrayPad}`);
    lines.push(`island_v=${s.islandV}`, `island_edge=${s.islandEdge}`);
  }
  if (s.startFill) lines.push(`start_fill=${argb(s.startFill, s.startAlpha)}`);
  if (s.startRadius !== null) lines.push(`start_radius=${s.startRadius}`);
  return lines.join("\n") + "\n";
}
