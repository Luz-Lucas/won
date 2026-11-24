import {
  act,
  render as rtlRender,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import type { TweakItem, TweaksPage } from "../api";
import Personalizar from "./Personalizar";
import { JobsProvider } from "../jobs";

const render = (ui: React.ReactElement) =>
  rtlRender(<JobsProvider>{ui}</JobsProvider>);

vi.mock("../api", () => ({
  shellModsStatus: vi.fn().mockResolvedValue(undefined),
  shellModSet: vi.fn(),
  xamlCfgWrite: vi.fn(),
  tweaksList: vi.fn(),
  tweaksApply: vi.fn(),
  tweaksApplyChoice: vi.fn(),
  tweaksUndo: vi.fn(),
  restartExplorer: vi.fn(),
  desktopStatus: vi.fn().mockRejectedValue("sem desktop"),
  terminalStatus: vi.fn().mockRejectedValue("sem terminal"),
  wallpaperStatus: vi.fn().mockResolvedValue({
    installed: false,
    running: false,
    build: null,
    pause_mode: "pause",
  }),
  relaunchElevated: vi.fn(),
  jobCancel: vi.fn(),
  jobRunning: vi.fn().mockResolvedValue(null),
}));

type Listener = (e: { payload: unknown }) => void;
const listeners = new Map<string, Listener>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, cb: Listener) => {
    listeners.set(name, cb);
    return Promise.resolve(() => listeners.delete(name));
  },
}));

const item = (
  id: string,
  over: {
    tweak?: Partial<TweakItem["tweak"]>;
    view?: Partial<TweakItem["view"]>;
    in_journal?: boolean;
  } = {},
): TweakItem => ({
  tweak: {
    id,
    title: `Título ${id}`,
    description: "desc",
    group: "personalizacao",
    evidence: "comprovado",
    effect: "immediate",
    needs_admin: false,
    laptop_warning: false,
    recommended: false,
    source: "fonte",
    actions: [],
    choice: null,
    choice_label: null,
    area: "barra",
    ...over.tweak,
  },
  view: {
    id,
    status: "not_applied",
    blocked: null,
    changes: [{ description: `Valor ${id}`, current: "1", target: "0" }],
    ...over.view,
  },
  in_journal: over.in_journal ?? false,
});

const option = (id: string, title: string, applied = false): TweakItem =>
  item(id, {
    tweak: {
      title,
      choice: "taskbar-align",
      choice_label: "Alinhamento da barra de tarefas",
      effect: "explorer",
    },
    view: { status: applied ? "applied" : "not_applied" },
    in_journal: applied,
  });

const page = (tweaks: TweakItem[], over: Partial<TweaksPage> = {}): TweaksPage => ({
  tweaks,
  laptop: false,
  elevated: true,
  hkcu_allowed: true,
  journal_count: 0,
  restore_point_done: false,
  ...over,
});

const finish = async (result: unknown, job = 7) => {
  await act(async () => {
    listeners.get("job:done")!({ payload: { job, kind: "tweaks", result } });
  });
};

const applyReport = (id: string, effect = "immediate") => ({
  results: [{ tweak_id: id, ok: true, changed: true, message: "aplicada" }],
  effect,
  restore_point: null,
  notice: null,
});

describe("Personalizar", () => {
  beforeEach(() => {
    for (const f of [
      api.tweaksList,
      api.tweaksApply,
      api.tweaksApplyChoice,
      api.tweaksUndo,
      api.restartExplorer,
    ])
      vi.mocked(f).mockReset();
    listeners.clear();
  });

  it("switch_asks_confirmation_with_preview", async () => {
    vi.mocked(api.tweaksList).mockResolvedValue(page([item("a")]));
    vi.mocked(api.tweaksApply).mockResolvedValue(7);
    render(<Personalizar />);
    await userEvent.click(await screen.findByRole("button", { name: "Barra de tarefas" }));
    const sw = await screen.findByRole("switch", { name: "Título a" });
    expect(sw).not.toBeChecked();
    await userEvent.click(sw);
    expect(api.tweaksApply).not.toHaveBeenCalled();
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText("Valor a: 1 → 0")).toBeInTheDocument();
    await userEvent.click(within(dialog).getByRole("button", { name: "Aplicar" }));
    expect(api.tweaksApply).toHaveBeenCalledWith(["a"]);
  });

  it("turning_off_undoes", async () => {
    vi.mocked(api.tweaksList).mockResolvedValue(
      page([item("a", { view: { status: "applied" }, in_journal: true })]),
    );
    vi.mocked(api.tweaksUndo).mockResolvedValue(7);
    render(<Personalizar />);
    await userEvent.click(await screen.findByRole("button", { name: "Barra de tarefas" }));
    const sw = await screen.findByRole("switch", { name: "Título a" });
    expect(sw).toBeChecked();
    await userEvent.click(sw);
    await userEvent.click(screen.getByRole("button", { name: "Desfazer" }));
    expect(api.tweaksUndo).toHaveBeenCalledWith(["a"]);
    expect(api.tweaksApply).not.toHaveBeenCalled();
  });

  it("choice_selects_applied_option", async () => {
    vi.mocked(api.tweaksList).mockResolvedValue(
      page([option("left", "Esquerda", true), option("center", "Centro")]),
    );
    vi.mocked(api.tweaksApplyChoice).mockResolvedValue(7);
    render(<Personalizar />);
    await userEvent.click(await screen.findByRole("button", { name: "Barra de tarefas" }));
    const group = await screen.findByRole("group", {
      name: "Alinhamento da barra de tarefas",
    });
    expect(within(group).getByRole("radio", { name: "Esquerda" })).toBeChecked();
    expect(within(group).getByRole("radio", { name: "Centro" })).not.toBeChecked();
    await userEvent.click(within(group).getByRole("radio", { name: "Centro" }));
    expect(
      screen.getByRole("dialog", { name: /Alinhamento da barra de tarefas: Centro\?/ }),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Aplicar" }));
    expect(api.tweaksApplyChoice).toHaveBeenCalledWith("center");
  });

  it("no option checked when the Windows value is in use", async () => {
    vi.mocked(api.tweaksList).mockResolvedValue(
      page([option("left", "Esquerda"), option("center", "Centro")]),
    );
    render(<Personalizar />);
    await userEvent.click(await screen.findByRole("button", { name: "Barra de tarefas" }));
    const group = await screen.findByRole("group", {
      name: "Alinhamento da barra de tarefas",
    });
    for (const r of within(group).getAllByRole("radio")) expect(r).not.toBeChecked();
  });

  it("external_value_disables_toggle", async () => {
    vi.mocked(api.tweaksList).mockResolvedValue(
      page([item("a", { view: { status: "applied" }, in_journal: false })]),
    );
    render(<Personalizar />);
    await userEvent.click(await screen.findByRole("button", { name: "Barra de tarefas" }));
    const sw = await screen.findByRole("switch", { name: "Título a" });
    expect(sw).toBeChecked();
    expect(sw).toBeDisabled();
    expect(screen.getByText("Configurado fora do Sentinela.")).toBeInTheDocument();
  });

  it("explorer_notice_and_restart_confirmation", async () => {
    vi.mocked(api.tweaksList).mockResolvedValue(
      page([item("a", { tweak: { effect: "explorer" } })]),
    );
    vi.mocked(api.tweaksApply).mockResolvedValue(7);
    vi.mocked(api.restartExplorer).mockResolvedValue(undefined);
    render(<Personalizar />);
    await userEvent.click(await screen.findByRole("button", { name: "Barra de tarefas" }));
    await userEvent.click(await screen.findByRole("switch", { name: "Título a" }));
    await userEvent.click(screen.getByRole("button", { name: "Aplicar" }));
    await waitFor(() => expect(api.tweaksApply).toHaveBeenCalled());
    await finish(applyReport("a", "explorer"));
    expect(
      await screen.findByText("Reinicie o Explorer para ver a mudança."),
    ).toBeInTheDocument();
    // Nunca reinicia sozinho: só pelo botão, e depois de confirmar.
    expect(api.restartExplorer).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Reiniciar Explorer" }));
    expect(
      screen.getByRole("dialog", {
        name: "Reiniciar o Explorer? As janelas de pastas abertas serão fechadas.",
      }),
    ).toBeInTheDocument();
    expect(api.restartExplorer).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Reiniciar" }));
    expect(api.restartExplorer).toHaveBeenCalledTimes(1);
    expect(await screen.findByText("Explorer reiniciado.")).toBeInTheDocument();
  });

  it("no explorer notice for an immediate tweak", async () => {
    vi.mocked(api.tweaksList).mockResolvedValue(page([item("a")]));
    vi.mocked(api.tweaksApply).mockResolvedValue(7);
    render(<Personalizar />);
    await userEvent.click(await screen.findByRole("button", { name: "Barra de tarefas" }));
    await userEvent.click(await screen.findByRole("switch", { name: "Título a" }));
    await userEvent.click(screen.getByRole("button", { name: "Aplicar" }));
    await waitFor(() => expect(api.tweaksApply).toHaveBeenCalled());
    await finish(applyReport("a"));
    await waitFor(() => expect(api.tweaksList).toHaveBeenCalledTimes(2));
    expect(
      screen.queryByRole("button", { name: "Reiniciar Explorer" }),
    ).not.toBeInTheDocument();
  });

  it("admin_items_locked_when_not_elevated", async () => {
    vi.mocked(api.tweaksList).mockResolvedValue(
      page(
        [
          item("w", {
            tweak: { needs_admin: true },
            view: { blocked: "requer administrador" },
          }),
          item("a"),
        ],
        { elevated: false },
      ),
    );
    render(<Personalizar />);
    await userEvent.click(await screen.findByRole("button", { name: "Barra de tarefas" }));
    expect(await screen.findByRole("switch", { name: "Título w" })).toBeDisabled();
    expect(screen.getByRole("switch", { name: "Título a" })).toBeEnabled();
    expect(screen.getByText("Requer administrador")).toBeInTheDocument();
    expect(
      screen.getByText("Algumas personalizações exigem administrador."),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Reiniciar como administrador" }),
    ).toBeInTheDocument();
  });

  it("hub lists the areas and each one shows only its own items", async () => {
    vi.mocked(api.tweaksList).mockResolvedValue(
      page([
        item("a"),
        item("dark", { tweak: { area: "aparencia" } }),
        item("transparency-off", { tweak: { group: "visual", area: null } }),
        item("game-mode", { tweak: { group: "jogos", area: null } }),
      ]),
    );
    render(<Personalizar />);
    expect(screen.getByRole("heading", { level: 1, name: "Estilização" })).toBeInTheDocument();
    for (const n of ["Barra de tarefas", "Menu Iniciar", "Explorador de Arquivos", "Cores e aparência", "Temas", "Sistema"])
      expect(await screen.findByRole("button", { name: n })).toBeEnabled();
    expect(screen.queryByRole("switch")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Barra de tarefas" }));
    expect(await screen.findByRole("switch", { name: "Título a" })).toBeInTheDocument();
    expect(screen.queryByRole("switch", { name: "Título dark" })).not.toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Prévia da barra de tarefas" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "← Todas as áreas" }));
    await userEvent.click(screen.getByRole("button", { name: "Cores e aparência" }));
    expect(await screen.findByRole("switch", { name: "Título dark" })).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Título transparency-off" })).toBeInTheDocument();
    expect(screen.queryByRole("switch", { name: "Título game-mode" })).not.toBeInTheDocument();
  });

  it("preview reacts to the option under the pointer", async () => {
    vi.mocked(api.tweaksList).mockResolvedValue(
      page([option("taskbar-align-left", "Esquerda"), option("taskbar-align-center", "Centro", true)]),
    );
    render(<Personalizar />);
    await userEvent.click(await screen.findByRole("button", { name: "Barra de tarefas" }));
    const img = await screen.findByRole("img", { name: "Prévia da barra de tarefas" });
    expect(img.querySelector(".sp-taskbar")).toHaveAttribute("data-align", "center");
    await userEvent.hover(screen.getByRole("radio", { name: "Esquerda" }));
    expect(img.querySelector(".sp-taskbar")).toHaveAttribute("data-align", "left");
    await userEvent.unhover(screen.getByRole("radio", { name: "Esquerda" }));
    expect(img.querySelector(".sp-taskbar")).toHaveAttribute("data-align", "center");
  });
});
