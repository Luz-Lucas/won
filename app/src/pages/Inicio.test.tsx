import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import type { SystemStatus } from "../api";
import Inicio, { updatesNotice } from "./Inicio";

vi.mock("../api", () => ({
  updatesUpdateNow: vi.fn(),
}));

const status = (over: Partial<SystemStatus> = {}): SystemStatus => ({
  version: "0.3.0",
  elevated: false,
  db: { dir: "d", initialized: true, sources: [], warnings: [] },
  quarantine_count: 0,
  activity: [],
  tweaks_applied: 3,
  tweaks_total: 12,
  updates_mode: "default",
  updates_days: null,
  updates_suspended: null,
  ...over,
});

describe("Inicio", () => {
  beforeEach(() => {
    vi.mocked(api.updatesUpdateNow).mockReset();
  });

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

  it("home_shows_days_without_updates", () => {
    render(
      <Inicio
        status={status({ updates_mode: "blocked", updates_days: 12 })}
        error={null}
        onNavigate={() => {}}
      />,
    );
    expect(
      screen.getByText(/Atualizações do Windows bloqueadas há 12 dias\./),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "Atualizar agora" })).toBeVisible();
  });

  it("no notice in the default mode", () => {
    render(<Inicio status={status()} error={null} onNavigate={() => {}} />);
    expect(screen.queryByRole("button", { name: "Atualizar agora" })).toBeNull();
  });

  it("notice text covers today, one day and paused", () => {
    expect(updatesNotice("paused", 0)).toBe("Atualizações do Windows pausadas desde hoje.");
    expect(updatesNotice("blocked", 1)).toBe("Atualizações do Windows bloqueadas há 1 dia.");
    expect(updatesNotice("paused", null)).toBe("Atualizações do Windows pausadas.");
  });

  it("update_now_undoes_and_offers_return", async () => {
    vi.mocked(api.updatesUpdateNow).mockResolvedValue({} as never);
    const onChanged = vi.fn();
    const onNavigate = vi.fn();
    const { rerender } = render(
      <Inicio
        status={status({ updates_mode: "paused", updates_days: 3 })}
        error={null}
        onNavigate={onNavigate}
        onChanged={onChanged}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Atualizar agora" }));
    // Confirma antes de liberar.
    expect(api.updatesUpdateNow).not.toHaveBeenCalled();
    expect(
      screen.getByRole("dialog", { name: "Liberar as atualizações agora?" }),
    ).toHaveTextContent(/volte ao modo Pausado em Ajustes/);
    await userEvent.click(
      screen.getAllByRole("button", { name: "Atualizar agora" }).slice(-1)[0],
    );
    expect(api.updatesUpdateNow).toHaveBeenCalledTimes(1);
    expect(onChanged).toHaveBeenCalled();
    // Depois de liberar: o aviso leva de volta ao modo em Ajustes.
    rerender(
      <Inicio
        status={status({ updates_suspended: "paused" })}
        error={null}
        onNavigate={onNavigate}
        onChanged={onChanged}
      />,
    );
    expect(screen.getByText(/liberadas temporariamente/)).toBeVisible();
    await userEvent.click(screen.getByRole("button", { name: "Ir para Ajustes" }));
    expect(onNavigate).toHaveBeenCalledWith("ajustes");
  });

  it("update now without admin shows the error", async () => {
    vi.mocked(api.updatesUpdateNow).mockRejectedValue("exige administrador");
    render(
      <Inicio
        status={status({ updates_mode: "blocked", updates_days: 2 })}
        error={null}
        onNavigate={() => {}}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Atualizar agora" }));
    await userEvent.click(
      screen.getAllByRole("button", { name: "Atualizar agora" }).slice(-1)[0],
    );
    expect(await screen.findByRole("alert")).toHaveTextContent("exige administrador");
  });
});
