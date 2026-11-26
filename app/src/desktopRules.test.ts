import { describe, expect, it } from "vitest";
import { applyExtras, readExtras } from "./desktopRules";

const FILE = `# meu arquivo
[bar]
height = 40

[[hotkey]]
keys = "Win+Alt+T"
action = "center"

[[hotkey]]
keys = "Win+Alt+N"
action = "run"
command = "notepad.exe"
args = ["a.txt"]

[[rule]]
exe = "a.exe"
float = true
opacity = 200

[borders]
active = "#112233"
inactive = "#445566"
`;

describe("desktopRules", () => {
  it("lê atalhos, regras e bordas", () => {
    const e = readExtras(FILE)!;
    expect(e.hotkeys).toHaveLength(2);
    expect(e.hotkeys[1]).toMatchObject({ action: "run", command: "notepad.exe", args: "a.txt" });
    expect(e.rules[0]).toMatchObject({ exe: "a.exe", float: true, opacity: "200" });
    expect(e.borders).toEqual({ active: "#112233", inactive: "#445566" });
  });

  it("regrava sem perder as outras seções", () => {
    const e = readExtras(FILE)!;
    const out = applyExtras(FILE, { ...e, hotkeys: [...e.hotkeys, { keys: "Win+Alt+Z", action: "snap-layouts", command: "", args: "" }] });
    expect(out).toContain("[bar]\nheight = 40");
    expect(out).toContain('action = "snap-layouts"');
    const again = readExtras(out)!;
    expect(again.hotkeys).toHaveLength(3);
    expect(again.rules).toHaveLength(1);
    expect(again.borders).not.toBeNull();
  });

  it("desliga as bordas", () => {
    const e = readExtras(FILE)!;
    expect(readExtras(applyExtras(FILE, { ...e, borders: null }))!.borders).toBeNull();
  });
});
