import { render as rtlRender, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import Personalizar from "./Personalizar";
import { JobsProvider } from "../jobs";

const render = (ui: React.ReactElement) => rtlRender(<JobsProvider>{ui}</JobsProvider>);

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: () => Promise.resolve(() => {}) }));

vi.mock("../api", () => ({
  tweaksList: vi.fn().mockResolvedValue({ tweaks: [], elevated: true, hkcu_allowed: true }),
  tweaksApply: vi.fn(),
  tweaksApplyChoice: vi.fn(),
  tweaksUndo: vi.fn(),
  restartExplorer: vi.fn(),
  relaunchElevated: vi.fn(),
  jobCancel: vi.fn(),
  jobRunning: vi.fn().mockResolvedValue(null),
  wallpaperStatus: vi.fn(),
  wallpaperGallery: vi.fn(),
  wallpaperOpen: vi.fn(),
  wallpaperPauseModeSet: vi.fn(),
  themeList: vi.fn().mockResolvedValue({ themes: [], history: [] }),
  themePreview: vi.fn(),
  themeApply: vi.fn(),
  themeUndo: vi.fn(),
  themeExport: vi.fn(),
  themeImport: vi.fn(),
  terminalStatus: vi.fn(),
  terminalInstallOmp: vi.fn(),
  terminalInstallFont: vi.fn(),
  terminalThemes: vi.fn(),
  terminalThemePreview: vi.fn(),
  terminalTiming: vi.fn(),
  terminalPlan: vi.fn(),
  terminalApply: vi.fn(),
  terminalDisablePrompt: vi.fn(),
  terminalHistory: vi.fn(),
  terminalUndo: vi.fn(),
}));

const m = vi.mocked(api);

const status = (over: Partial<api.TerminalStatus> = {}): api.TerminalStatus => ({
  installed: true,
  version: "24.0.0",
  update_available: false,
  profiles: [{ shell: "PowerShell 7", path: "C:\\p.ps1", active: false }],
  windows_terminal: true,
  fonts: [
    { name: "Meslo", label: "Meslo (padrão)" },
    { name: "FiraCode", label: "Fira Code" },
  ],
  can_undo: false,
  ...over,
});

const plan: api.TerminalPlan = {
  changes: [
    {
      label: "Perfil do PowerShell 7",
      path: "C:\\p.ps1",
      is_new: false,
      lines: [
        [true, "# >>> sentinela >>>"],
        [true, "# <<< sentinela <<<"],
      ],
    },
  ],
  warnings: [],
  disabled: ["tema: segmento command"],
};

async function openTerminal() {
  render(<Personalizar />);
  await userEvent.click(await screen.findByRole("button", { name: "Terminal" }));
}

beforeEach(() => {
  vi.clearAllMocks();
  m.wallpaperStatus.mockResolvedValue({ installed: false, running: false, build: null, pause_mode: "pause" });
  m.terminalStatus.mockResolvedValue(status());
  m.terminalThemes.mockResolvedValue(["agnoster", "jandedobbeleer"]);
  m.terminalThemePreview.mockResolvedValue("~/proj  main");
  m.terminalHistory.mockResolvedValue([]);
});

describe("Terminal em Personalizar", () => {
  it("terminal_hidden_until_checked", async () => {
    let resolve!: (s: api.TerminalStatus) => void;
    m.terminalStatus.mockReturnValue(new Promise((r) => (resolve = r)));
    render(<Personalizar />);
    await screen.findByRole("button", { name: "Temas" });
    expect(screen.queryByRole("button", { name: "Terminal" })).toBeNull();
    resolve(status());
    expect(await screen.findByRole("button", { name: "Terminal" })).toBeTruthy();
  });

  it("stays hidden without PowerShell or Windows Terminal", async () => {
    m.terminalStatus.mockResolvedValue(status({ profiles: [], windows_terminal: false }));
    render(<Personalizar />);
    await screen.findByRole("button", { name: "Temas" });
    await waitFor(() => expect(m.terminalStatus).toHaveBeenCalled());
    expect(screen.queryByRole("button", { name: "Terminal" })).toBeNull();
  });

  it("shows_diff_before_apply", async () => {
    m.terminalPlan.mockResolvedValue(plan);
    m.terminalApply.mockResolvedValue({ changed: 1, errors: [], warnings: [] });
    await openTerminal();
    await userEvent.selectOptions(await screen.findByLabelText("Tema"), "agnoster");
    expect(await screen.findByLabelText("Amostra do prompt")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Ver o que muda" }));
    const dlg = await screen.findByRole("dialog");
    expect(within(dlg).getByLabelText("Mudanças em Perfil do PowerShell 7").textContent).toContain(
      "+ # >>> sentinela >>>",
    );
    expect(within(dlg).getByText(/Desligados por segurança/)).toBeTruthy();
    expect(m.terminalApply).not.toHaveBeenCalled();
    await userEvent.click(within(dlg).getByRole("button", { name: "Aplicar" }));
    await waitFor(() => expect(m.terminalApply).toHaveBeenCalledTimes(1));
    expect(m.terminalApply.mock.calls[0]![0]).toMatchObject({ theme: "agnoster", font: null });
  });

  it("font_and_theme_install_confirmations", async () => {
    m.terminalInstallFont.mockResolvedValue(undefined);
    await openTerminal();
    await userEvent.click(await screen.findByRole("button", { name: "Instalar Fira Code" }));
    const dlg = await screen.findByRole("dialog");
    expect(m.terminalInstallFont).not.toHaveBeenCalled();
    await userEvent.click(within(dlg).getByRole("button", { name: "Cancelar" }));
    expect(m.terminalInstallFont).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Instalar Fira Code" }));
    await userEvent.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Instalar" }));
    await waitFor(() => expect(m.terminalInstallFont).toHaveBeenCalledWith("FiraCode"));
  });

  it("offers install when Oh My Posh is missing", async () => {
    m.terminalStatus.mockResolvedValue(status({ installed: false, version: null }));
    m.terminalInstallOmp.mockResolvedValue("instalado");
    await openTerminal();
    await userEvent.click(await screen.findByRole("button", { name: "Instalar o Oh My Posh" }));
    expect(m.terminalInstallOmp).not.toHaveBeenCalled();
    await userEvent.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Instalar" }));
    await waitFor(() => expect(m.terminalInstallOmp).toHaveBeenCalledWith(false));
  });

  it("undo_from_history", async () => {
    m.terminalHistory.mockResolvedValue([
      { seq: 2, label: "Perfil do PowerShell 7", path: "C:\\p.ps1", applied_at: "x" },
    ]);
    m.terminalUndo.mockResolvedValueOnce({ undone: null, changed_after: "C:\\p.ps1" });
    m.terminalUndo.mockResolvedValueOnce({
      undone: { seq: 2, label: "Perfil do PowerShell 7", path: "C:\\p.ps1", applied_at: "x" },
      changed_after: null,
    });
    await openTerminal();
    await userEvent.click(await screen.findByRole("button", { name: "Desfazer a última alteração" }));
    await waitFor(() => expect(m.terminalUndo).toHaveBeenCalledWith(false));
    const dlg = await screen.findByRole("dialog");
    expect(dlg.textContent).toContain("foi editado depois");
    await userEvent.click(within(dlg).getByRole("button", { name: "Desfazer mesmo assim" }));
    await waitFor(() => expect(m.terminalUndo).toHaveBeenLastCalledWith(true));
    expect(await screen.findByText(/Desfeito: Perfil do PowerShell 7/)).toBeTruthy();
  });
});
