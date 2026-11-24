import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import type { QuarantineEntry } from "../api";
import Quarentena from "./Quarentena";

vi.mock("../api", () => ({
  quarantineList: vi.fn(),
  quarantineRestore: vi.fn(),
  quarantineDelete: vi.fn(),
}));

const entry = (over: Partial<QuarantineEntry> = {}): QuarantineEntry => ({
  id: "abc",
  original_path: "C:\\x\\mal.exe",
  sha256: "00",
  size: 2048,
  rule: "EICAR_Test_File",
  quarantined_at: "2026-03-05T12:00:00Z",
  ...over,
});

const confirm = async (button: string, confirmLabel: string) => {
  await userEvent.click(await screen.findByRole("button", { name: button }));
  await userEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: confirmLabel }));
};

describe("Quarentena", () => {
  beforeEach(() => {
    vi.mocked(api.quarantineList).mockReset().mockResolvedValue([]);
    vi.mocked(api.quarantineRestore).mockReset();
    vi.mocked(api.quarantineDelete).mockReset();
  });

  it("quarentena vazia mostra o estado vazio", async () => {
    render(<Quarentena />);
    expect(await screen.findByText("Nenhum arquivo em quarentena.")).toBeInTheDocument();
  });

  it("restaura e mostra o erro do restore como está", async () => {
    vi.mocked(api.quarantineList).mockResolvedValue([entry()]);
    vi.mocked(api.quarantineRestore).mockRejectedValueOnce("o destino já existe");
    render(<Quarentena />);
    expect(await screen.findByText("C:\\x\\mal.exe")).toBeInTheDocument();
    expect(screen.getByText(/EICAR_Test_File/)).toBeInTheDocument();
    await confirm("Restaurar", "Restaurar");
    expect(api.quarantineRestore).toHaveBeenCalledWith("abc");
    expect(await screen.findByRole("alert")).toHaveTextContent("o destino já existe");
    vi.mocked(api.quarantineRestore).mockResolvedValueOnce("C:\\x\\mal.exe");
    vi.mocked(api.quarantineList).mockResolvedValue([]);
    await confirm("Restaurar", "Restaurar");
    expect(await screen.findByText("Nenhum arquivo em quarentena.")).toBeInTheDocument();
    expect(screen.getByText("Restaurado: C:\\x\\mal.exe")).toBeInTheDocument();
  });

  it("Restaurar pede confirmação com o caminho e a regra", async () => {
    vi.mocked(api.quarantineList).mockResolvedValue([entry()]);
    vi.mocked(api.quarantineRestore).mockResolvedValue("C:\\x\\mal.exe");
    const onRestored = vi.fn();
    render(<Quarentena onRestored={onRestored} />);
    await userEvent.click(await screen.findByRole("button", { name: "Restaurar" }));
    expect(api.quarantineRestore).not.toHaveBeenCalled();
    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveTextContent(
      "Restaurar C:\\x\\mal.exe? Este arquivo foi colocado em quarentena por: EICAR_Test_File.",
    );
    await userEvent.click(within(dialog).getByRole("button", { name: "Restaurar" }));
    await screen.findByText("Restaurado: C:\\x\\mal.exe");
    expect(onRestored).toHaveBeenCalledTimes(1);
  });

  it("delete_needs_confirmation", async () => {
    vi.mocked(api.quarantineList).mockResolvedValue([entry()]);
    vi.mocked(api.quarantineDelete).mockResolvedValue("C:\\x\\mal.exe");
    render(<Quarentena />);
    await userEvent.click(await screen.findByRole("button", { name: "Apagar" }));
    const dialog = screen.getByRole("dialog", { name: "Apagar definitivamente?" });
    expect(dialog).toHaveTextContent("Não tem volta");
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancelar" }));
    expect(api.quarantineDelete).not.toHaveBeenCalled();
    vi.mocked(api.quarantineList).mockResolvedValue([]);
    await confirm("Apagar", "Apagar definitivamente");
    expect(api.quarantineDelete).toHaveBeenCalledWith("abc");
    expect(await screen.findByText("Apagado definitivamente: C:\\x\\mal.exe")).toBeInTheDocument();
  });

  it("Cancelar e Escape não restauram", async () => {
    vi.mocked(api.quarantineList).mockResolvedValue([entry()]);
    render(<Quarentena />);
    await userEvent.click(await screen.findByRole("button", { name: "Restaurar" }));
    await userEvent.click(screen.getByRole("button", { name: "Cancelar" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Restaurar" }));
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(api.quarantineRestore).not.toHaveBeenCalled();
  });
});
