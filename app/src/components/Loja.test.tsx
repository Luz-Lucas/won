import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import Loja from "./Loja";

vi.mock("../api", () => ({
  lojaList: vi.fn(),
  lojaInstall: vi.fn(),
  lojaApply: vi.fn(),
  lojaRemove: vi.fn(),
  lojaRevert: vi.fn(),
  restartExplorer: vi.fn(),
  themePreview: vi.fn(),
  themeApply: vi.fn(),
  themeUndo: vi.fn(),
}));

const item = (o: Partial<api.LojaItem>): api.LojaItem => ({
  id: "x",
  kind: "cursores",
  name: "X",
  author: "WON",
  license: "CC0 1.0",
  description: "d",
  tags: [],
  preview: "data:image/png;base64,AA==",
  bytes: 2048,
  remote: false,
  toml: null,
  cursor: null,
  icons: null,
  installed: false,
  applied: false,
  ...o,
});

const page = (items: api.LojaItem[]): api.LojaPage => ({ items, online: false, online_error: null });

beforeEach(() => {
  vi.clearAllMocks();
});

describe("Loja", () => {
  it("baixa, aplica e reverte um ponteiro", async () => {
    const user = userEvent.setup();
    const l = vi.mocked(api.lojaList);
    l.mockResolvedValueOnce(page([item({ id: "c1", name: "Noite" })]));
    vi.mocked(api.lojaInstall).mockResolvedValue();
    render(<Loja />);
    l.mockResolvedValueOnce(page([item({ id: "c1", name: "Noite", installed: true })]));
    await user.click(await screen.findByRole("button", { name: "Baixar Noite" }));
    expect(api.lojaInstall).toHaveBeenCalledWith("c1", false);

    vi.mocked(api.lojaApply).mockResolvedValue({ id: "c1", restart_explorer: false });
    l.mockResolvedValueOnce(page([item({ id: "c1", name: "Noite", installed: true, applied: true })]));
    await user.click(await screen.findByRole("button", { name: "Aplicar Noite" }));
    expect(api.lojaApply).toHaveBeenCalledWith("c1", false);

    vi.mocked(api.lojaRevert).mockResolvedValue({ id: "c1", restart_explorer: false, failed: [] });
    l.mockResolvedValueOnce(page([item({ id: "c1", name: "Noite", installed: true })]));
    await user.click(await screen.findByRole("button", { name: "Reverter Noite" }));
    expect(api.lojaRevert).toHaveBeenCalledWith("cursores");
    await waitFor(() => expect(screen.getByText(/Revertido/)).toBeTruthy());
  });

  it("filtra por categoria e mostra erro de rede sem esconder o catálogo", async () => {
    const user = userEvent.setup();
    vi.mocked(api.lojaList).mockResolvedValue({
      items: [item({ id: "c1", name: "Noite" }), item({ id: "i1", kind: "icones", name: "Flat" })],
      online: false,
      online_error: "falha de rede",
    });
    render(<Loja />);
    expect(await screen.findByText("Noite")).toBeTruthy();
    expect(screen.getByText(/indisponível/)).toBeTruthy();
    await user.click(screen.getByRole("tab", { name: "Pacotes de ícones" }));
    expect(screen.queryByText("Noite")).toBeNull();
    expect(screen.getByText("Flat")).toBeTruthy();
  });
});
