import { render as rtlRender, screen, waitFor } from "@testing-library/react";
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
  terminalStatus: vi.fn().mockRejectedValue("sem terminal"),
  desktopStatus: vi.fn(),
  desktopStart: vi.fn(),
  desktopStop: vi.fn(),
  desktopConfigRead: vi.fn(),
  desktopConfigCheck: vi.fn(),
  desktopConfigWrite: vi.fn(),
}));

const m = vi.mocked(api);

const status = (over: Partial<api.DesktopStatus> = {}): api.DesktopStatus => ({
  installed: true,
  running: false,
  config_path: "C:\\Users\\x\\AppData\\Roaming\\Sentinela\\desktop.toml",
  config_exists: true,
  errors: [],
  hotkeys: 1,
  rules: 0,
  ...over,
});

async function openDesktop() {
  render(<Personalizar />);
  await userEvent.click(await screen.findByRole("button", { name: "Desktop" }));
}

beforeEach(() => {
  vi.clearAllMocks();
  m.wallpaperStatus.mockResolvedValue({ installed: false, running: false, build: null, pause_mode: "pause" });
  m.desktopStatus.mockResolvedValue(status());
  m.desktopConfigRead.mockResolvedValue("# vazio\n");
  m.desktopConfigCheck.mockResolvedValue([]);
});

describe("Desktop em Personalizar", () => {
  it("hidden_when_program_is_missing", async () => {
    m.desktopStatus.mockResolvedValue(status({ installed: false }));
    render(<Personalizar />);
    await screen.findByRole("button", { name: "Temas" });
    await waitFor(() => expect(m.desktopStatus).toHaveBeenCalled());
    expect(screen.queryByRole("button", { name: "Desktop" })).toBeNull();
  });

  it("start_and_stop", async () => {
    m.desktopStart.mockResolvedValue(undefined);
    await openDesktop();
    m.desktopStatus.mockResolvedValue(status({ running: true }));
    await userEvent.click(await screen.findByRole("button", { name: "Iniciar" }));
    await waitFor(() => expect(m.desktopStart).toHaveBeenCalledTimes(1));
    m.desktopStop.mockResolvedValue(undefined);
    await userEvent.click(await screen.findByRole("button", { name: "Parar" }));
    await waitFor(() => expect(m.desktopStop).toHaveBeenCalledTimes(1));
  });

  it("save_is_blocked_while_the_file_has_problems", async () => {
    await openDesktop();
    const box = await screen.findByLabelText("desktop.toml");
    m.desktopConfigCheck.mockResolvedValue(["regra 1: opacity deve ficar entre 20 e 100"]);
    await userEvent.type(box, "x");
    expect(await screen.findByText(/opacity deve ficar/)).toBeTruthy();
    expect((screen.getByRole("button", { name: "Salvar" }) as HTMLButtonElement).disabled).toBe(true);
    expect(m.desktopConfigWrite).not.toHaveBeenCalled();
  });

  it("saves_a_valid_edit", async () => {
    m.desktopConfigWrite.mockResolvedValue(undefined);
    await openDesktop();
    const box = await screen.findByLabelText("desktop.toml");
    expect((screen.getByRole("button", { name: "Salvar" }) as HTMLButtonElement).disabled).toBe(true);
    await userEvent.type(box, "# a");
    await userEvent.click(screen.getByRole("button", { name: "Salvar" }));
    await waitFor(() => expect(m.desktopConfigWrite).toHaveBeenCalledWith("# vazio\n# a"));
    expect(await screen.findByText(/Salvo\./)).toBeTruthy();
  });

  it("shows_problems_of_the_current_file", async () => {
    m.desktopStatus.mockResolvedValue(status({ errors: ["TOML inválido: linha 3"] }));
    await openDesktop();
    expect(await screen.findByText(/TOML inválido: linha 3/)).toBeTruthy();
    expect(screen.getByText(/continua com a configuração anterior/)).toBeTruthy();
  });
});
