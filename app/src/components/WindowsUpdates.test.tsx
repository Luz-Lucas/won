import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import type { UpdatesPage } from "../api";
import WindowsUpdates from "./WindowsUpdates";

vi.mock("../api", () => ({
  updatesStatus: vi.fn(),
  updatesPreview: vi.fn(),
  updatesSetMode: vi.fn(),
  updatesResume: vi.fn(),
}));

const page = (over: Partial<UpdatesPage> = {}): UpdatesPage => ({
  state: { mode: "default", since: null, paused_until: null, reverted: [] },
  days: null,
  elevated: true,
  version: "Windows 11 25H2",
  suspended: null,
  defender: { last_attempt: null, last_ok: null, last_error: null },
  ...over,
});

describe("WindowsUpdates", () => {
  beforeEach(() => {
    for (const f of [api.updatesStatus, api.updatesPreview, api.updatesSetMode, api.updatesResume])
      vi.mocked(f).mockReset();
  });

  it("mode_selector_shows_preview_and_warnings", async () => {
    vi.mocked(api.updatesStatus).mockResolvedValue(page());
    vi.mocked(api.updatesPreview).mockResolvedValue([
      { description: "Serviço wuauserv (inicialização)", current: "Manual", target: "Desativado" },
    ]);
    vi.mocked(api.updatesSetMode).mockResolvedValue(
      page({ state: { mode: "blocked", since: "2026-11-23T10:00:00Z", paused_until: null, reverted: [] } }),
    );
    render(<WindowsUpdates />);
    expect(await screen.findByText("Versão instalada: Windows 11 25H2")).toBeInTheDocument();
    const apply = screen.getByRole("button", { name: "Aplicar modo" });
    expect(apply).toBeDisabled(); // nada mudou ainda
    await userEvent.click(screen.getByRole("radio", { name: /Bloqueado/ }));
    await userEvent.click(apply);
    expect(api.updatesPreview).toHaveBeenCalledWith("blocked", null);
    const dialog = await screen.findByRole("dialog", {
      name: 'Mudar as atualizações para "Bloqueado"?',
    });
    expect(dialog).toHaveTextContent("Serviço wuauserv (inicialização): Manual → Desativado");
    expect(dialog).toHaveTextContent(/fica sem correções de segurança/);
    expect(dialog).toHaveTextContent(/Microsoft Store/);
    expect(api.updatesSetMode).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Aplicar" }));
    expect(api.updatesSetMode).toHaveBeenCalledWith("blocked", null);
    await waitFor(() =>
      expect(screen.getByRole("radio", { name: /Bloqueado/ })).toBeChecked(),
    );
    expect(
      screen.getByText(/atualiza as definições do Defender uma vez por dia/),
    ).toBeInTheDocument();
  });

  it("paused sends the chosen date", async () => {
    vi.mocked(api.updatesStatus).mockResolvedValue(page());
    vi.mocked(api.updatesPreview).mockResolvedValue([]);
    vi.mocked(api.updatesSetMode).mockResolvedValue(page());
    render(<WindowsUpdates />);
    await userEvent.click(await screen.findByRole("radio", { name: /Pausado até/ }));
    const date = screen.getByLabelText("Pausar até") as HTMLInputElement;
    await userEvent.clear(date);
    await userEvent.type(date, "2026-12-01");
    await userEvent.click(screen.getByRole("button", { name: "Aplicar modo" }));
    expect(api.updatesPreview).toHaveBeenCalledWith("paused", "2026-12-01");
    expect(
      await screen.findByRole("dialog", { name: "Pausar as atualizações até 2026-12-01?" }),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Aplicar" }));
    expect(api.updatesSetMode).toHaveBeenCalledWith("paused", "2026-12-01");
  });

  it("blocked_requires_admin", async () => {
    vi.mocked(api.updatesStatus).mockResolvedValue(page({ elevated: false }));
    render(<WindowsUpdates />);
    expect(await screen.findByText(/exige administrador/)).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /Bloqueado/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Aplicar modo" })).toBeDisabled();
  });

  it("partial_lists_what_came_back", async () => {
    vi.mocked(api.updatesStatus).mockResolvedValue(
      page({
        state: {
          mode: "blocked",
          since: "2026-11-01T10:00:00Z",
          paused_until: null,
          reverted: ["Serviço UsoSvc (inicialização): Automático"],
        },
      }),
    );
    vi.mocked(api.updatesSetMode).mockResolvedValue(page());
    render(<WindowsUpdates />);
    expect(
      await screen.findByText("Parcial: o Windows desfez parte do modo Bloqueado:"),
    ).toBeInTheDocument();
    expect(screen.getByText("Serviço UsoSvc (inicialização): Automático")).toBeInTheDocument();
    // Nada é reaplicado sozinho: só pelo botão.
    expect(api.updatesSetMode).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Aplicar de novo" }));
    expect(api.updatesSetMode).toHaveBeenCalledWith("blocked", null);
  });

  it("suspended mode offers to go back", async () => {
    vi.mocked(api.updatesStatus).mockResolvedValue(
      page({ suspended: { mode: "blocked", until: null } }),
    );
    vi.mocked(api.updatesResume).mockResolvedValue(page());
    render(<WindowsUpdates />);
    await userEvent.click(await screen.findByRole("button", { name: "Voltar a Bloqueado" }));
    expect(api.updatesResume).toHaveBeenCalled();
  });

  it("shows the error when applying fails", async () => {
    vi.mocked(api.updatesStatus).mockResolvedValue(page());
    vi.mocked(api.updatesPreview).mockResolvedValue([]);
    vi.mocked(api.updatesSetMode).mockRejectedValue("falhou: acesso negado");
    render(<WindowsUpdates />);
    await userEvent.click(await screen.findByRole("radio", { name: /Só segurança/ }));
    await userEvent.click(screen.getByRole("button", { name: "Aplicar modo" }));
    await userEvent.click(await screen.findByRole("button", { name: "Aplicar" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("falhou: acesso negado");
  });
});
