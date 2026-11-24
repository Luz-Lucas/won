/** Leitura e escrita do `xaml-barra.cfg` (linhas `chave=valor`) usado pelos mods 6 e 7. */
export interface XamlStyle {
  fill: string; // #RRGGBB
  alpha: number; // 0 a 255
  radius: number;
  margin: number; // mesma margem nos quatro lados ("ilha")
  hideBorder: boolean;
  startFill: string | null; // #RRGGBB ou null = não mexe
  startAlpha: number;
  startRadius: number | null;
}

export const DEFAULT_XAML: XamlStyle = {
  fill: "#1C1C24",
  alpha: 200,
  radius: 14,
  margin: 0,
  hideBorder: false,
  startFill: null,
  startAlpha: 230,
  startRadius: null,
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
      const n = Number(v.split(",")[0]);
      if (Number.isFinite(n)) s.margin = Math.max(0, Math.min(200, n));
    } else if (k === "hide_border") {
      s.hideBorder = v === "true" || v === "1";
    } else if (k === "start_fill") {
      const c = parseArgb(v);
      if (c) {
        s.startFill = c.rgb;
        s.startAlpha = c.a;
      }
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
    `margin=${s.margin},${s.margin},${s.margin},${s.margin}`,
    `hide_border=${s.hideBorder}`,
  ];
  if (s.startFill) lines.push(`start_fill=${argb(s.startFill, s.startAlpha)}`);
  if (s.startRadius !== null) lines.push(`start_radius=${s.startRadius}`);
  return lines.join("\n") + "\n";
}
