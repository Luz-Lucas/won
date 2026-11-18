import {
  act,
  render as rtlRender,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import type { TweakItem, TweaksPage } from "../api";
import Jogos from "./Jogos";
import { JobsProvider } from "../jobs";

const render = (ui: React.ReactElement) =>
  rtlRender(<JobsProvider>{ui}</JobsProvider>);

vi.mock("../api", () => ({
  tweaksList: vi.fn(),
  tweaksApply: vi.fn(),
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
  } = {},
): TweakItem => ({
  tweak: {
    id,
    title: `Título ${id}`,
    description: "desc",
    group: "jogos",
    evidence: "comprovado",
    effect: "immediate",
    needs_admin: false,
    laptop_warning: false,
    recommended: true,
    source: "fonte",
    actions: [],
    ...over.tweak,
  },
  view: {
    id,
    status: "not_applied",
    blocked: null,
    changes: [{ description: `Mudança ${id}`, current: "0", target: "1" }],
    ...over.view,
  },
  in_journal: false,
});

const page = (tweaks: TweakItem[], over: Partial<TweaksPage> = {}): TweaksPage => ({
  tweaks,
  laptop: false,
  elevated: true,
  hkcu_allowed: true,
  journal_count: 0,
  ...over,
});

const finish = async (result: unknown, job = 7) => {
  await act(async () => {
    listeners.get("job:done")!({ payload: { job, kind: "tweaks", result } });
  });
};

describe("Jogos", () => {
  beforeEach(() => {
    vi.mocked(api.tweaksList).mockReset();
    vi.mocked(api.tweaksApply).mockReset();
    listeners.clear();
  });

  it("recommended_profile_checks_only_available_recommended", async () => {
    vi.mocked(api.tweaksList).mockResolvedValue(
      page([
        item("a"),
        item("b", { tweak: { recommended: false } }),
        item("c", { view: { blocked: "Requer administrador." } }),
        item("d", { view: { status: "unavailable" } }),
        item("e", { view: { status: "applied" } }),
      ]),
    );
    render(<Jogos />);
    expect(await screen.findByRole("checkbox", { name: /Título a/ })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: /Título b/ })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: /Título c/ })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: /Título d/ })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: /Título e/ })).not.toBeChecked();
  });

  it("laptop_does_not_preselect_power_plan_and_shows_warning", async () => {
    vi.mocked(api.tweaksList).mockResolvedValue(
      page(
        [
          item("power-high-performance", {
            tweak: { group: "energia", laptop_warning: true },
          }),
          item("a"),
        ],
        { laptop: true },
      ),
    );
    render(<Jogos />);
    expect(
      await screen.findByRole("checkbox", { name: /Título power-high-performance/ }),
    ).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: /Título a/ })).toBeChecked();
    expect(
      screen.getByText("Em notebook, gasta mais bateria e esquenta mais."),
    ).toBeVisible();
  });

  it("blocked_row_is_disabled_with_reason", async () => {
    vi.mocked(api.tweaksList).mockResolvedValue(
      page([item("c", { view: { blocked: "Outra conta de usuário ativa." } })]),
    );
    render(<Jogos />);
    expect(await screen.findByRole("checkbox", { name: /Título c/ })).toBeDisabled();
    expect(screen.getByText("Outra conta de usuário ativa.")).toBeVisible();
  });

  it("applies_only_after_confirmation", async () => {
    vi.mocked(api.tweaksList).mockResolvedValue(
      page([item("a"), item("b", { tweak: { recommended: false } })]),
    );
    vi.mocked(api.tweaksApply).mockResolvedValue(7);
    render(<Jogos />);
    await screen.findByRole("checkbox", { name: /Título a/ });
    expect(screen.getByText("Mudança a: 0 → 1")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Aplicar" }));
    expect(api.tweaksApply).not.toHaveBeenCalled();
    expect(screen.getByText("Aplicar 1 otimização(ões)?")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Cancelar" }));
    expect(api.tweaksApply).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Aplicar" }));
    const dialog = screen.getByRole("dialog");
    await userEvent.click(
      Array.from(dialog.querySelectorAll("button")).find(
        (b) => b.textContent === "Aplicar",
      )!,
    );
    expect(api.tweaksApply).toHaveBeenCalledWith(["a"]);
  });

  it("shows_reboot_notice_after_apply_with_reboot_effect", async () => {
    vi.mocked(api.tweaksList).mockResolvedValue(
      page([item("a", { tweak: { needs_admin: true, effect: "reboot" } })]),
    );
    vi.mocked(api.tweaksApply).mockResolvedValue(7);
    render(<Jogos />);
    await screen.findByRole("checkbox", { name: /Título a/ });
    await userEvent.click(screen.getByRole("button", { name: "Aplicar" }));
    expect(
      screen.getByText("Um ponto de restauração do Windows será criado antes."),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getAllByRole("button", { name: "Aplicar" }).slice(-1)[0],
    );
    await finish({
      results: [{ tweak_id: "a", ok: true, message: "ok" }],
      effect: "reboot",
      restore_point: "Sentinela 1",
    });
    expect(
      await screen.findByText("Reinicie o computador para concluir."),
    ).toBeInTheDocument();
    expect(screen.getByText("Ponto de restauração: Sentinela 1")).toBeInTheDocument();
    await waitFor(() => expect(api.tweaksList).toHaveBeenCalledTimes(2));
  });

  it("item_failure_is_listed_with_reason", async () => {
    vi.mocked(api.tweaksList).mockResolvedValue(page([item("a")]));
    vi.mocked(api.tweaksApply).mockResolvedValue(7);
    render(<Jogos />);
    await screen.findByRole("checkbox", { name: /Título a/ });
    await userEvent.click(screen.getByRole("button", { name: "Aplicar" }));
    await userEvent.click(
      screen.getAllByRole("button", { name: "Aplicar" }).slice(-1)[0],
    );
    await finish({
      results: [{ tweak_id: "a", ok: false, message: "acesso negado" }],
      effect: "immediate",
      restore_point: null,
    });
    expect(
      await screen.findByText("Título a: acesso negado"),
    ).toBeInTheDocument();
  });

  it("shows_admin_banner_when_not_elevated_even_if_blocked", async () => {
    vi.mocked(api.tweaksList).mockResolvedValue(
      page(
        [
          item("a", {
            tweak: { needs_admin: true },
            view: { blocked: "Requer administrador." },
          }),
        ],
        { elevated: false },
      ),
    );
    vi.mocked(api.relaunchElevated).mockResolvedValue(undefined);
    render(<Jogos />);
    expect(
      await screen.findByText("Algumas otimizações exigem administrador."),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Reiniciar como administrador" }),
    );
    expect(api.relaunchElevated).toHaveBeenCalled();
  });

  it("personalizado_unchecks_rows_that_became_applied_after_reload", async () => {
    vi.mocked(api.tweaksList)
      .mockResolvedValueOnce(page([item("a", { tweak: { recommended: false } })]))
      .mockResolvedValue(
        page([
          item("a", {
            tweak: { recommended: false },
            view: { status: "applied" },
          }),
        ]),
      );
    vi.mocked(api.tweaksApply).mockResolvedValue(7);
    render(<Jogos />);
    await userEvent.click(
      await screen.findByRole("checkbox", { name: /Título a/ }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Aplicar" }));
    await userEvent.click(
      screen.getAllByRole("button", { name: "Aplicar" }).slice(-1)[0],
    );
    await finish({
      results: [{ tweak_id: "a", ok: true, message: "ok" }],
      effect: "immediate",
      restore_point: null,
    });
    await waitFor(() =>
      expect(screen.getByRole("checkbox", { name: /Título a/ })).not.toBeChecked(),
    );
    expect(screen.getByRole("button", { name: "Aplicar" })).toBeDisabled();
  });

  it("shows_alert_and_retry_when_list_fails", async () => {
    vi.mocked(api.tweaksList).mockRejectedValueOnce("falhou");
    vi.mocked(api.tweaksList).mockResolvedValue(page([item("a")]));
    render(<Jogos />);
    expect(await screen.findByRole("alert")).toHaveTextContent("falhou");
    await userEvent.click(screen.getByRole("button", { name: "Tentar de novo" }));
    expect(await screen.findByRole("checkbox", { name: /Título a/ })).toBeInTheDocument();
  });

  it("shows_alert_when_apply_job_errors", async () => {
    vi.mocked(api.tweaksList).mockResolvedValue(page([item("a")]));
    vi.mocked(api.tweaksApply).mockResolvedValue(7);
    render(<Jogos />);
    await screen.findByRole("checkbox", { name: /Título a/ });
    await userEvent.click(screen.getByRole("button", { name: "Aplicar" }));
    await userEvent.click(
      screen.getAllByRole("button", { name: "Aplicar" }).slice(-1)[0],
    );
    await act(async () => {
      listeners.get("job:error")!({
        payload: { job: 7, kind: "tweaks", message: "boom" },
      });
    });
    expect(await screen.findByRole("alert")).toHaveTextContent("boom");
  });

  it("ignores_a_stale_or_foreign_job_result_from_the_shared_store", async () => {
    vi.mocked(api.tweaksList).mockResolvedValue(page([item("a")]));
    vi.mocked(api.tweaksApply).mockResolvedValue(7);
    const { rerender } = rtlRender(<Jogos />, { wrapper: JobsProvider });
    await screen.findByRole("checkbox", { name: /Título a/ });
    await userEvent.click(screen.getByRole("button", { name: "Aplicar" }));
    await userEvent.click(
      screen.getAllByRole("button", { name: "Aplicar" }).slice(-1)[0],
    );
    // Resultado de outro tipo (UndoReport) no job compartilhado.
    await finish({ results: [{ tweak_id: "a", ok: true, message: "" }], conflicts: [] });
    expect(screen.queryByText(/Ponto de restauração/)).toBeNull();
    // Volta à página depois de um resultado concluído: nada antigo reaparece.
    rerender(<div />);
    rerender(<Jogos />);
    await screen.findByRole("checkbox", { name: /Título a/ });
    expect(screen.queryByText("Título a: aplicado")).toBeNull();
    expect(screen.queryByText(/Ponto de restauração/)).toBeNull();
  });
});
