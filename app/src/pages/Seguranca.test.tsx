import { render as rtlRender, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import type { QuarantineEntry, ScanSummary } from "../api";
import Seguranca from "./Seguranca";
import { JobsProvider } from "../jobs";

const render = (ui: React.ReactElement) =>
  rtlRender(<JobsProvider>{ui}</JobsProvider>);

vi.mock("../api", () => ({
  scanStart: vi.fn(),
  quarantineList: vi.fn(),
  quarantineRestore: vi.fn(),
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

const openDialog = vi.fn();
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...a: unknown[]) => openDialog(...a),
}));

const reveal = vi.fn();
vi.mock("@tauri-apps/plugin-opener", () => ({
  revealItemInDir: (...a: unknown[]) => reveal(...a),
}));

const summary = (over: Partial<ScanSummary> = {}): ScanSummary => ({
  scanned: 10,
  interrupted: false,
  malicious: 0,
  suspicious: 0,
  items: [],
  items_total: 0,
  errors: [],
  errors_total: 0,
  missing: [],
  db_initialized: true,
  warnings: [],
  ...over,
});

const finding = (
  rule: string,
  severity: "malicious" | "suspicious",
  detector = "yara",
) => ({ detector, rule, severity, detail: "" });

const finish = async (result: ScanSummary, job = 5) => {
  await act(async () => {
    listeners.get("job:done")!({ payload: { job, kind: "scan", result } });
  });
};

const startQuick = async () => {
  vi.mocked(api.scanStart).mockResolvedValue(5);
  await userEvent.click(screen.getByRole("button", { name: "Rápida" }));
};

const entry = (over: Partial<QuarantineEntry> = {}): QuarantineEntry => ({
  id: "abc",
  original_path: "C:\\x\\mal.exe",
  sha256: "00",
  size: 2048,
  rule: "EICAR_Test_File",
  quarantined_at: "2026-03-05T12:00:00Z",
  ...over,
});

const confirmRestore = async () => {
  await userEvent.click(screen.getByRole("button", { name: "Restaurar" }));
  await userEvent.click(
    within(screen.getByRole("dialog")).getByRole("button", {
      name: "Restaurar",
    }),
  );
};

describe("Segurança", () => {
  beforeEach(() => {
    vi.mocked(api.scanStart).mockReset();
    vi.mocked(api.quarantineList).mockReset().mockResolvedValue([]);
    vi.mocked(api.quarantineRestore).mockReset();
    vi.mocked(api.jobCancel).mockReset();
    openDialog.mockReset();
    reveal.mockReset();
    listeners.clear();
  });

  it("mostra achados por nível, o motivo, e as pastas puladas", async () => {
    render(<Seguranca />);
    await startQuick();
    await finish(
      summary({
        malicious: 1,
        suspicious: 1,
        items_total: 2,
        items: [
          {
            path: "C:\\x\\eicar.com",
            verdict: "malicious",
            findings: [finding("EICAR_Test_File", "malicious")],
          },
          {
            path: "C:\\x\\estranho.exe",
            verdict: "suspicious",
            findings: [finding("packed_section", "suspicious", "pe")],
          },
        ],
        missing: ["D:\\Downloads"],
      }),
    );
    const mal = await screen.findByText(/^Malicioso/);
    const sus = screen.getByText(/^Suspeito/);
    expect(
      mal.compareDocumentPosition(sus) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(screen.getByText(/EICAR_Test_File/)).toBeVisible();
    expect(screen.getByText(/packed_section/)).toBeVisible();
    expect(
      screen.getByText("Não foi possível verificar: D:\\Downloads"),
    ).toBeInTheDocument();
  });

  it("resultado interrompido diz que é parcial", async () => {
    render(<Seguranca />);
    await startQuick();
    await finish(summary({ interrupted: true }));
    expect(
      await screen.findByText("Verificação interrompida: resultado parcial."),
    ).toBeInTheDocument();
    expect(
      screen.queryByText(/Nenhuma ameaça conhecida encontrada/),
    ).not.toBeInTheDocument();
  });

  it("nenhum arquivo verificado não diz que nada foi encontrado", async () => {
    render(<Seguranca />);
    await startQuick();
    await finish(summary({ scanned: 0 }));
    expect(await screen.findByText(/0 arquivos verificados/)).toBeInTheDocument();
    expect(
      screen.queryByText(/Nenhuma ameaça conhecida encontrada/),
    ).not.toBeInTheDocument();
  });

  it("sem achados não promete segurança", async () => {
    render(<Seguranca />);
    await startQuick();
    await finish(summary());
    expect(
      await screen.findByText(
        "Nenhuma ameaça conhecida encontrada nos arquivos verificados.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText(/limpo|seguro/i)).not.toBeInTheDocument();
  });

  it("durante a verificação os botões ficam desabilitados e dá para cancelar", async () => {
    render(<Seguranca />);
    await startQuick();
    for (const n of ["Rápida", "Completa", "Pasta…"])
      expect(screen.getByRole("button", { name: n })).toBeDisabled();
    vi.mocked(api.jobCancel).mockRejectedValue("falhou");
    await userEvent.click(screen.getByRole("button", { name: "Cancelar" }));
    expect(api.jobCancel).toHaveBeenCalledWith(5);
  });

  it("mostra o erro quando a verificação não pode começar", async () => {
    vi.mocked(api.scanStart).mockRejectedValue(
      "Nenhuma pasta disponível para verificar.",
    );
    render(<Seguranca />);
    await userEvent.click(screen.getByRole("button", { name: "Rápida" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Nenhuma pasta disponível para verificar.",
    );
    expect(screen.getByRole("button", { name: "Rápida" })).toBeEnabled();
  });

  it("Pasta… envia a pasta escolhida e ignora o cancelamento do seletor", async () => {
    render(<Seguranca />);
    openDialog.mockResolvedValueOnce(null);
    await userEvent.click(screen.getByRole("button", { name: "Pasta…" }));
    expect(api.scanStart).not.toHaveBeenCalled();
    openDialog.mockResolvedValueOnce("C:\\pasta");
    vi.mocked(api.scanStart).mockResolvedValue(5);
    await userEvent.click(screen.getByRole("button", { name: "Pasta…" }));
    expect(api.scanStart).toHaveBeenCalledWith({
      kind: "custom",
      paths: ["C:\\pasta"],
    });
  });

  it("Completa avisa que demora e que sem administrador é parcial", async () => {
    render(<Seguranca />);
    expect(
      screen.getByText(
        "A verificação completa lê todos os discos fixos e pode demorar bastante. Sem administrador, algumas pastas do sistema ficam de fora.",
      ),
    ).toBeInTheDocument();
    vi.mocked(api.scanStart).mockResolvedValue(5);
    await userEvent.click(screen.getByRole("button", { name: "Completa" }));
    expect(api.scanStart).toHaveBeenCalledWith({ kind: "full" });
  });

  it("limita a lista e avisa o total; mostra os não verificados", async () => {
    render(<Seguranca />);
    await startQuick();
    await finish(
      summary({
        suspicious: 1500,
        items_total: 1500,
        items: Array.from({ length: 1000 }, (_, i) => ({
          path: `C:\\a${i}.exe`,
          verdict: "suspicious" as const,
          findings: [finding("r", "suspicious", "pe")],
        })),
        errors: [{ path: "C:\\Sistema\\x", reason: "acesso negado" }],
        errors_total: 1,
      }),
    );
    // Desenhar 1.000 linhas no jsdom é lento com a máquina sob carga.
    expect(
      await screen.findByText("Mostrando 1.000 de 1.500", {}, { timeout: 5000 }),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: /Não verificados/ }),
    );
    expect(screen.getByText(/C:\\Sistema\\x/)).toBeInTheDocument();
    expect(screen.getByText(/acesso negado/)).toBeInTheDocument();
  }, 15000);

  it("banco não inicializado avisa e leva a Ajustes", async () => {
    const onNavigate = vi.fn();
    render(<Seguranca onNavigate={onNavigate} />);
    await startQuick();
    await finish(summary({ db_initialized: false }));
    expect(
      await screen.findByText(
        "Banco de assinaturas não inicializado: só a assinatura de teste EICAR e as heurísticas estão ativas.",
      ),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Ir para Ajustes" }),
    );
    expect(onNavigate).toHaveBeenCalledWith("ajustes");
  });

  it("Abrir pasta revela o arquivo", async () => {
    render(<Seguranca />);
    await startQuick();
    await finish(
      summary({
        malicious: 1,
        items_total: 1,
        items: [
          {
            path: "C:\\x\\eicar.com",
            verdict: "malicious",
            findings: [finding("R", "malicious")],
          },
        ],
      }),
    );
    await userEvent.click(
      await screen.findByRole("button", { name: "Abrir pasta" }),
    );
    expect(reveal).toHaveBeenCalledWith("C:\\x\\eicar.com");
  });

  it("quarentena vazia mostra o estado vazio", async () => {
    render(<Seguranca />);
    expect(
      await screen.findByText("Nenhum arquivo em quarentena."),
    ).toBeInTheDocument();
  });

  it("restaura e mostra o erro do restore como está", async () => {
    vi.mocked(api.quarantineList).mockResolvedValue([entry()]);
    vi.mocked(api.quarantineRestore).mockRejectedValueOnce(
      "o destino já existe",
    );
    render(<Seguranca />);
    expect(await screen.findByText("C:\\x\\mal.exe")).toBeInTheDocument();
    expect(screen.getByText(/EICAR_Test_File/)).toBeInTheDocument();
    await confirmRestore();
    expect(api.quarantineRestore).toHaveBeenCalledWith("abc");
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "o destino já existe",
    );
    vi.mocked(api.quarantineRestore).mockResolvedValueOnce("C:\\x\\mal.exe");
    vi.mocked(api.quarantineList).mockResolvedValue([]);
    await confirmRestore();
    expect(
      await screen.findByText("Nenhum arquivo em quarentena."),
    ).toBeInTheDocument();
    expect(screen.getByText("Restaurado: C:\\x\\mal.exe")).toBeInTheDocument();
  });

  it("Restaurar pede confirmação com o caminho e a regra", async () => {
    vi.mocked(api.quarantineList).mockResolvedValue([entry()]);
    vi.mocked(api.quarantineRestore).mockResolvedValue("C:\\x\\mal.exe");
    const onRestored = vi.fn();
    render(<Seguranca onRestored={onRestored} />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Restaurar" }),
    );
    expect(api.quarantineRestore).not.toHaveBeenCalled();
    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveTextContent(
      "Restaurar C:\\x\\mal.exe? Este arquivo foi colocado em quarentena por: EICAR_Test_File.",
    );
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Restaurar" }),
    );
    expect(api.quarantineRestore).toHaveBeenCalledWith("abc");
    await screen.findByText("Restaurado: C:\\x\\mal.exe");
    expect(onRestored).toHaveBeenCalledTimes(1);
  });

  it("Cancelar e Escape não restauram", async () => {
    vi.mocked(api.quarantineList).mockResolvedValue([entry()]);
    render(<Seguranca />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Restaurar" }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Cancelar" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Restaurar" }));
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(api.quarantineRestore).not.toHaveBeenCalled();
  });
});
