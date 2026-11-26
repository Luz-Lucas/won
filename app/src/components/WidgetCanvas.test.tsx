import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import WidgetCanvas, { SIZES, snap } from "./WidgetCanvas";
import { newWidget } from "../desktopToml";

describe("WidgetCanvas", () => {
  it("encaixa na grade", () => {
    expect(snap(33, 20)).toBe(40);
    expect(snap(29, 20)).toBe(20);
    expect(snap(-4, 10)).toBe(0);
  });

  it("adiciona um widget pelo botão", async () => {
    const onChange = vi.fn();
    render(<WidgetCanvas widgets={[]} onChange={onChange} />);
    await userEvent.click(screen.getByRole("button", { name: "+ Relógio" }));
    const list = onChange.mock.calls[0][0];
    expect(list).toHaveLength(1);
    expect(list[0].kind).toBe("clock");
    expect([list[0].width, list[0].height]).toEqual(SIZES.other.M);
  });

  it("troca o tamanho pelas predefinições e a cor pelas amostras", async () => {
    const w = newWidget("cpu", []);
    const onChange = vi.fn();
    render(<WidgetCanvas widgets={[w]} onChange={onChange} />);
    await userEvent.click(screen.getByRole("button", { name: "Tamanho G" }));
    expect([onChange.mock.calls[0][0][0].width, onChange.mock.calls[0][0][0].height]).toEqual(SIZES.other.G);
    await userEvent.click(screen.getByRole("button", { name: "Cor Claro" }));
    expect(onChange.mock.calls[1][0][0].background).toBe("#F2F4F7");
  });

  it("remove o widget selecionado", async () => {
    const onChange = vi.fn();
    render(<WidgetCanvas widgets={[newWidget("ram", [])]} onChange={onChange} />);
    await userEvent.click(screen.getByRole("button", { name: "Remover" }));
    expect(onChange).toHaveBeenCalledWith([]);
  });
});
