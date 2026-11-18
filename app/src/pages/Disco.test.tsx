import { render as rtlRender, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import type { DupGroup, FindResult, LargeFile } from "../api";
import Disco from "./Disco";
import { JobsProvider } from "../jobs";

const render = (ui: React.ReactElement) =>
  rtlRender(<JobsProvider>{ui}</JobsProvider>);

vi.mock("../api", () => ({
  diskFindLarge: vi.fn(),
  diskFindDupes: vi.fn(),
  jobCancel: vi.fn(),
  jobRunning: vi.fn().mockResolvedValue(null),
}));

type Listener = (e: { payload: unknown }) => void;
const listeners = new Map<string, Set<Listener>>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, cb: Listener) => {
    const set = listeners.get(name) ?? new Set<Listener>();
    set.add(cb);
    listeners.set(name, set);
    return Promise.resolve(() => set.delete(cb));
  },
}));

const openDialog = vi.fn();
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...a: unknown[]) => openDialog(...a),
}));

const reveal = vi.fn();
vi.mock("@tauri-apps/plugin-opener", () => ({
  revealItemInDir: (...a: unknown[]) => reveal(...a),
}));

const result = <T,>(
  items: T[],
  over: Partial<FindResult<T>> = {},
): FindResult<T> => ({
  items,
  errors: [],
  interrupted: false,
  errors_total: over.errors?.length ?? 0,
  ...over,
});

const finish = async (res: unknown, kind: string, job = 7) => {
  await act(async () => {
    listeners
      .get("job:done")!
      .forEach((cb) => cb({ payload: { job, kind, result: res } }));
  });
};

const pick = async () => {
  openDialog.mockResolvedValueOnce("C:/dados");
  await userEvent.click(
    screen.getByRole("button", { name: "Escolher pasta…" }),
  );
  await screen.findByText("C:/dados");
};

const searchLarge = async () => {
  vi.mocked(api.diskFindLarge).mockResolvedValue(7);
  await pick();
  await userEvent.click(screen.getByRole("button", { name: "Procurar" }));
};

const searchDupes = async () => {
  vi.mocked(api.diskFindDupes).mockResolvedValue(8);
  await userEvent.click(screen.getByRole("tab", { name: "Duplicados" }));
  await pick();
  await userEvent.click(screen.getByRole("button", { name: "Procurar" }));
};

describe("Disco", () => {
  beforeEach(() => {
    vi.mocked(api.diskFindLarge).mockReset();
    vi.mocked(api.diskFindDupes).mockReset();
    vi.mocked(api.jobCancel).mockReset();
    openDialog.mockReset();
    reveal.mockReset();
    listeners.clear();
  });

  it("duplicados mostram o espaço desperdiçado e não oferecem apagar", async () => {
    render(<Disco />);
    await searchDupes();
    const group: DupGroup = {
      size: 100,
      sha256: "ab",
      paths: ["C:/a.bin", "C:/b.bin", "C:/c.bin"],
    };
    await finish(result([group]), "find_dupes", 8);
    expect(
      await screen.findByText("Espaço desperdiçado: 200 B"),
    ).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "Abrir pasta" })).toHaveLength(
      3,
    );
    for (const b of screen.getAllByRole("button"))
      expect(b.textContent).not.toMatch(/apagar|excluir/i);
    expect(screen.queryByRole("button", { name: /apagar|excluir/i })).toBeNull();
  });

  it("envia os valores padrão e lista os maiores arquivos", async () => {
    render(<Disco />);
    expect(screen.getByLabelText("Quantidade")).toHaveValue(20);
    expect(screen.getByLabelText("Tamanho mínimo (MB)")).toHaveValue(0);
    expect(screen.getByRole("button", { name: "Procurar" })).toBeDisabled();
    await searchLarge();
    expect(api.diskFindLarge).toHaveBeenCalledWith("C:/dados", 20, 0);
    await finish(
      result<LargeFile>([{ path: "C:/dados/grande.iso", size: 2048 }]),
      "find_large",
    );
    expect(await screen.findByText("C:/dados/grande.iso")).toBeVisible();
    expect(screen.getByText("2.0 KB")).toBeVisible();
    await userEvent.click(screen.getByRole("button", { name: "Abrir pasta" }));
    expect(reveal).toHaveBeenCalledWith("C:/dados/grande.iso");
  });

  it("Duplicados usa o mínimo em KB (padrão 1)", async () => {
    render(<Disco />);
    await userEvent.click(screen.getByRole("tab", { name: "Duplicados" }));
    expect(screen.getByLabelText("Tamanho mínimo (KB)")).toHaveValue(1);
    await searchDupes();
    expect(api.diskFindDupes).toHaveBeenCalledWith("C:/dados", 1);
  });

  it("durante a busca desabilita os botões e permite cancelar", async () => {
    render(<Disco />);
    await searchLarge();
    expect(screen.getByText("Procurando…")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Escolher pasta…" }),
    ).toBeDisabled();
    expect(screen.queryByRole("button", { name: "Procurar" })).toBeNull();
    vi.mocked(api.jobCancel).mockRejectedValue("falhou");
    await userEvent.click(screen.getByRole("button", { name: "Cancelar" }));
    expect(api.jobCancel).toHaveBeenCalledWith(7);
  });

  it("mostra o erro de validação e volta a habilitar", async () => {
    vi.mocked(api.diskFindLarge).mockRejectedValue(
      "o caminho não existe: C:/dados",
    );
    render(<Disco />);
    await pick();
    await userEvent.click(screen.getByRole("button", { name: "Procurar" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "o caminho não existe: C:/dados",
    );
    expect(screen.getByRole("button", { name: "Procurar" })).toBeEnabled();
  });

  it("resultado interrompido é parcial; vazio e não verificados", async () => {
    render(<Disco />);
    await searchLarge();
    await finish(
      result<LargeFile>([], {
        interrupted: true,
        errors: [{ path: "C:\\dados\\x", reason: "acesso negado" }],
      }),
      "find_large",
    );
    expect(
      await screen.findByText("Busca interrompida: resultado parcial."),
    ).toBeInTheDocument();
    expect(screen.getByText("Nenhum arquivo encontrado.")).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: /Não verificados/ }),
    );
    expect(screen.getByText(/acesso negado/)).toBeInTheDocument();
  });

  it("duplicados vazio mostra o estado vazio", async () => {
    render(<Disco />);
    await searchDupes();
    await finish(result<DupGroup>([]), "find_dupes", 8);
    expect(
      await screen.findByText("Nenhum duplicado encontrado."),
    ).toBeInTheDocument();
  });

  it("valida os campos no cliente e não chama o backend", async () => {
    render(<Disco />);
    await pick();
    const qty = screen.getByLabelText("Quantidade");
    const search = screen.getByRole("button", { name: "Procurar" });
    for (const bad of ["-1", "0", "501", "abc", ""]) {
      await userEvent.clear(qty);
      if (bad) await userEvent.type(qty, bad);
      await userEvent.click(search);
      expect(await screen.findByRole("alert")).toBeInTheDocument();
    }
    const mb = screen.getByLabelText("Tamanho mínimo (MB)");
    await userEvent.clear(qty);
    await userEvent.type(qty, "5");
    expect(screen.queryByRole("alert")).toBeNull();
    await userEvent.clear(mb);
    await userEvent.click(search);
    expect(await screen.findByRole("alert")).toHaveTextContent(/inteiros/);
    await userEvent.type(mb, "-1");
    await userEvent.click(search);
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("tab", { name: "Duplicados" }));
    expect(screen.queryByRole("alert")).toBeNull();
    const kb = screen.getByLabelText("Tamanho mínimo (KB)");
    for (const bad of ["", "-1", "abc"]) {
      await userEvent.clear(kb);
      if (bad) await userEvent.type(kb, bad);
      await userEvent.click(screen.getByRole("button", { name: "Procurar" }));
      expect(await screen.findByRole("alert")).toBeInTheDocument();
    }
    expect(api.diskFindLarge).not.toHaveBeenCalled();
    expect(api.diskFindDupes).not.toHaveBeenCalled();
  });

  it("evento de outro job não afeta a aba em andamento nem a outra aba", async () => {
    render(<Disco />);
    await searchLarge(); // maiores arquivos = job 7
    await finish(result<DupGroup>([]), "find_dupes", 8);
    expect(screen.getByText("Procurando…")).toBeInTheDocument();
    expect(screen.queryByText("Nenhum duplicado encontrado.")).toBeNull();
    await finish(result<LargeFile>([]), "find_large", 7);
    expect(
      await screen.findByText("Nenhum arquivo encontrado."),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("tab", { name: "Duplicados" }));
    expect(screen.queryByText("Nenhum duplicado encontrado.")).toBeNull();
  });

  it("limita a lista de não verificados a 200", async () => {
    render(<Disco />);
    await searchLarge();
    await finish(
      result<LargeFile>([], {
        errors_total: 250,
        errors: Array.from({ length: 200 }, (_, i) => ({
          path: "p" + i,
          reason: "negado",
        })),
      }),
      "find_large",
    );
    await userEvent.click(
      await screen.findByRole("button", { name: "Não verificados (250)" }),
    );
    expect(screen.getByText("Mostrando 200 de 250")).toBeInTheDocument();
    expect(screen.getByText("p199")).toBeInTheDocument();
  });
});
