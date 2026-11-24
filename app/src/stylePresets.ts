import type { XamlStyle } from "./xamlCfg";

/**
 * Estilos prontos da barra e do Iniciar, tirados dos repositórios de referência. O método do
 * Sentinela (XAML Diagnostics, o mesmo do Windhawk) troca cor, opacidade, cantos, margens
 * (barra flutuante) e a borda do fundo da barra; não reproduz desfoque (acrílico), gradientes,
 * imagens nem o layout de docks. Quando o original usa desfoque, o preset usa só a tinta dele
 * (`approx`). Os valores foram lidos dos arquivos dos repositórios; cada um tem crédito.
 */
export interface StylePreset {
  id: string;
  name: string;
  /** Repositório de origem (dono/nome). */
  repo: string;
  /** Autor ou tema de origem. */
  credit: string;
  /** Aproximação: o original usa algo que o método não reproduz. */
  approx: boolean;
  note: string;
  style: Partial<XamlStyle>;
}

const GUIDE = "ramensoftware/windows-11-taskbar-styling-guide";

export const STYLE_PRESETS: StylePreset[] = [
  {
    id: "guide-translucent",
    name: "Translucent Taskbar",
    repo: GUIDE,
    credit: "tema TranslucentTaskbar, de Undisputed00x",
    approx: true,
    note: "Tinta #25323232 do original; sem o desfoque.",
    style: { fill: "#323232", alpha: 0x25, radius: 0, margin: [0, 0, 0, 0], hideBorder: true },
  },
  {
    id: "guide-tinted-glass",
    name: "Tinted Glass",
    repo: GUIDE,
    credit: "tema TintedGlass, de TheRealCisWhiteMale",
    approx: true,
    note: "Tinta #80000000 do original; sem o desfoque.",
    style: { fill: "#000000", alpha: 0x80, radius: 0, margin: [0, 0, 0, 0], hideBorder: true },
  },
  {
    id: "guide-plasma",
    name: "Plasma",
    repo: GUIDE,
    credit: "tema Plasma, de Sand216",
    approx: true,
    note: "Tinta #CC2A2E32 do original; sem o desfoque.",
    style: { fill: "#2A2E32", alpha: 0xcc, radius: 0, margin: [0, 0, 0, 0], hideBorder: true },
  },
  {
    id: "guide-oversimplified",
    name: "Oversimplified & Accentuated",
    repo: GUIDE,
    credit: "tema Oversimplified&Accentuated, de OsamaJT",
    approx: true,
    note: "O original usa acrílico a 60% com a cor do sistema; aqui, preto a 60%.",
    style: { fill: "#000000", alpha: 0x99, radius: 0, margin: [0, 0, 0, 0], hideBorder: true },
  },
  {
    id: "guide-transparent",
    name: "Transparente",
    repo: GUIDE,
    credit:
      "fundo transparente de SimplyTransparent (Osprey00), RosePine (asev), Squircle (AsvnDG), xdark (xscriptor), BottomDensy (es) e Matter (ZoraizLajwer)",
    approx: true,
    note: "Só o fundo transparente desses temas; os botões e a bandeja deles não são reproduzidos.",
    style: { fill: "#000000", alpha: 0, radius: 0, margin: [0, 0, 0, 0], hideBorder: true },
  },
  {
    id: "catppuccin-mocha",
    name: "Catppuccin Mocha",
    repo: "abdullah-al-faahim/Windows-11-Customization",
    credit: "configs/windhawk (paleta Catppuccin Mocha, #1e1e2e)",
    approx: true,
    note: "No original o fundo é transparente e os botões e a bandeja têm #1e1e2e com cantos 6; aqui a barra inteira usa a cor e os cantos.",
    style: {
      fill: "#1E1E2E",
      alpha: 0xf0,
      radius: 6,
      margin: [4, 4, 4, 4],
      hideBorder: true,
      startFill: "#1E1E2E",
      startAlpha: 0xf0,
      startRadius: 6,
    },
  },
  {
    id: "astraeus-starfall",
    name: "Starfall",
    repo: "Icetist/Astraeus",
    credit: "aparência Starfall (src/theme.cpp: índigo quase preto, superfície a 74%)",
    approx: true,
    note: "Cor calculada do HSL do original; sem as estrelas e o brilho animado.",
    style: {
      fill: "#12121F",
      alpha: 0xbd,
      radius: 16,
      margin: [8, 4, 8, 4],
      hideBorder: true,
      startFill: "#12121F",
      startAlpha: 0xd0,
      startRadius: 16,
    },
  },
  {
    id: "astraeus-light",
    name: "Claro suave",
    repo: "Icetist/Astraeus",
    credit: "aparência clara (src/theme.cpp: véu pálido tingido, HSL 0.25 / 0.94)",
    approx: true,
    note: "Cor calculada do HSL do original, com matiz fixo; o original segue a cor de destaque.",
    style: {
      fill: "#EDECF4",
      alpha: 0xe6,
      radius: 14,
      margin: [8, 4, 8, 4],
      hideBorder: true,
      startFill: "#EDECF4",
      startAlpha: 0xf0,
      startRadius: 14,
    },
  },
  {
    id: "hyperland-liquid-glass",
    name: "Liquid Glass",
    repo: "Tanmay-Tiwaricyber/HyperLand",
    credit: "visual Liquid Glass do Hyper Island (HyperLand-v2.cpp)",
    approx: true,
    note: "O original usa desfoque real e tinta do tema; aqui, vidro escuro translúcido flutuante, com valores aproximados.",
    style: {
      fill: "#14141C",
      alpha: 0x70,
      radius: 18,
      margin: [10, 4, 10, 4],
      hideBorder: true,
      startFill: "#14141C",
      startAlpha: 0x90,
      startRadius: 18,
    },
  },
  {
    id: "widdgyy-aurora",
    name: "Aurora",
    repo: "dantejvar28/widdgyy",
    credit: "showcase aurora (styles.css: fundo #0d1730)",
    approx: true,
    note: "Só a paleta do showcase; o widdgyy é para Wayland e não estiliza o Windows.",
    style: { fill: "#0D1730", alpha: 0xcc, radius: 14, margin: [0, 0, 0, 0], hideBorder: true },
  },
  {
    id: "widdgyy-bonsai",
    name: "Bonsai",
    repo: "dantejvar28/widdgyy",
    credit: "showcase bonsai (styles.css: papel #e2cfae)",
    approx: true,
    note: "Só a paleta do showcase; o widdgyy é para Wayland e não estiliza o Windows.",
    style: { fill: "#E2CFAE", alpha: 0xe0, radius: 14, margin: [0, 0, 0, 0], hideBorder: true },
  },
  {
    id: "widdgyy-neon",
    name: "Neon",
    repo: "dantejvar28/widdgyy",
    credit: "showcase neon (styles.css: fundo #080d14)",
    approx: true,
    note: "Só a paleta do showcase; o widdgyy é para Wayland e não estiliza o Windows.",
    style: { fill: "#080D14", alpha: 0xe6, radius: 14, margin: [0, 0, 0, 0], hideBorder: true },
  },
  {
    id: "widdgyy-neon-divide",
    name: "Neon Divide",
    repo: "dantejvar28/widdgyy",
    credit: "showcase neon-divide (styles.css: fundo #02060e a 86%)",
    approx: true,
    note: "Só a paleta do showcase; o widdgyy é para Wayland e não estiliza o Windows.",
    style: { fill: "#02060E", alpha: 0xdb, radius: 14, margin: [0, 0, 0, 0], hideBorder: true },
  },
  {
    id: "widdgyy-night-train",
    name: "Night Train",
    repo: "dantejvar28/widdgyy",
    credit: "showcase night-train (styles.css: fundo #06080a a 62%)",
    approx: true,
    note: "Só a paleta do showcase; o widdgyy é para Wayland e não estiliza o Windows.",
    style: { fill: "#06080A", alpha: 0x9e, radius: 14, margin: [0, 0, 0, 0], hideBorder: true },
  },
];

/** O que ficou de fora e por quê (mostrado na tela, para ninguém procurar). */
export const PRESETS_UNAVAILABLE: { what: string; why: string }[] = [
  {
    what: "Lucent, WinXP, Windows7, WinVista",
    why: "Usam gradiente ou imagem no fundo; o método só aplica cor sólida.",
  },
  {
    what: "CleanSlate, Aeris, TaskbarToStatusbar",
    why: "Dependem do acrílico e da cor de destaque do sistema, que mudam de PC para PC.",
  },
  {
    what: "Pills, Blob, Bubbles, DockLike, FluentGlass, FrostyGlass, LiquidGlass2, OS26 Liquid Glass, One UI 8.5, Surface, TaskbarXII, UltraWideFriendly, WindowGlass, Minecraft Hotbar, LayerMicaUI",
    why: "Recriam o layout da barra (dock, botões, bandeja) com expressões do Windhawk; isso exige reescrever a árvore XAML, o que o método atual não faz.",
  },
  {
    what: "21996Taskbar, SunValley, Fluid, LiquidGlass, Command Center, Borderless, Luminosity",
    why: "Mudam botões, bandeja e miniaturas, não só o fundo da barra.",
  },
  {
    what: "Ilhas do Astraeus e do HyperLand (widgets, mídia, foco)",
    why: "São aplicativos próprios que desenham uma janela; não são estilos da barra do Windows.",
  },
];
