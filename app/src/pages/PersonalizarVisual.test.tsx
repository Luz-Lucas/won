import { render as rtlRender, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import Personalizar from "./Personalizar";
import { JobsProvider } from "../jobs";

const render = (ui: React.ReactElement) =>
  rtlRender(<JobsProvider>{ui}</JobsProvider>);

const openDialog = vi.fn();
const saveDialog = vi.fn();
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...a: unknown[]) => openDialog(...a),
  save: (...a: unknown[]) => saveDialog(...a),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

vi.mock("../api", () => ({
  systemFontStatus: vi.fn().mockResolvedValue(undefined),
  systemFontSet: vi.fn(),
  shellModsStatus: vi.fn().mockResolvedValue(undefined),
  shellModSet: vi.fn(),
  xamlCfgWrite: vi.fn(),
  tweaksList: vi.fn(),
  tweaksApply: vi.fn(),
  tweaksApplyChoice: vi.fn(),
  tweaksUndo: vi.fn(),
  restartExplorer: vi.fn(),
  relaunchElevated: vi.fn(),
  jobCancel: vi.fn(),
  jobRunning: vi.fn().mockResolvedValue(null),
  wallpaperStatus: vi.fn(),
  desktopStatus: vi.fn().mockRejectedValue("sem desktop"),
  terminalStatus: vi.fn().mockRejectedValue("sem terminal"),
  wallpaperGallery: vi.fn(),
  wallpaperOpen: vi.fn(),
  wallpaperMonitors: vi.fn(),
  wallpaperPauseModeSet: vi.fn(),
}));

const status = (installed: boolean, pause_mode: api.WallpaperPauseMode = "pause") => ({
  installed,
  running: installed,
  build: null,
  pause_mode,
});


const goTo = async (name: string) => {
  await userEvent.click(await screen.findByRole("button", { name }));
};

describe("Personalizar: papel de parede", () => {
  beforeEach(() => {
    for (const f of Object.values(api)) if (vi.isMockFunction(f)) f.mockReset();
    vi.mocked(api.terminalStatus).mockRejectedValue("sem terminal");
    vi.mocked(api.desktopStatus).mockRejectedValue("sem desktop");
    vi.mocked(api.jobRunning).mockResolvedValue(null);
    vi.mocked(api.tweaksList).mockResolvedValue({
      tweaks: [],
      laptop: false,
      elevated: true,
      hkcu_allowed: true,
      journal_count: 0,
      restore_point_done: false,
    });
    vi.mocked(api.wallpaperMonitors).mockResolvedValue([]);
  });

  it("gallery_hidden_without_wallpaper_engine", async () => {
    vi.mocked(api.wallpaperStatus).mockResolvedValue(status(false));
    render(<Personalizar />);
    await screen.findByRole("button", { name: "Loja" });
    expect(screen.getByRole("button", { name: "Papel de parede" })).toBeDisabled();
  });

  it("gallery_lists_workshop_and_uses_a_wallpaper", async () => {
    vi.mocked(api.wallpaperStatus).mockResolvedValue(status(true));
    vi.mocked(api.wallpaperGallery).mockResolvedValue([
      { id: "1", dir: "d", title: "Chuva", kind: "scene", preview: null, thumb: null, active: true, active_on: [0] },
      { id: "2", dir: "d", title: "Neve", kind: null, preview: null, thumb: "data:image/jpeg;base64,AA==", active: false, active_on: [] },
    ]);
    vi.mocked(api.wallpaperOpen).mockResolvedValue(undefined);
    render(<Personalizar />);
    await goTo("Papel de parede");
    expect(await screen.findByText("Chuva")).toBeInTheDocument();
    expect(screen.getByText("Em uso")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Usar Chuva" })).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Usar Neve" }));
    expect(api.wallpaperOpen).toHaveBeenCalledWith("2", null);
  });

  it("gallery_applies_to_one_monitor", async () => {
    vi.mocked(api.wallpaperStatus).mockResolvedValue(status(true));
    vi.mocked(api.wallpaperMonitors).mockResolvedValue([
      { index: 0, name: "A", width: 1920, height: 1080, primary: true, current: null },
      { index: 1, name: "B", width: 2560, height: 1440, primary: false, current: null },
    ]);
    vi.mocked(api.wallpaperGallery).mockResolvedValue([
      { id: "1", dir: "d", title: "Chuva", kind: null, preview: null, thumb: null, active: true, active_on: [0] },
      { id: "2", dir: "d", title: "Neve", kind: null, preview: null, thumb: null, active: false, active_on: [] },
    ]);
    vi.mocked(api.wallpaperOpen).mockResolvedValue(undefined);
    render(<Personalizar />);
    await goTo("Papel de parede");
    expect(await screen.findByText("Em uso: monitor 1")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("radio", { name: /Monitor 2/ }));
    await userEvent.click(screen.getByRole("button", { name: "Usar Neve" }));
    expect(api.wallpaperOpen).toHaveBeenCalledWith("2", 1);
    // Chuva já está no monitor 1 e ainda pode ir para o 2.
    expect(screen.getByRole("button", { name: "Usar Chuva" })).toBeInTheDocument();
  });

  it("pause_mode_setting", async () => {
    vi.mocked(api.wallpaperStatus).mockResolvedValue(status(true));
    vi.mocked(api.wallpaperGallery).mockResolvedValue([]);
    vi.mocked(api.wallpaperPauseModeSet).mockResolvedValue(status(true, "stop"));
    render(<Personalizar />);
    await goTo("Papel de parede");
    expect(await screen.findByRole("radio", { name: /^Pausar/ })).toBeChecked();
    await userEvent.click(screen.getByRole("radio", { name: /^Parar/ }));
    expect(api.wallpaperPauseModeSet).toHaveBeenCalledWith("stop");
    await waitFor(() => expect(screen.getByRole("radio", { name: /^Parar/ })).toBeChecked());
  });
});
