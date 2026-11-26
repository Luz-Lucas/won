import { describe, expect, it } from "vitest";
import { alignBoxes, distributeBoxes, snapToOthers } from "./widgetAlign";

const b = (id: string, x: number, y: number, width = 100, height = 100) => ({ id, x, y, width, height });
const screen = { w: 1920, h: 1080 };

describe("widgetAlign", () => {
  it("gruda na borda do vizinho quando está perto", () => {
    const r = snapToOthers(b("a", 205, 400), [b("o", 300, 100)], screen);
    // a borda direita de a (305) gruda na esquerda de o (300): x = 200
    expect(r.x).toBe(200);
    expect(r.guidesX).toEqual([300]);
  });

  it("não mexe quando está longe", () => {
    const r = snapToOthers(b("a", 600, 400), [b("o", 300, 100)], screen);
    expect([r.x, r.y]).toEqual([600, 400]);
    expect(r.guidesX).toEqual([]);
  });

  it("alinha pela esquerda e pelo meio", () => {
    const all = [b("a", 10, 0), b("b", 80, 50), b("c", 500, 500)];
    expect(alignBoxes(all, ["a", "b"], "left").map((x) => x.x)).toEqual([10, 10, 500]);
    const mid = alignBoxes(all, ["a", "b"], "middle");
    expect(mid[0]!.y + 50).toBe(mid[1]!.y + 50);
  });

  it("não alinha com um só", () => {
    const all = [b("a", 10, 0)];
    expect(alignBoxes(all, ["a"], "right")).toBe(all);
  });

  it("distribui com espaços iguais", () => {
    const all = [b("a", 0, 0), b("b", 130, 0), b("c", 400, 0)];
    const r = distributeBoxes(all, ["a", "b", "c"], "x");
    const gap1 = r[1]!.x - (r[0]!.x + r[0]!.width);
    const gap2 = r[2]!.x - (r[1]!.x + r[1]!.width);
    expect(gap1).toBe(gap2);
  });
});
