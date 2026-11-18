import { describe, expect, it } from "vitest";
import { humanBytes, humanDate } from "./format";

describe("humanBytes", () => {
  it.each([
    [0, "0 B"],
    [1023, "1023 B"],
    [1024, "1.0 KB"],
    [1536, "1.5 KB"],
    [1048576, "1.0 MB"],
    [1024 ** 4, "1.0 TB"],
    [1024 ** 5, "1024.0 TB"],
  ])("humanBytes(%d) = %s", (n, s) => expect(humanBytes(n)).toBe(s));
});

describe("humanDate", () => {
  it("formata em pt-BR (dia/mês/ano)", () => {
    expect(humanDate("2026-03-05T12:00:00Z")).toMatch(/^05\/03\/2026/);
  });

  it("devolve o texto original quando a data é inválida", () => {
    expect(humanDate("ontem")).toBe("ontem");
  });
});
