import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import type { StartupItem, StartupPage } from "../api";
import StartupList from "./StartupList";

vi.mock("../api", () => ({
  startupList: vi.fn(),
  startupSetEnabled: vi.fn(),
}));

const item = (id: string, over: Partial<StartupItem> = {}): StartupItem => ({
  id,
  name: `Programa ${id}`,
  command: `C:\\Apps\\${id}.exe --silent`,
  source: "run_hkcu",
  enabled: true,
  file_exists: true,
  needs_admin: false,
  sentinela: false,
  locked: null,
  ...over,
});

const page = (items: StartupItem[]): StartupPage => ({
  items,
  warnings: [],
  elevated: false,
});

describe("StartupList", () => {
  beforeEach(() => {
    vi.mocked(api.startupList).mockReset();
    vi.mocked(api.startupSetEnabled).mockReset();
  });

  it("disable_asks_confirmation", async () => {
    vi.mocked(api.startupList).mockResolvedValue(page([item("a")]));
    vi.mocked(api.startupSetEnabled).mockResolvedValue(
      page([item("a", { enabled: false })]),
    );
    render(<StartupList />);
    const sw = await screen.findByRole("switch", { name: "Programa a" });
    expect(sw).toBeChecked();
    await userEvent.click(sw);
    expect(api.startupSetEnabled).not.toHaveBeenCalled();
    expect(
      screen.getByRole("dialog", {
        name: 'Desativar "Programa a" na inicialização? O programa continua instalado.',
      }),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Desativar" }));
    expect(api.startupSetEnabled).toHaveBeenCalledWith("a", false);
    await waitFor(() =>
      expect(screen.getByRole("switch", { name: "Programa a" })).not.toBeChecked(),
    );
  });

  it("cancelling keeps the entry enabled", async () => {
    vi.mocked(api.startupList).mockResolvedValue(page([item("a")]));
    render(<StartupList />);
    await userEvent.click(await screen.findByRole("switch", { name: "Programa a" }));
    await userEvent.click(screen.getByRole("button", { name: "Cancelar" }));
    expect(api.startupSetEnabled).not.toHaveBeenCalled();
  });

  it("enabling again needs no confirmation", async () => {
    vi.mocked(api.startupList).mockResolvedValue(page([item("a", { enabled: false })]));
    vi.mocked(api.startupSetEnabled).mockResolvedValue(page([item("a")]));
    render(<StartupList />);
    await userEvent.click(await screen.findByRole("switch", { name: "Programa a" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(api.startupSetEnabled).toHaveBeenCalledWith("a", true);
  });

  it("locked_without_admin", async () => {
    vi.mocked(api.startupList).mockResolvedValue(
      page([
        item("m", {
          source: "run_hklm",
          needs_admin: true,
          locked: "Requer administrador: vale para todos os usuários.",
        }),
      ]),
    );
    render(<StartupList />);
    expect(await screen.findByRole("switch", { name: "Programa m" })).toBeDisabled();
    expect(screen.getByText("🔒 Bloqueado")).toBeInTheDocument();
    expect(
      screen.getByText("Requer administrador: vale para todos os usuários."),
    ).toBeInTheDocument();
    expect(screen.getByText("Registro (todos os usuários)")).toBeInTheDocument();
  });

  it("missing_file_label", async () => {
    vi.mocked(api.startupList).mockResolvedValue(
      page([item("x", { file_exists: false }), item("ok")]),
    );
    render(<StartupList />);
    await screen.findByRole("switch", { name: "Programa x" });
    expect(screen.getAllByText("Arquivo não encontrado")).toHaveLength(1);
    // Continua com interruptor: pode ser desativada, nunca apagada.
    expect(screen.getByRole("switch", { name: "Programa x" })).toBeEnabled();
  });

  it("sentinela entry has no switch", async () => {
    vi.mocked(api.startupList).mockResolvedValue(
      page([item("s", { name: "Sentinela", sentinela: true })]),
    );
    render(<StartupList />);
    expect(
      await screen.findByText(
        "Este é o WON. Para desligar, use Ajustes → Inicialização.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("switch")).not.toBeInTheDocument();
  });

  it("long commands are shortened with the full text as a hint", async () => {
    const long = `C:\\${"pasta\\".repeat(20)}app.exe`;
    vi.mocked(api.startupList).mockResolvedValue(page([item("l", { command: long })]));
    render(<StartupList />);
    const shown = await screen.findByTitle(long);
    expect(shown.textContent!.length).toBeLessThanOrEqual(60);
    expect(shown.textContent!.endsWith("…")).toBe(true);
  });

  it("shows the error when toggling fails", async () => {
    vi.mocked(api.startupList).mockResolvedValue(page([item("a", { enabled: false })]));
    vi.mocked(api.startupSetEnabled).mockRejectedValue("acesso negado");
    render(<StartupList />);
    await userEvent.click(await screen.findByRole("switch", { name: "Programa a" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("acesso negado");
  });
});
