import { render as rtlRender, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import type { CategorySummary, CleanSummary } from "../api";
import Limpeza from "./Limpeza";
import { JobsProvider } from "../jobs";

const render = (ui: React.ReactElement) =>
  rtlRender(<JobsProvider>{ui}</JobsProvider>);

vi.mock("../api", () => ({
  cleanPlan: vi.fn(),
  cleanApply: vi.fn(),
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

const cat = (
  id: string,
  files: number,
  bytes: number,
  over: Partial<CategorySummary> = {},
): CategorySummary => ({
  id,
  needs_admin: false,
  files,
  bytes,
  warnings: [],
  skipped_reason: null,
  sample: [{ path: `C:\\tmp\\${id}.tmp`, size: 1 }],
  deleted: null,
  freed: null,
  skipped_files: 0,
  ...over,
});

const summary = (over: Partial<CleanSummary> = {}): CleanSummary => ({
  categories: [],
  total_files: 0,
  total_bytes: 0,
  deleted: 0,
  freed: 0,
  elevated: false,
  ...over,
});

const mockPlan = (s: Partial<CleanSummary>) =>
  vi.mocked(api.cleanPlan).mockResolvedValue(summary(s));

describe("Limpeza", () => {
  beforeEach(() => {
    vi.mocked(api.cleanPlan).mockReset();
    vi.mocked(api.cleanApply).mockReset();
    listeners.clear();
  });

  it("só apaga depois da confirmação e avisa da Lixeira", async () => {
    mockPlan({
      categories: [cat("user-temp", 3, 300), cat("recycle-bin", 2, 200)],
      total_files: 5,
      total_bytes: 500,
    });
    vi.mocked(api.cleanApply).mockResolvedValue(1);
    render(<Limpeza />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Limpar" }),
    );
    expect(api.cleanApply).not.toHaveBeenCalled();
    expect(
      screen.getByText("A Lixeira será esvaziada permanentemente."),
    ).toBeInTheDocument();
    expect(screen.getByText(/5 arquivo\(s\), 500 B/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Apagar" }));
    expect(api.cleanApply).toHaveBeenCalledWith(["user-temp", "recycle-bin"]);
  });

  it("cancelar o diálogo não apaga nada", async () => {
    mockPlan({
      categories: [cat("user-temp", 3, 300)],
      total_files: 3,
      total_bytes: 300,
    });
    render(<Limpeza />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Limpar" }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Cancelar" }));
    expect(api.cleanApply).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("categoria que exige administrador aparece bloqueada", async () => {
    mockPlan({
      categories: [
        cat("user-temp", 3, 300),
        cat("windows-update", 0, 0, {
          needs_admin: true,
          skipped_reason: "requer administrador",
        }),
      ],
      total_files: 3,
      total_bytes: 300,
    });
    render(<Limpeza />);
    const box = await screen.findByRole("checkbox", {
      name: /Cache do Windows Update/,
    });
    expect(box).toBeDisabled();
    expect(screen.getByText(/requer administrador/)).toBeVisible();
    expect(
      screen.getByRole("checkbox", { name: /Temporários do usuário/ }),
    ).toBeChecked();
  });

  it("sem nada marcado, Limpar fica desabilitado", async () => {
    mockPlan({
      categories: [cat("user-temp", 3, 300)],
      total_files: 3,
      total_bytes: 300,
    });
    render(<Limpeza />);
    await userEvent.click(
      await screen.findByRole("checkbox", { name: /Temporários do usuário/ }),
    );
    expect(screen.getByRole("button", { name: "Limpar" })).toBeDisabled();
  });

  it("mostra o erro quando a simulação falha", async () => {
    vi.mocked(api.cleanPlan).mockRejectedValue("falhou");
    render(<Limpeza />);
    expect(await screen.findByRole("alert")).toHaveTextContent("falhou");
  });

  it("expande a amostra", async () => {
    mockPlan({
      categories: [cat("user-temp", 3, 300)],
      total_files: 3,
      total_bytes: 300,
    });
    render(<Limpeza />);
    await userEvent.click(
      await screen.findByRole("button", { name: "ver amostra" }),
    );
    expect(screen.getByText("C:\\tmp\\user-temp.tmp")).toBeInTheDocument();
  });

  it("depois de apagar mostra o resultado e refaz a simulação", async () => {
    mockPlan({
      categories: [cat("user-temp", 3, 300)],
      total_files: 3,
      total_bytes: 300,
    });
    vi.mocked(api.cleanApply).mockResolvedValue(7);
    render(<Limpeza />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Limpar" }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Apagar" }));
    await waitFor(() => expect(api.cleanApply).toHaveBeenCalled());
    listeners.get("job:done")!({
      payload: {
        job: 7,
        kind: "clean",
        result: summary({
          categories: [
            cat("user-temp", 3, 300, {
              deleted: 2,
              freed: 200,
              skipped_files: 1,
            }),
          ],
          deleted: 2,
          freed: 200,
        }),
      },
    });
    expect(
      await screen.findByText(/Apagados: 2 arquivo\(s\), 200 B liberados/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/1 arquivo\(s\) pulados \(em uso ou sem permissão\)/),
    ).toBeInTheDocument();
    await waitFor(() => expect(api.cleanPlan).toHaveBeenCalledTimes(3));
  });

  it("envia só a categoria marcada e selecionável", async () => {
    mockPlan({
      categories: [
        cat("user-temp", 3, 300),
        cat("crash-dumps", 4, 400),
        cat("windows-update", 0, 0, {
          needs_admin: true,
          skipped_reason: "requer administrador",
        }),
      ],
      total_files: 7,
      total_bytes: 700,
    });
    vi.mocked(api.cleanApply).mockResolvedValue(1);
    render(<Limpeza />);
    await userEvent.click(
      await screen.findByRole("checkbox", { name: /Despejos de erro/ }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Limpar" }));
    await userEvent.click(await screen.findByRole("button", { name: "Apagar" }));
    expect(api.cleanApply).toHaveBeenCalledWith(["user-temp"]);
  });

  it("Escape fecha o diálogo sem apagar", async () => {
    mockPlan({
      categories: [cat("user-temp", 3, 300)],
      total_files: 3,
      total_bytes: 300,
    });
    render(<Limpeza />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Limpar" }),
    );
    expect(await screen.findByRole("dialog")).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(api.cleanApply).not.toHaveBeenCalled();
  });

  it("o diálogo usa a simulação refeita e não envia categoria vazia", async () => {
    vi.mocked(api.cleanPlan)
      .mockResolvedValueOnce(
        summary({
          categories: [cat("user-temp", 3, 300), cat("recycle-bin", 2, 200)],
          total_files: 5,
          total_bytes: 500,
        }),
      )
      .mockResolvedValueOnce(
        summary({
          categories: [cat("user-temp", 1, 100), cat("recycle-bin", 0, 0)],
          total_files: 1,
          total_bytes: 100,
        }),
      );
    vi.mocked(api.cleanApply).mockResolvedValue(1);
    render(<Limpeza />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Limpar" }),
    );
    expect(await screen.findByText(/1 arquivo\(s\), 100 B/)).toBeInTheDocument();
    expect(api.cleanPlan).toHaveBeenLastCalledWith(["user-temp", "recycle-bin"]);
    expect(
      screen.queryByText("A Lixeira será esvaziada permanentemente."),
    ).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Apagar" }));
    expect(api.cleanApply).toHaveBeenCalledWith(["user-temp"]);
  });

  it("se a simulação refeita não achar nada, avisa e não abre o diálogo", async () => {
    vi.mocked(api.cleanPlan)
      .mockResolvedValueOnce(
        summary({
          categories: [cat("user-temp", 3, 300)],
          total_files: 3,
          total_bytes: 300,
        }),
      )
      .mockResolvedValueOnce(
        summary({ categories: [cat("user-temp", 0, 0)] }),
      );
    render(<Limpeza />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Limpar" }),
    );
    expect(await screen.findByText("Nada para apagar.")).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(api.cleanApply).not.toHaveBeenCalled();
  });

  it("mantém categorias desmarcadas ao simular novamente", async () => {
    mockPlan({
      categories: [cat("user-temp", 3, 300), cat("crash-dumps", 4, 400)],
      total_files: 7,
      total_bytes: 700,
    });
    render(<Limpeza />);
    await userEvent.click(
      await screen.findByRole("checkbox", { name: /Despejos de erro/ }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: "Simular novamente" }),
    );
    await waitFor(() => expect(api.cleanPlan).toHaveBeenCalledTimes(2));
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Simular novamente" }),
      ).toBeEnabled(),
    );
    expect(
      screen.getByRole("checkbox", { name: /Despejos de erro/ }),
    ).not.toBeChecked();
    expect(
      screen.getByRole("checkbox", { name: /Temporários do usuário/ }),
    ).toBeChecked();
  });
});
