import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import MemoryOptimizer from "./MemoryOptimizer";

vi.mock("../api", () => ({
  ramStatus: vi.fn(),
  ramConfigGet: vi.fn(),
  ramConfigSet: vi.fn(),
  ramOptimize: vi.fn(),
}));

const cfg = { auto: false, threshold: 85, clear_standby: true, exclude: [] as string[] };

describe("MemoryOptimizer", () => {
  beforeEach(() => {
    vi.mocked(api.ramStatus).mockResolvedValue({ total_mb: 16384, used_mb: 12288, available_mb: 4096, percent: 75 });
    vi.mocked(api.ramConfigGet).mockResolvedValue(cfg);
    vi.mocked(api.ramConfigSet).mockImplementation(async (c) => c);
  });

  it("mostra o uso e o antes/depois ao otimizar", async () => {
    vi.mocked(api.ramOptimize).mockResolvedValue({
      used_before_mb: 12288, used_after_mb: 10000, freed_mb: 2288, trimmed: 40, skipped: 10,
      standby_cleared: false, standby_note: "precisa de administrador",
    });
    render(<MemoryOptimizer />);
    expect(await screen.findByText("75%")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Otimizar agora" }));
    expect(await screen.findByText(/Liberou 2288 MB/)).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent(/precisa de administrador/);
  });

  it("liga o modo automático e protege um programa", async () => {
    render(<MemoryOptimizer />);
    await userEvent.click(await screen.findByRole("checkbox", { name: /automaticamente/ }));
    await waitFor(() => expect(api.ramConfigSet).toHaveBeenCalledWith({ ...cfg, auto: true }));
    await userEvent.type(screen.getByLabelText("Nome do programa a proteger"), "game.exe");
    await userEvent.click(screen.getByRole("button", { name: "Adicionar" }));
    await waitFor(() =>
      expect(api.ramConfigSet).toHaveBeenLastCalledWith(expect.objectContaining({ exclude: ["game.exe"] })),
    );
  });
});
