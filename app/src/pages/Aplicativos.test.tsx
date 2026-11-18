import { act, render as rtlRender, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import type { AppItem, AppState, AppsPage } from "../api";
import Aplicativos from "./Aplicativos";
import { JobsProvider } from "../jobs";

const render = (ui: React.ReactElement) => rtlRender(<JobsProvider>{ui}</JobsProvider>);

vi.mock("../api", () => ({
  appsPage: vi.fn(),
  appsPageRefresh: vi.fn(),
  appsInstall: vi.fn(),
  appsUpgrade: vi.fn(),
  jobCancel: vi.fn(),
  jobRunning: vi.fn().mockResolvedValue(null),
  STORE_APP_INSTALLER_URL: "ms-windows-store://pdp/?ProductId=9NBLGGH4NNS1",
}));

const openUrl = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: (...a: unknown[]) => openUrl(...a),
}));

type Listener = (e: { payload: unknown }) => void;
const listeners = new Map<string, Listener>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, cb: Listener) => {
    listeners.set(name, cb);
    return Promise.resolve(() => listeners.delete(name));
  },
}));

const LICENSE = "Ao continuar, você aceita os termos de licença de cada aplicativo.";

const item = (
  id: string,
  name: string,
  state: AppState | null,
  over: Partial<AppItem["app"]> = {},
): AppItem => ({
  app: {
    id,
    name,
    description: `Descrição de ${name}`,
    category: "navegadores",
    winget: `W.${id}`,
    link: `https://${id}.example`,
    foss: false,
    ...over,
  },
  state,
});

const page = (apps: AppItem[], over: Partial<AppsPage> = {}): AppsPage => ({
  availability: { available: true, version: "1.7", reason: null },
  apps,
  categories: [
    { id: "navegadores", label: "Navegadores" },
    { id: "utilitarios", label: "Utilitários" },
  ],
  ...over,
});

const finish = async (result: unknown, job = 9) => {
  await act(async () => {
    listeners.get("job:done")!({ payload: { job, kind: "apps", result } });
  });
};

const sample = () =>
  page([
    item("firefox", "Firefox", "not_installed", { foss: true }),
    item("chrome", "Chrome", "installed"),
    item("7zip", "7-Zip", "update_available", {
      category: "utilitarios",
      description: "Compactador ótimo",
    }),
    item("vlc", "VLC", "not_installed", { category: "utilitarios" }),
  ]);

describe("Aplicativos", () => {
  beforeEach(() => {
    listeners.clear();
    openUrl.mockClear();
    vi.mocked(api.appsPage).mockReset().mockResolvedValue(sample());
    vi.mocked(api.appsPageRefresh).mockReset().mockResolvedValue(sample());
    vi.mocked(api.appsInstall).mockReset().mockResolvedValue(9);
    vi.mocked(api.appsUpgrade).mockReset().mockResolvedValue(9);
    vi.mocked(api.jobCancel).mockReset().mockResolvedValue(true);
  });

  it("filters_by_search_ignoring_accents", async () => {
    render(<Aplicativos />);
    await screen.findByText("Firefox");
    await userEvent.type(
      screen.getByRole("searchbox", { name: "Buscar aplicativos" }),
      "COMPACTADOR otimo",
    );
    expect(screen.getByText("7-Zip")).toBeInTheDocument();
    expect(screen.queryByText("Firefox")).not.toBeInTheDocument();
  });

  it("filters_by_category", async () => {
    render(<Aplicativos />);
    await screen.findByText("Firefox");
    await userEvent.selectOptions(screen.getByRole("combobox", { name: "Categoria" }), "utilitarios");
    expect(screen.getByText("VLC")).toBeInTheDocument();
    expect(screen.queryByText("Firefox")).not.toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Todas" })).toBeInTheDocument();
  });

  it("shows badges and site link", async () => {
    render(<Aplicativos />);
    await screen.findByText("Firefox");
    expect(screen.getByText("Código aberto")).toBeInTheDocument();
    expect(screen.getByText("Instalado")).toBeInTheDocument();
    expect(screen.getByText("Atualização disponível")).toBeInTheDocument();
    await userEvent.click(screen.getAllByRole("button", { name: "Site" })[0]);
    expect(openUrl).toHaveBeenCalledWith("https://firefox.example");
  });

  it("installed_without_update_is_not_selectable", async () => {
    render(<Aplicativos />);
    await screen.findByText("Firefox");
    expect(screen.getByRole("checkbox", { name: /Chrome/ })).toBeDisabled();
    expect(screen.getByRole("checkbox", { name: /7-Zip/ })).toBeEnabled();
  });

  it("install_only_after_confirmation_with_license_text", async () => {
    render(<Aplicativos />);
    await screen.findByText("Firefox");
    await userEvent.click(screen.getByRole("checkbox", { name: /Firefox/ }));
    expect(screen.getByText("1 selecionado(s)")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Instalar" }));
    const dlg = screen.getByRole("dialog");
    expect(within(dlg).getByText("Instalar 1 aplicativo(s)?")).toBeInTheDocument();
    expect(within(dlg).getByText(LICENSE)).toBeInTheDocument();
    expect(within(dlg).getByText("Firefox")).toBeInTheDocument();
    expect(api.appsInstall).not.toHaveBeenCalled();
    await userEvent.click(within(dlg).getByRole("button", { name: "Instalar" }));
    expect(api.appsInstall).toHaveBeenCalledWith(["firefox"]);
  });

  it("install_sends_only_not_installed_selected", async () => {
    render(<Aplicativos />);
    await screen.findByText("Firefox");
    await userEvent.click(screen.getByRole("checkbox", { name: /Firefox/ }));
    await userEvent.click(screen.getByRole("checkbox", { name: /7-Zip/ }));
    await userEvent.click(screen.getByRole("button", { name: "Instalar" }));
    await userEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Instalar" }));
    expect(api.appsInstall).toHaveBeenCalledWith(["firefox"]);
    expect(api.appsUpgrade).not.toHaveBeenCalled();
  });

  it("update_all_targets_apps_with_updates", async () => {
    render(<Aplicativos />);
    await screen.findByText("Firefox");
    await userEvent.click(screen.getByRole("button", { name: "Atualizar tudo" }));
    const dlg = screen.getByRole("dialog");
    expect(within(dlg).getByText("Atualizar 1 aplicativo(s)?")).toBeInTheDocument();
    expect(within(dlg).getByText(LICENSE)).toBeInTheDocument();
    await userEvent.click(within(dlg).getByRole("button", { name: "Atualizar" }));
    expect(api.appsUpgrade).toHaveBeenCalledWith(["7zip"]);
  });

  it("update_all_disabled_without_updates", async () => {
    vi.mocked(api.appsPage).mockResolvedValue(page([item("a", "A", "installed")]));
    render(<Aplicativos />);
    await screen.findByText("A");
    expect(screen.getByRole("button", { name: "Atualizar tudo" })).toBeDisabled();
  });

  it("shows_queue_progress_and_stop_button", async () => {
    render(<Aplicativos />);
    await screen.findByText("Firefox");
    await userEvent.click(screen.getByRole("checkbox", { name: /Firefox/ }));
    await userEvent.click(screen.getByRole("checkbox", { name: /VLC/ }));
    await userEvent.click(screen.getByRole("button", { name: "Instalar" }));
    await userEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Instalar" }));
    await act(async () => {
      listeners.get("job:progress")!({
        payload: { job: 9, phase: "apps", index: 1, total: 2, current: "VLC" },
      });
    });
    expect(screen.getByText("Instalando 2 de 2: VLC…")).toBeInTheDocument();
    expect(screen.getByRole("progressbar")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Parar depois deste" }));
    expect(api.jobCancel).toHaveBeenCalledWith(9);

    vi.mocked(api.appsPageRefresh).mockResolvedValue(
      page([item("firefox", "Firefox", "installed"), item("vlc", "VLC", "not_installed")]),
    );
    await finish([
      { id: "firefox", ok: true, changed: true, message: "Instalado" },
      { id: "vlc", ok: false, changed: false, message: "Falhou feio" },
    ]);
    expect(await screen.findByText(/Falhou feio/)).toBeInTheDocument();
    expect(api.appsPageRefresh).toHaveBeenCalled();
    // sucesso sai da seleção; falha continua selecionada
    await waitFor(() => expect(screen.getByText("1 selecionado(s)")).toBeInTheDocument());
  });

  it("cancel failure re-enables the stop button", async () => {
    vi.mocked(api.jobCancel).mockRejectedValue("falhou");
    render(<Aplicativos />);
    await screen.findByText("Firefox");
    await userEvent.click(screen.getByRole("checkbox", { name: /Firefox/ }));
    await userEvent.click(screen.getByRole("button", { name: "Instalar" }));
    await userEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Instalar" }));
    await act(async () => {
      listeners.get("job:progress")!({
        payload: { job: 9, phase: "apps", index: 0, total: 1, current: "Firefox" },
      });
    });
    await userEvent.click(screen.getByRole("button", { name: "Parar depois deste" }));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Parar depois deste" })).toBeEnabled(),
    );
  });

  it("stop button shows Parando… and is disabled", async () => {
    let release: (v: boolean) => void = () => {};
    vi.mocked(api.jobCancel).mockReturnValue(new Promise<boolean>((r) => (release = r)));
    render(<Aplicativos />);
    await screen.findByText("Firefox");
    await userEvent.click(screen.getByRole("checkbox", { name: /Firefox/ }));
    await userEvent.click(screen.getByRole("button", { name: "Instalar" }));
    await userEvent.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Instalar" }));
    await act(async () => {
      listeners.get("job:progress")!({
        payload: { job: 9, phase: "apps", index: 0, total: 1, current: "Firefox" },
      });
    });
    await userEvent.click(screen.getByRole("button", { name: "Parar depois deste" }));
    expect(screen.getByRole("button", { name: "Parando…" })).toBeDisabled();
    release(true);
  });

  it("selection survives filtering", async () => {
    render(<Aplicativos />);
    await screen.findByText("Firefox");
    await userEvent.click(screen.getByRole("checkbox", { name: /Firefox/ }));
    await userEvent.type(screen.getByRole("searchbox", { name: "Buscar aplicativos" }), "vlc");
    expect(screen.queryByText("Firefox")).not.toBeInTheDocument();
    expect(screen.getByText("1 selecionado(s)")).toBeInTheDocument();
  });

  it("no_winget_shows_store_button", async () => {
    vi.mocked(api.appsPage).mockResolvedValue(
      page([item("a", "A", null)], {
        availability: { available: false, version: null, reason: "winget não encontrado" },
      }),
    );
    render(<Aplicativos />);
    expect(await screen.findByText("winget não encontrado")).toBeInTheDocument();
    expect(
      screen.getByText(
        "O Sentinela usa o winget, o instalador oficial da Microsoft, para baixar cada aplicativo do fabricante e conferir o arquivo.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText("A")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Abrir na Microsoft Store" }));
    expect(openUrl).toHaveBeenCalledWith("ms-windows-store://pdp/?ProductId=9NBLGGH4NNS1");
  });

  it("unknown_state_shows_retry", async () => {
    vi.mocked(api.appsPage).mockResolvedValue(page([item("a", "A", null), item("b", "B", null)]));
    render(<Aplicativos />);
    expect(
      await screen.findByText("Não foi possível verificar o que já está instalado."),
    ).toBeInTheDocument();
    expect(screen.getByText("A")).toBeInTheDocument();
    expect(screen.queryByText("Instalado")).not.toBeInTheDocument();
    vi.mocked(api.appsPageRefresh).mockResolvedValue(sample());
    await userEvent.click(screen.getByRole("button", { name: "Tentar de novo" }));
    expect(await screen.findByText("Firefox")).toBeInTheDocument();
    expect(
      screen.queryByText("Não foi possível verificar o que já está instalado."),
    ).not.toBeInTheDocument();
  });

  it("load error shows alert", async () => {
    vi.mocked(api.appsPage).mockRejectedValueOnce("boom");
    render(<Aplicativos />);
    expect(await screen.findByRole("alert")).toHaveTextContent("boom");
  });
});
