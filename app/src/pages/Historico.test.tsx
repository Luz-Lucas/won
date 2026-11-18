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
import type { JournalEntry } from "../api";
import Historico from "./Historico";
import { JobsProvider } from "../jobs";

const render = (ui: React.ReactElement) =>
  rtlRender(<JobsProvider>{ui}</JobsProvider>);

vi.mock("../api", () => ({
  tweaksJournal: vi.fn(),
  tweaksUndo: vi.fn(),
  tweaksUndoAll: vi.fn(),
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

const entry = (id: string, doneCount = 1): JournalEntry => ({
  tweak_id: id,
  title: `Título ${id}`,
  applied_at: "2026-03-05T12:00:00Z",
  actions: Array.from({ length: doneCount + 1 }, (_, n) => ({
    action: {} as never,
    before: { kind: "unavailable", reason: "x" },
    written: { kind: "unavailable", reason: "x" },
    done: n < doneCount,
  })),
});

const finish = async (result: unknown, job = 9) => {
  await act(async () => {
    listeners.get("job:done")!({ payload: { job, kind: "tweaks", result } });
  });
};

describe("Historico", () => {
  beforeEach(() => {
    vi.mocked(api.tweaksJournal).mockReset();
    vi.mocked(api.tweaksUndo).mockReset();
    vi.mocked(api.tweaksUndoAll).mockReset();
    listeners.clear();
  });

  it("empty_journal_message", async () => {
    vi.mocked(api.tweaksJournal).mockResolvedValue([]);
    render(<Historico />);
    expect(
      await screen.findByText("Nenhuma otimização aplicada pelo Sentinela."),
    ).toBeVisible();
    expect(screen.queryByRole("button", { name: "Desfazer tudo" })).toBeNull();
  });

  it("rows_show_title_date_and_done_change_count", async () => {
    vi.mocked(api.tweaksJournal).mockResolvedValue([entry("a", 2)]);
    render(<Historico />);
    expect(await screen.findByText("Título a")).toBeVisible();
    expect(screen.getByText("2 mudança(s)")).toBeVisible();
  });

  it("undo_asks_confirmation", async () => {
    vi.mocked(api.tweaksJournal).mockResolvedValue([entry("a"), entry("b")]);
    vi.mocked(api.tweaksUndo).mockResolvedValue(9);
    render(<Historico />);
    const row = (await screen.findByText("Título a")).closest("li")!;
    await userEvent.click(within(row).getByRole("button", { name: "Desfazer" }));
    expect(api.tweaksUndo).not.toHaveBeenCalled();
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText('Desfazer "Título a"?')).toBeVisible();
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancelar" }));
    expect(api.tweaksUndo).not.toHaveBeenCalled();
    await userEvent.click(within(row).getByRole("button", { name: "Desfazer" }));
    await userEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", { name: "Desfazer" }),
    );
    expect(api.tweaksUndo).toHaveBeenCalledWith(["a"]);
  });

  it("undo_all_asks_confirmation", async () => {
    vi.mocked(api.tweaksJournal).mockResolvedValue([entry("a"), entry("b")]);
    vi.mocked(api.tweaksUndoAll).mockResolvedValue(9);
    render(<Historico />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Desfazer tudo" }),
    );
    expect(api.tweaksUndoAll).not.toHaveBeenCalled();
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText("Desfazer todas as 2 otimizações?")).toBeVisible();
    await userEvent.click(within(dialog).getByRole("button", { name: "Desfazer tudo" }));
    expect(api.tweaksUndoAll).toHaveBeenCalledTimes(1);
  });

  it("conflicts_are_listed_after_undo", async () => {
    vi.mocked(api.tweaksJournal)
      .mockResolvedValueOnce([entry("a")])
      .mockResolvedValue([]);
    vi.mocked(api.tweaksUndo).mockResolvedValue(9);
    render(<Historico />);
    const row = (await screen.findByText("Título a")).closest("li")!;
    await userEvent.click(within(row).getByRole("button", { name: "Desfazer" }));
    await userEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", { name: "Desfazer" }),
    );
    await finish({
      results: [{ tweak_id: "a", ok: true, message: "" }],
      conflicts: ["HKCU\\Software\\X\\Y"],
    });
    expect(
      await screen.findByText("Mantido porque mudou depois da otimização:"),
    ).toBeVisible();
    expect(screen.getByText("HKCU\\Software\\X\\Y")).toBeVisible();
    expect(screen.getByText("Título a: desfeito")).toBeVisible();
    await waitFor(() => expect(api.tweaksJournal).toHaveBeenCalledTimes(2));
  });

  it("undo_buttons_are_disabled_while_a_tweaks_job_runs", async () => {
    vi.mocked(api.tweaksJournal).mockResolvedValue([entry("a")]);
    vi.mocked(api.tweaksUndoAll).mockReturnValue(new Promise(() => {}));
    render(<Historico />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Desfazer tudo" }),
    );
    await userEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", { name: "Desfazer tudo" }),
    );
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Desfazer tudo" })).toBeDisabled(),
    );
    expect(screen.getByRole("button", { name: "Desfazer" })).toBeDisabled();
  });

  it("job_error_is_shown_as_alert", async () => {
    vi.mocked(api.tweaksJournal).mockResolvedValue([entry("a")]);
    vi.mocked(api.tweaksUndoAll).mockRejectedValue("falhou");
    render(<Historico />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Desfazer tudo" }),
    );
    await userEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", { name: "Desfazer tudo" }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent("falhou");
  });

  it("foreign_apply_report_in_shared_store_does_not_crash_or_show", async () => {
    vi.mocked(api.tweaksJournal).mockResolvedValue([entry("a")]);
    vi.mocked(api.tweaksUndoAll).mockResolvedValue(9);
    render(<Historico />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Desfazer tudo" }),
    );
    await userEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", { name: "Desfazer tudo" }),
    );
    await finish({
      results: [{ tweak_id: "a", ok: true, message: "ok" }],
      effect: "reboot",
      restore_point: "Sentinela 1",
    });
    expect(screen.getByRole("heading", { name: "Histórico" })).toBeVisible();
    expect(screen.queryByText("Título a: desfeito")).toBeNull();
  });

  it("old_undo_report_does_not_reappear_after_leaving_and_returning", async () => {
    vi.mocked(api.tweaksJournal).mockResolvedValue([entry("a")]);
    vi.mocked(api.tweaksUndoAll).mockResolvedValue(9);
    const { rerender } = rtlRender(<Historico />, { wrapper: JobsProvider });
    await userEvent.click(
      await screen.findByRole("button", { name: "Desfazer tudo" }),
    );
    await userEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", { name: "Desfazer tudo" }),
    );
    await finish({
      results: [{ tweak_id: "a", ok: true, message: "" }],
      conflicts: [],
    });
    expect(await screen.findByText("Título a: desfeito")).toBeVisible();
    rerender(<div />);
    rerender(<Historico />);
    await screen.findByRole("heading", { name: "Histórico" });
    expect(screen.queryByText(/desfeito/)).toBeNull();
    expect(screen.queryByText("a: desfeito")).toBeNull();
  });
});
