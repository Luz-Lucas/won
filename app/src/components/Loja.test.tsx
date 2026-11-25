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
  lojaCursorSize: vi.fn(),
  lojaSetCursorSize: vi.fn(),
  lojaApplyShortcuts: vi.fn(),
  lojaRevertShortcuts: vi.fn(),
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

const page = (items: api.LojaItem[]): api.LojaPage => ({ items, online: false, online_error: null, shortcuts_active: null });

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.lojaCursorSize).mockResolvedValue(null);
  vi.mocked(api.lojaSetCursorSize).mockResolvedValue();
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

  it("troca o ícone dos atalhos só depois de confirmar e restaura os originais", async () => {
    const user = userEvent.setup();
    const l = vi.mocked(api.lojaList);
    const pack = item({ id: "i1", kind: "icones", name: "Fluent preto", tags: ["atalhos"], installed: true, applied: true });
    l.mockResolvedValueOnce(page([pack]));
    render(<Loja />);
    await user.click(await screen.findByRole("button", { name: "Aplicar Fluent preto nos atalhos" }));
    expect(api.lojaApplyShortcuts).not.toHaveBeenCalled();

    vi.mocked(api.lojaApplyShortcuts).mockResolvedValue({ id: "i1", changed: 7, skipped: 2, failed: [], restart_explorer: false });
    l.mockResolvedValueOnce({ ...page([pack]), shortcuts_active: "i1" });
    await user.click(await screen.findByRole("button", { name: "Trocar ícones" }));
    expect(api.lojaApplyShortcuts).toHaveBeenCalledWith("i1", false);
    await waitFor(() => expect(screen.getByText(/7 atalho\(s\)/)).toBeTruthy());

    vi.mocked(api.lojaRevertShortcuts).mockResolvedValue({ id: "i1", restored: 7, failed: [], restart_explorer: false });
    l.mockResolvedValueOnce(page([pack]));
    await user.click(await screen.findByRole("button", { name: "Restaurar atalhos" }));
    expect(api.lojaRevertShortcuts).toHaveBeenCalled();
  });

  it("filtra por categoria e mostra erro de rede sem esconder o catálogo", async () => {
    const user = userEvent.setup();
    vi.mocked(api.lojaList).mockResolvedValue({
      items: [item({ id: "c1", name: "Noite" }), item({ id: "i1", kind: "icones", name: "Flat" })],
      online: false,
      online_error: "falha de rede",
      shortcuts_active: null,
    });
    render(<Loja />);
    expect(await screen.findByText("Noite")).toBeTruthy();
    expect(screen.getByText(/indisponível/)).toBeTruthy();
    await user.click(screen.getByRole("tab", { name: "Pacotes de ícones" }));
    expect(screen.queryByText("Noite")).toBeNull();
    expect(screen.getByText("Flat")).toBeTruthy();
  });

  it("muda o tamanho do ponteiro e desfaz", async () => {
    const user = userEvent.setup();
    vi.mocked(api.lojaList).mockResolvedValue(page([item({ id: "c1", name: "Noite" })]));
    render(<Loja />);
    await user.click(await screen.findByRole("button", { name: "Pequeno" }));
    await waitFor(() => expect(api.lojaSetCursorSize).toHaveBeenCalledWith(24));
    vi.mocked(api.lojaCursorSize).mockResolvedValue(24);
    await user.click(await screen.findByRole("button", { name: "Desfazer tamanho" }));
    await waitFor(() => expect(api.lojaSetCursorSize).toHaveBeenCalledWith(null));
  });
});
