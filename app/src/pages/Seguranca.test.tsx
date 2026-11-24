import { render as rtlRender, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import type { ScanItem, ScanSummary } from "../api";
import Seguranca from "./Seguranca";
import { JobsProvider } from "../jobs";

const render = (ui: React.ReactElement) => rtlRender(<JobsProvider>{ui}</JobsProvider>);

vi.mock("../api", () => ({
  scanStart: vi.fn(),
  scanGuard: vi.fn(),
  scanQuarantine: vi.fn(),
  scanExceptionAdd: vi.fn(),
  scanScheduleGet: vi.fn(),
  scanScheduleSet: vi.fn(),
  scanScheduleDismiss: vi.fn(),
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
const openUrl = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/plugin-opener", () => ({
  revealItemInDir: (...a: unknown[]) => reveal(...a),
  openUrl: (...a: unknown[]) => openUrl(...a),
}));

const summary = (over: Partial<ScanSummary> = {}): ScanSummary => ({
  scanned: 10,
  interrupted: false,
  malicious: 0,
  suspicious: 0,
  items: [],
  items_total: 0,
  ignored: 0,
  errors: [],
  errors_total: 0,
  missing: [],
  db_initialized: true,
  warnings: [],
  ...over,
});

const SHA_A = "a".repeat(64);
const SHA_B = "b".repeat(64);

const hashItem = (path = "C:\\x\\mal.exe", sha256 = SHA_A): ScanItem => ({
  path,
  verdict: "malicious",
  findings: [{ detector: "hash", rule: `sha256:${sha256}`, severity: "malicious", detail: "" }],
  sha256,
  recommended: true,
  why: ["O SHA-256 deste arquivo está na lista pública de malware conhecido."],
});

const peItem = (path = "C:\\x\\estranho.exe", sha256 = SHA_B): ScanItem => ({
  path,
  verdict: "suspicious",
  findings: [{ detector: "pe", rule: "pe-heuristics", severity: "suspicious", detail: "packed" }],
  sha256,
  recommended: false,
  why: ["Executável com sinais incomuns: packed."],
});

const finish = async (result: ScanSummary, job = 5) => {
  await act(async () => {
    listeners.get("job:done")!({ payload: { job, kind: "scan", result } });
  });
};

const startQuick = async () => {
  vi.mocked(api.scanStart).mockResolvedValue(5);
  await userEvent.click(screen.getByRole("button", { name: "Rápida" }));
};

const withFindings = (...items: ScanItem[]) =>
  summary({
    malicious: items.filter((i) => i.verdict === "malicious").length,
    suspicious: items.filter((i) => i.verdict === "suspicious").length,
    items,
    items_total: items.length,
  });

describe("Segurança", () => {
  beforeEach(() => {
    for (const f of [
      api.scanStart,
      api.scanGuard,
      api.scanQuarantine,
      api.scanExceptionAdd,
      api.scanScheduleSet,
      api.scanScheduleDismiss,
      api.jobCancel,
    ])
      vi.mocked(f).mockReset();
    vi.mocked(api.scanScheduleGet)
      .mockReset()
      .mockResolvedValue({ scheduled: false, auto_quarantine: false, offered: true });
    openDialog.mockReset();
    reveal.mockReset();
    openUrl.mockClear();
    listeners.clear();
  });

  it("mostra achados por nível, o porquê, e as pastas puladas", async () => {
    render(<Seguranca />);
    await startQuick();
    await finish({ ...withFindings(hashItem(), peItem()), missing: ["D:\\Downloads"] });
    const mal = await screen.findByText(/^Malicioso/);
    const sus = screen.getByText(/^Suspeito/);
    expect(mal.compareDocumentPosition(sus) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.getByText(/lista pública de malware conhecido/)).toBeVisible();
    expect(screen.getByText("Executável com sinais incomuns: packed.")).toBeVisible();
    expect(screen.getByText("Não foi possível verificar: D:\\Downloads")).toBeInTheDocument();
  });

  it("avisa que é sob demanda e que o Defender continua ligado", () => {
    render(<Seguranca />);
    expect(screen.getByText(/não há proteção em tempo real/)).toBeInTheDocument();
    expect(screen.getByText(/Mantenha o Microsoft Defender ligado/)).toBeInTheDocument();
  });

  it("resultado interrompido diz que é parcial", async () => {
    render(<Seguranca />);
    await startQuick();
    await finish(summary({ interrupted: true }));
    expect(
      await screen.findByText("Verificação interrompida: resultado parcial."),
    ).toBeInTheDocument();
    expect(screen.queryByText(/Nenhuma ameaça conhecida encontrada/)).not.toBeInTheDocument();
  });

  it("nenhum arquivo verificado não diz que nada foi encontrado", async () => {
    render(<Seguranca />);
    await startQuick();
    await finish(summary({ scanned: 0 }));
    expect(await screen.findByText(/0 arquivos verificados/)).toBeInTheDocument();
    expect(screen.queryByText(/Nenhuma ameaça conhecida encontrada/)).not.toBeInTheDocument();
  });

  it("sem achados não promete segurança", async () => {
    render(<Seguranca />);
    await startQuick();
    await finish(summary());
    expect(
      await screen.findByText("Nenhuma ameaça conhecida encontrada nos arquivos verificados."),
    ).toBeInTheDocument();
    expect(screen.queryByText(/limpo|seguro/i)).not.toBeInTheDocument();
  });

  it("durante a verificação os botões ficam desabilitados e dá para cancelar", async () => {
    render(<Seguranca />);
    await startQuick();
    for (const n of ["Rápida", "Completa", "Personalizada…"])
      expect(screen.getByRole("button", { name: n })).toBeDisabled();
    vi.mocked(api.jobCancel).mockRejectedValue("falhou");
    await userEvent.click(screen.getByRole("button", { name: "Cancelar" }));
    expect(api.jobCancel).toHaveBeenCalledWith(5);
  });

  it("mostra o erro quando a verificação não pode começar", async () => {
    vi.mocked(api.scanStart).mockRejectedValue("Nenhuma pasta disponível para verificar.");
    render(<Seguranca />);
    await userEvent.click(screen.getByRole("button", { name: "Rápida" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Nenhuma pasta disponível para verificar.",
    );
    expect(screen.getByRole("button", { name: "Rápida" })).toBeEnabled();
  });

  it("Personalizada envia as pastas escolhidas e ignora o cancelamento do seletor", async () => {
    render(<Seguranca />);
    openDialog.mockResolvedValueOnce(null);
    await userEvent.click(screen.getByRole("button", { name: "Personalizada…" }));
    expect(api.scanStart).not.toHaveBeenCalled();
    expect(openDialog).toHaveBeenCalledWith({ directory: true, multiple: true });
    openDialog.mockResolvedValueOnce(["C:\\pasta", "E:\\"]);
    vi.mocked(api.scanStart).mockResolvedValue(5);
    await userEvent.click(screen.getByRole("button", { name: "Personalizada…" }));
    expect(api.scanStart).toHaveBeenCalledWith({ kind: "custom", paths: ["C:\\pasta", "E:\\"] });
  });

  it("Completa avisa que demora e que sem administrador é parcial", async () => {
    render(<Seguranca />);
    expect(screen.getByText(/A completa lê todos os discos fixos/)).toHaveTextContent(
      "Sem administrador, algumas pastas do sistema ficam de fora.",
    );
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
        items: Array.from({ length: 300 }, (_, i) => peItem(`C:\\a${i}.exe`, `${i}`.padStart(64, "0"))),
        errors: [{ path: "C:\\Sistema\\x", reason: "acesso negado" }],
        errors_total: 1,
      }),
    );
    expect(
      await screen.findByText("Mostrando 300 de 1.500", {}, { timeout: 5000 }),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /Não verificados/ }));
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
    await userEvent.click(screen.getByRole("button", { name: "Ir para Ajustes" }));
    expect(onNavigate).toHaveBeenCalledWith("ajustes");
  });

  it("Abrir pasta revela o arquivo e Detalhes mostra o SHA-256", async () => {
    render(<Seguranca />);
    await startQuick();
    await finish(withFindings(hashItem()));
    await userEvent.click(await screen.findByRole("button", { name: "Abrir pasta" }));
    expect(reveal).toHaveBeenCalledWith("C:\\x\\mal.exe");
    await userEvent.click(screen.getByRole("button", { name: "Detalhes" }));
    expect(screen.getByText(`SHA-256: ${SHA_A}`)).toBeInTheDocument();
  });

  it("hash_hits_come_checked", async () => {
    render(<Seguranca />);
    await startQuick();
    await finish(withFindings(hashItem(), peItem()));
    expect(
      await screen.findByRole("checkbox", { name: "Selecionar C:\\x\\mal.exe" }),
    ).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Selecionar C:\\x\\estranho.exe" })).not.toBeChecked();
    expect(screen.getByRole("button", { name: "Colocar em quarentena (1)" })).toBeEnabled();
    // Nada se move sem o clique.
    expect(api.scanQuarantine).not.toHaveBeenCalled();
  });

  it("quarentena sem travas move e tira da lista", async () => {
    vi.mocked(api.scanGuard).mockResolvedValue([{ path: "C:\\x\\mal.exe", guards: [] }]);
    vi.mocked(api.scanQuarantine).mockResolvedValue({
      done: [{ path: "C:\\x\\mal.exe", status: "quarantined", id: "q1" }],
      not_ended: [],
    });
    render(<Seguranca />);
    await startQuick();
    await finish(withFindings(hashItem(), peItem()));
    await userEvent.click(await screen.findByRole("button", { name: "Colocar em quarentena (1)" }));
    expect(api.scanQuarantine).toHaveBeenCalledWith(
      [{ path: "C:\\x\\mal.exe", sha256: SHA_A, rule: `sha256:${SHA_A}` }],
      [],
    );
    expect(await screen.findByText("1 arquivo colocado em quarentena.")).toBeInTheDocument();
    expect(screen.queryByRole("checkbox", { name: "Selecionar C:\\x\\mal.exe" })).toBeNull();
  });

  it("quarantine_asks_again_for_windows_folder", async () => {
    const sys = "C:\\Windows\\System32\\x.dll";
    vi.mocked(api.scanGuard).mockResolvedValue([{ path: sys, guards: [{ kind: "system_folder" }] }]);
    vi.mocked(api.scanQuarantine).mockResolvedValue({ done: [], not_ended: [] });
    render(<Seguranca />);
    await startQuick();
    await finish(withFindings(hashItem(sys)));
    await userEvent.click(await screen.findByRole("button", { name: "Colocar em quarentena (1)" }));
    const dialog = screen.getByRole("dialog", { name: "Arquivos da pasta do Windows" });
    expect(dialog).toHaveTextContent(sys);
    expect(api.scanQuarantine).not.toHaveBeenCalled();
    // Cancelar não move nada.
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancelar" }));
    expect(api.scanQuarantine).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Colocar em quarentena (1)" }));
    await userEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", {
        name: "Colocar em quarentena mesmo assim",
      }),
    );
    expect(api.scanQuarantine).toHaveBeenCalledTimes(1);
  });

  it("in_use_offers_to_end_process", async () => {
    vi.mocked(api.scanGuard).mockResolvedValue([
      { path: "C:\\x\\mal.exe", guards: [{ kind: "in_use", pids: [42], names: ["mal.exe"] }] },
    ]);
    vi.mocked(api.scanQuarantine).mockResolvedValue({
      done: [{ path: "C:\\x\\mal.exe", status: "in_use" }],
      not_ended: [42],
    });
    render(<Seguranca />);
    await startQuick();
    await finish(withFindings(hashItem()));
    await userEvent.click(await screen.findByRole("button", { name: "Colocar em quarentena (1)" }));
    const dialog = screen.getByRole("dialog", { name: "Programas abertos" });
    expect(dialog).toHaveTextContent("mal.exe — C:\\x\\mal.exe");
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Encerrar e colocar em quarentena" }),
    );
    expect(api.scanQuarantine).toHaveBeenCalledWith(expect.any(Array), [42]);
    expect(
      await screen.findByText("Em uso, continua no lugar: C:\\x\\mal.exe"),
    ).toBeInTheDocument();
    expect(screen.getByText("Não foi possível encerrar: 42")).toBeInTheDocument();
  });

  it("ignore_always_adds_exception", async () => {
    vi.mocked(api.scanExceptionAdd).mockResolvedValue([]);
    render(<Seguranca />);
    await startQuick();
    await finish(withFindings(peItem()));
    await userEvent.click(await screen.findByRole("button", { name: "Ignorar sempre" }));
    const dialog = screen.getByRole("dialog", { name: "Ignorar sempre?" });
    expect(dialog).toHaveTextContent("Dá para desfazer em Ajustes");
    await userEvent.click(within(dialog).getByRole("button", { name: "Ignorar sempre" }));
    expect(api.scanExceptionAdd).toHaveBeenCalledWith(SHA_B, "C:\\x\\estranho.exe", "pe-heuristics");
    expect(await screen.findByText(/1 achado ignorado/)).toBeInTheDocument();
    expect(screen.queryByText("C:\\x\\estranho.exe")).toBeNull();
  });

  it("ignorar malware conhecido avisa", async () => {
    render(<Seguranca />);
    await startQuick();
    await finish(withFindings(hashItem()));
    await userEvent.click(await screen.findByRole("button", { name: "Ignorar sempre" }));
    expect(screen.getByRole("dialog")).toHaveTextContent("lista de malware conhecido");
  });

  it("virustotal_opens_hash_url", async () => {
    render(<Seguranca />);
    await startQuick();
    await finish(withFindings(hashItem()));
    await userEvent.click(await screen.findByRole("button", { name: "Consultar no VirusTotal" }));
    expect(openUrl).toHaveBeenCalledWith(`https://www.virustotal.com/gui/file/${SHA_A}`);
  });

  it("schedule_offer_after_first_scan", async () => {
    vi.mocked(api.scanScheduleGet).mockResolvedValue({
      scheduled: false,
      auto_quarantine: false,
      offered: false,
    });
    vi.mocked(api.scanScheduleSet).mockResolvedValue({
      scheduled: true,
      auto_quarantine: false,
      offered: true,
    });
    render(<Seguranca />);
    expect(screen.queryByText(/Agendar uma verificação rápida/)).toBeNull();
    await startQuick();
    await finish(summary());
    expect(await screen.findByText(/Agendar uma verificação rápida/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Agendar" }));
    expect(api.scanScheduleSet).toHaveBeenCalledWith(true);
    expect(screen.queryByText(/Agendar uma verificação rápida/)).toBeNull();
  });

  it("Agora não marca a oferta como respondida", async () => {
    vi.mocked(api.scanScheduleGet).mockResolvedValue({
      scheduled: false,
      auto_quarantine: false,
      offered: false,
    });
    vi.mocked(api.scanScheduleDismiss).mockResolvedValue({
      scheduled: false,
      auto_quarantine: false,
      offered: true,
    });
    render(<Seguranca />);
    await startQuick();
    await finish(summary());
    await userEvent.click(await screen.findByRole("button", { name: "Agora não" }));
    expect(api.scanScheduleDismiss).toHaveBeenCalled();
    expect(api.scanScheduleSet).not.toHaveBeenCalled();
  });

  it("Ver quarentena leva à página", async () => {
    const onNavigate = vi.fn();
    render(<Seguranca onNavigate={onNavigate} />);
    await userEvent.click(screen.getByRole("button", { name: "Ver quarentena" }));
    expect(onNavigate).toHaveBeenCalledWith("quarentena");
  });
});
