import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import ScanSettings from "./ScanSettings";

vi.mock("../api", () => ({
  scanScheduleGet: vi.fn(),
  scanScheduleSet: vi.fn(),
  scanAutoQuarantineSet: vi.fn(),
  scanExceptionsList: vi.fn(),
  scanExceptionRemove: vi.fn(),
}));

const view = (over = {}) => ({ scheduled: false, auto_quarantine: false, offered: true, ...over });

describe("ScanSettings", () => {
  beforeEach(() => {
    vi.mocked(api.scanScheduleGet).mockReset().mockResolvedValue(view());
    vi.mocked(api.scanExceptionsList).mockReset().mockResolvedValue([]);
    vi.mocked(api.scanScheduleSet).mockReset();
    vi.mocked(api.scanAutoQuarantineSet).mockReset();
    vi.mocked(api.scanExceptionRemove).mockReset();
  });

  it("liga a verificação semanal", async () => {
    vi.mocked(api.scanScheduleSet).mockResolvedValue(view({ scheduled: true }));
    render(<ScanSettings />);
    const box = await screen.findByLabelText("Verificação rápida toda semana");
    expect(box).not.toBeChecked();
    await userEvent.click(box);
    expect(api.scanScheduleSet).toHaveBeenCalledWith(true);
    expect(await screen.findByLabelText("Verificação rápida toda semana")).toBeChecked();
  });

  it("quarentena automática vem desligada e liga", async () => {
    vi.mocked(api.scanAutoQuarantineSet).mockResolvedValue(view({ auto_quarantine: true }));
    render(<ScanSettings />);
    const box = await screen.findByLabelText(
      "Colocar malware confirmado por hash em quarentena automaticamente",
    );
    expect(box).not.toBeChecked();
    await userEvent.click(box);
    expect(api.scanAutoQuarantineSet).toHaveBeenCalledWith(true);
  });

  it("exceptions_in_settings", async () => {
    const sha = "c".repeat(64);
    vi.mocked(api.scanExceptionsList).mockResolvedValue([
      { sha256: sha, path: "C:\\jogo\\cheat.exe", rule: "pe-heuristics", added_at: "2026-11-20T10:00:00Z" },
    ]);
    vi.mocked(api.scanExceptionRemove).mockResolvedValue([]);
    render(<ScanSettings />);
    expect(await screen.findByText("C:\\jogo\\cheat.exe")).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Voltar a verificar C:\\jogo\\cheat.exe" }),
    );
    expect(api.scanExceptionRemove).toHaveBeenCalledWith(sha);
    expect(await screen.findByText('Nenhum arquivo em "Ignorar sempre".')).toBeInTheDocument();
  });
});
