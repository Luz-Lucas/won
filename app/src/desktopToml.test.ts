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
      tiling: { layout: "grid", gap: 4, outerGap: 6, masterRatio: 0.5 },
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
    const t = { layout: "master-stack" as const, gap: 20, outerGap: 10, masterRatio: 0.6 };
    const b = tileBoxes(t, 4, 192, 108, 10);
    expect(b).toHaveLength(4);
    expect(b[0]!.w).toBeGreaterThan(b[1]!.w);
    expect(tileBoxes({ ...t, layout: "monocle" }, 4, 192, 108, 10)).toHaveLength(1);
    expect(tileBoxes({ ...t, layout: "grid" }, 4, 192, 108, 10)).toHaveLength(4);
  });
});
