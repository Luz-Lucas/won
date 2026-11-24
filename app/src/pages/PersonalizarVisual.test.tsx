import { render as rtlRender, screen, waitFor, within } from "@testing-library/react";
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
  themeList: vi.fn(),
  themePreview: vi.fn(),
  themeApply: vi.fn(),
  themeUndo: vi.fn(),
  themeExport: vi.fn(),
  themeImport: vi.fn(),
}));

const status = (installed: boolean, pause_mode: api.WallpaperPauseMode = "pause") => ({
  installed,
  running: installed,
  build: null,
  pause_mode,
});

const MIN = { name: "Minimalista", description: "Escuro e enxuto", toml: "version = 1", builtin: true };
const record = { seq: 1, theme: "Minimalista", applied_at: "x", tweak_ids: ["a"], wallpaper: null };

const preview = {
  items: [
    { kind: "tweak" as const, label: "Tema escuro dos apps", change: true, detail: "Valor: 1 → 0" },
    { kind: "tweak" as const, label: "Barra", change: false, detail: "já está assim" },
  ],
  warnings: ["o wallpaper 9 não está na sua Oficina"],
};

const goTo = async (name: string) => {
  await userEvent.click(await screen.findByRole("button", { name }));
};

describe("Personalizar: papel de parede e temas", () => {
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
    vi.mocked(api.themeList).mockResolvedValue({ themes: [MIN], history: [] });
    vi.mocked(api.wallpaperMonitors).mockResolvedValue([]);
  });

  it("gallery_hidden_without_wallpaper_engine", async () => {
    vi.mocked(api.wallpaperStatus).mockResolvedValue(status(false));
    render(<Personalizar />);
    await screen.findByRole("button", { name: "Temas" });
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

  it("apply_theme_shows_preview_first", async () => {
    vi.mocked(api.wallpaperStatus).mockResolvedValue(status(false));
    vi.mocked(api.themePreview).mockResolvedValue(preview);
    vi.mocked(api.themeApply).mockResolvedValue({
      results: [],
      effect: "relogon",
      warnings: [],
      record,
    });
    render(<Personalizar />);
    await goTo("Temas");
    await userEvent.click(await screen.findByRole("button", { name: "Ver mudanças de Minimalista" }));
    expect(api.themePreview).toHaveBeenCalledWith("version = 1");
    expect(api.themeApply).not.toHaveBeenCalled();
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Tema escuro dos apps: Valor: 1 → 0")).toBeInTheDocument();
    expect(within(dialog).getByText(/o wallpaper 9 não está/)).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Aplicar tema" }));
    expect(api.themeApply).toHaveBeenCalledWith("version = 1");
    expect(await screen.findByText("Tema aplicado: Minimalista.")).toBeInTheDocument();
    expect(screen.getByText(/Saia e entre de novo/)).toBeInTheDocument();
  });

  it("cancel_preview_applies_nothing", async () => {
    vi.mocked(api.wallpaperStatus).mockResolvedValue(status(false));
    vi.mocked(api.themePreview).mockResolvedValue(preview);
    render(<Personalizar />);
    await goTo("Temas");
    await userEvent.click(await screen.findByRole("button", { name: "Ver mudanças de Minimalista" }));
    await userEvent.click(await screen.findByRole("button", { name: "Cancelar" }));
    expect(api.themeApply).not.toHaveBeenCalled();
  });

  it("undo_theme_in_one_step", async () => {
    vi.mocked(api.wallpaperStatus).mockResolvedValue(status(false));
    vi.mocked(api.themeList).mockResolvedValue({ themes: [MIN], history: [record] });
    vi.mocked(api.themeUndo).mockResolvedValue({
      results: [],
      conflicts: [],
      warnings: [],
      remaining: null,
    });
    render(<Personalizar />);
    await goTo("Temas");
    expect(await screen.findByText("Último tema aplicado: Minimalista.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Desfazer tema" }));
    expect(api.themeUndo).toHaveBeenCalledTimes(1);
    expect(await screen.findByText("Tema desfeito.")).toBeInTheDocument();
  });

  it("import_shows_warnings", async () => {
    vi.mocked(api.wallpaperStatus).mockResolvedValue(status(false));
    openDialog.mockResolvedValue("C:\\temas\\noite.sentinela-theme");
    vi.mocked(api.themeImport).mockResolvedValue({
      name: "Noite",
      description: "d",
      toml: "version = 1",
      warnings: ["o wallpaper 424242 não está na sua Oficina; assine em https://steamcommunity.com/x"],
      disabled: ["a seção [mods] é de uma fase futura e foi ignorada"],
    });
    render(<Personalizar />);
    await goTo("Temas");
    await userEvent.click(await screen.findByRole("button", { name: "Importar tema…" }));
    expect(api.themeImport).toHaveBeenCalledWith("C:\\temas\\noite.sentinela-theme");
    expect(await screen.findByText(/o wallpaper 424242 não está/)).toBeInTheDocument();
    expect(screen.getByText(/Desligado: a seção \[mods\]/)).toBeInTheDocument();
    // Fica na lista, mas só aplica depois da prévia.
    expect(screen.getByRole("button", { name: "Ver mudanças de Noite" })).toBeInTheDocument();
    expect(api.themeApply).not.toHaveBeenCalled();
  });

  it("export_current_look", async () => {
    vi.mocked(api.wallpaperStatus).mockResolvedValue(status(false));
    saveDialog.mockResolvedValue("C:\\temas\\meu.sentinela-theme");
    vi.mocked(api.themeExport).mockResolvedValue(undefined);
    render(<Personalizar />);
    await goTo("Temas");
    const btn = await screen.findByRole("button", { name: "Exportar o visual atual…" });
    expect(btn).toBeDisabled();
    await userEvent.type(screen.getByLabelText("Nome do tema atual"), "Meu");
    await userEvent.click(btn);
    expect(api.themeExport).toHaveBeenCalledWith("Meu", "C:\\temas\\meu.sentinela-theme");
    expect(await screen.findByText(/Salvo em C:\\temas\\meu\.sentinela-theme/)).toBeInTheDocument();
  });
});
