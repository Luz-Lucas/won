import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { SystemStatus } from "../api";
import Inicio from "./Inicio";

const status = (over: Partial<SystemStatus> = {}): SystemStatus => ({
  version: "0.3.0",
  elevated: false,
  db: { dir: "d", initialized: true, sources: [], warnings: [] },
  quarantine_count: 0,
  activity: [],
  tweaks_applied: 3,
  tweaks_total: 12,
  ...over,
});

describe("Inicio", () => {
  it("optimization_card_shows_counts", async () => {
    const onNavigate = vi.fn();
    render(<Inicio status={status()} error={null} onNavigate={onNavigate} />);
    expect(screen.getByRole("heading", { name: "Otimização" })).toBeVisible();
    expect(screen.getByText("3 de 12")).toBeVisible();
    await userEvent.click(screen.getByRole("button", { name: "Ir para Jogos" }));
    expect(onNavigate).toHaveBeenCalledWith("jogos");
  });

  it("version_moves_to_footer", () => {
    render(<Inicio status={status()} error={null} onNavigate={() => {}} />);
    expect(screen.getByText("Sentinela 0.3.0")).toBeVisible();
    expect(screen.queryByText("Versão 0.3.0")).toBeNull();
  });
});
