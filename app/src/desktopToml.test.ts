import { describe, expect, it } from "vitest";
import { DEFAULT_BAR, applySections, readSections, tileBoxes } from "./desktopToml";

const FILE = `# meu arquivo
[[hotkey]]
keys = "Win+Alt+T"
action = "center"

[bar]
edge = "bottom"
height = 40
left = ["clock"]
right = []

# tiling comentado
[[rule]]
exe = "a.exe"
float = true
`;

describe("desktopToml", () => {
  it("reads the managed sections with defaults for missing keys", () => {
    const s = readSections(FILE)!;
    expect(s.bar).toMatchObject({ edge: "bottom", height: 40, left: ["clock"], right: [], font: "Segoe UI" });
    expect(s.tiling).toBeNull();
    expect(s.launcher).toBeNull();
  });

  it("returns null for invalid TOML", () => {
    expect(readSections("[bar\nx")).toBeNull();
  });

  it("rewrites only the managed blocks and keeps hotkeys, rules and comments", () => {
    const s = readSections(FILE)!;
    const out = applySections(FILE, {
      ...s,
      bar: { ...DEFAULT_BAR, height: 50 },
      tiling: { layout: "grid", gap: 4, outerGap: 6, masterRatio: 0.5, portraitLayout: "rows", monitors: [] },
    });
    expect(out).toContain("# meu arquivo");
    expect(out).toContain('[[hotkey]]\nkeys = "Win+Alt+T"');
    expect(out).toContain('[[rule]]\nexe = "a.exe"');
    expect(out.match(/\[bar\]/g)).toHaveLength(1);
    const back = readSections(out)!;
    expect(back.bar).toMatchObject({ height: 50, edge: "top" });
    expect(back.tiling).toMatchObject({ layout: "grid", gap: 4, outerGap: 6, masterRatio: 0.5 });
  });

  it("removes a section when turned off and keeps launcher apps on rewrite", () => {
    const text = `[launcher]\nhotkey = "Alt+Space"\n[[launcher.app]]\nname = "T"\ncommand = "wt.exe"\nargs = ["-p", "PS"]\n[bar]\nheight = 30\n`;
    const s = readSections(text)!;
    const off = applySections(text, { ...s, bar: null });
    expect(readSections(off)!.bar).toBeNull();
    expect(readSections(off)!.launcher!.apps).toEqual([{ name: "T", command: "wt.exe", args: ["-p", "PS"] }]);
    const emptied = applySections(off, { bar: null, tiling: null, launcher: null });
    expect(emptied).not.toContain("launcher");
  });

  it("tile boxes never overlap and fill the layout", () => {
    const t = { layout: "master-stack" as const, gap: 20, outerGap: 10, masterRatio: 0.6, portraitLayout: "rows" as const, monitors: [] };
    const b = tileBoxes(t, 4, 192, 108, 10);
    expect(b).toHaveLength(4);
    expect(b[0]!.w).toBeGreaterThan(b[1]!.w);
    expect(tileBoxes({ ...t, layout: "monocle" }, 4, 192, 108, 10)).toHaveLength(1);
    expect(tileBoxes({ ...t, layout: "grid" }, 4, 192, 108, 10)).toHaveLength(4);
  });
});

describe("tiling por monitor", () => {
  it("lê e grava layout de retrato e regras de monitor", () => {
    const text = `[tiling]\nlayout = "grid"\nportrait_layout = "rows"\n\n[[tiling.monitor]]\nindex = 2\nlayout = "rows"\nmax_windows = 3\n`;
    const s = readSections(text)!;
    expect(s.tiling!.portraitLayout).toBe("rows");
    expect(s.tiling!.monitors).toEqual([{ index: 2, layout: "rows", maxWindows: 3 }]);
    const out = applySections(text, s);
    expect(out).toContain("[[tiling.monitor]]\nindex = 2\nlayout = \"rows\"\nmax_windows = 3");
    expect(out.match(/\[\[tiling\.monitor\]\]/g)).toHaveLength(1);
  });
});

describe("barra em duas ilhas", () => {
  it("lê e grava as opções das ilhas", () => {
    const text = `[bar]\nislands = true\nisland_radius = 20\nisland_margin = 8\nisland_align = "left"\n`;
    const b = readSections(text)!.bar!;
    expect(b).toMatchObject({ islands: true, islandRadius: 20, islandMargin: 8, islandAlign: "left" });
    const out = applySections(text, { ...readSections(text)!, bar: { ...b, islandRadius: 10 } });
    expect(out).toContain("islands = true");
    expect(out).toContain("island_radius = 10");
    expect(out).toContain('island_align = "left"');
  });
});

describe("widgets", () => {
  it("lê, grava e preserva o resto do arquivo", async () => {
    const { readSections, applySections, newWidget } = await import("./desktopToml");
    const base = "# meu arquivo\n[[hotkey]]\nkeys = \"Alt+A\"\naction = \"close\"\n";
    const s = readSections(base)!;
    expect(s.widgets).toEqual([]);
    const w = newWidget("clock", []);
    const text = applySections(base, { ...s, widgets: [w, newWidget("cpu", [w])] });
    expect(text).toContain("# meu arquivo");
    const back = readSections(text)!;
    expect(back.widgets?.map((x) => x.kind)).toEqual(["clock", "cpu"]);
    expect(back.widgets?.[0]?.format).toBe("%H:%M");
    expect(back.widgets?.[0]?.opacity).toBe(100);
    // regravar não duplica
    expect(applySections(text, back).match(/\[\[widget\]\]/g)).toHaveLength(2);
  });
});
