import { cleanup, render as rtlRender, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import type { ScanItem, ScanSummary } from "../api";
import Seguranca from "./Seguranca";
import { JobsProvider } from "../jobs";

const render = (ui: React.ReactElement) => rtlRender(<JobsProvider>{ui}</JobsProvider>);

vi.mock("../api", () => ({
  scanPause: vi.fn(),
  scanPauseState: vi.fn(),
  relaunchElevated: vi.fn(),
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
  risk: 0,
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
  in_archive: null,
});

const peItem = (path = "C:\\x\\estranho.exe", sha256 = SHA_B): ScanItem => ({
  path,
  verdict: "suspicious",
  findings: [{ detector: "pe", rule: "pe-heuristics", severity: "suspicious", detail: "packed" }],
  sha256,
  recommended: false,
  why: ["Executável com sinais incomuns: packed."],
  in_archive: null,
});

const riskItem = (path = "C:\\x\\foto.jpg.exe", sha256 = "c".repeat(64)): ScanItem => ({
  path,
  verdict: "risk",
  findings: [{ detector: "context", rule: "double-extension", severity: "risk", detail: ".jpg.exe" }],
  sha256,
  recommended: false,
  why: ["O nome tem extensão dupla (.jpg.exe): parece um documento."],
  in_archive: null,
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
    risk: items.filter((i) => i.verdict === "risk").length,
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
      api.scanPause,
      api.relaunchElevated,
    ])
      vi.mocked(f).mockReset();
    vi.mocked(api.scanPauseState).mockReset().mockResolvedValue({ paused: false, reason: null });
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

  it("risk_level_is_listed_unchecked", async () => {
    render(<Seguranca />);
    await startQuick();
    await finish(withFindings(hashItem(), riskItem()));
    const risk = await screen.findByText(/^Risco \(1\)/);
    const mal = screen.getByText(/^Malicioso/);
    expect(mal.compareDocumentPosition(risk) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(screen.getByText(/extensão dupla/)).toBeVisible();
    expect(screen.getByText(/1 em risco/)).toBeInTheDocument();
    expect(
      screen.getByRole("checkbox", { name: "Selecionar C:\\x\\foto.jpg.exe" }),
    ).not.toBeChecked();
    // Só com achados de Risco não promete que nada foi encontrado.
    await finish(withFindings(riskItem()), 5);
    expect(screen.queryByText(/Nenhuma ameaça conhecida encontrada/)).toBeNull();
  });

  it("archive_warning_before_quarantine", async () => {
    const zip = { ...hashItem("C:\\x\\baixado.zip"), in_archive: "pasta/m.bin" };
    vi.mocked(api.scanGuard).mockResolvedValue([{ path: zip.path, guards: [] }]);
    vi.mocked(api.scanQuarantine).mockResolvedValue({ done: [], not_ended: [] });
    render(<Seguranca />);
    await startQuick();
    await finish(withFindings(zip));
    expect(await screen.findByText("dentro do compactado: pasta/m.bin")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Colocar em quarentena (1)" }));
    const dialog = screen.getByRole("dialog", { name: "Achado dentro de um compactado" });
    expect(dialog).toHaveTextContent("move o arquivo compactado inteiro");
    expect(dialog).toHaveTextContent("item: pasta/m.bin");
    expect(api.scanQuarantine).not.toHaveBeenCalled();
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Colocar o compactado em quarentena" }),
    );
    expect(api.scanQuarantine).toHaveBeenCalledTimes(1);
  });

  it("microsoft_signed_asks_again", async () => {
    const p = "C:\\x\\assinado.exe";
    vi.mocked(api.scanGuard).mockResolvedValue([
      { path: p, guards: [{ kind: "microsoft_signed", signer: "Microsoft Corporation" }] },
    ]);
    vi.mocked(api.scanQuarantine).mockResolvedValue({ done: [], not_ended: [] });
    render(<Seguranca />);
    await startQuick();
    await finish(withFindings(hashItem(p)));
    await userEvent.click(await screen.findByRole("button", { name: "Colocar em quarentena (1)" }));
    const dialog = screen.getByRole("dialog", { name: "Arquivos assinados pela Microsoft" });
    expect(dialog).toHaveTextContent("assinatura válida da Microsoft");
    expect(dialog).toHaveTextContent(`${p} — assinado por Microsoft Corporation`);
    expect(api.scanQuarantine).not.toHaveBeenCalled();
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Colocar em quarentena mesmo assim" }),
    );
    expect(api.scanQuarantine).toHaveBeenCalledTimes(1);
  });

  it("pause_and_resume", async () => {
    vi.mocked(api.scanPause)
      .mockResolvedValueOnce({ paused: true, reason: "user" })
      .mockResolvedValueOnce({ paused: false, reason: null });
    render(<Seguranca />);
    await startQuick();
    await userEvent.click(screen.getByRole("button", { name: "Pausar" }));
    expect(api.scanPause).toHaveBeenLastCalledWith(true);
    expect(await screen.findByText("Verificação pausada.")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Continuar" }));
    expect(api.scanPause).toHaveBeenLastCalledWith(false);
    expect(screen.queryByText("Verificação pausada.")).toBeNull();
  });

  it("paused_by_game", async () => {
    render(<Seguranca />);
    await startQuick();
    await act(async () => {
      listeners.get("scan:paused")!({ payload: { paused: true, reason: "game" } });
    });
    expect(screen.getByText(/pausada: jogo aberto/)).toBeInTheDocument();
    // O usuário não "continua" por cima do jogo.
    expect(screen.getByRole("button", { name: "Pausar" })).toBeDisabled();
    await act(async () => {
      listeners.get("scan:paused")!({ payload: { paused: false, reason: null } });
    });
    expect(screen.queryByText(/pausada: jogo aberto/)).toBeNull();
    expect(screen.getByRole("button", { name: "Pausar" })).toBeEnabled();
  });

  it("elevate_offer_on_access_denied", async () => {
    vi.mocked(api.relaunchElevated).mockResolvedValue(undefined);
    const denied = {
      errors: [
        { path: "C:\\Windows\\Tasks", reason: "Acesso negado. (os error 5)" },
        { path: "C:\\x", reason: "arquivo em uso" },
      ],
      errors_total: 2,
    };
    render(<Seguranca elevated={false} />);
    await startQuick();
    await finish(summary(denied));
    expect(await screen.findByText(/1 pasta ou arquivo ficou de fora por acesso negado/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Reiniciar como administrador" }));
    expect(api.relaunchElevated).toHaveBeenCalled();
  });

  it("não oferece administrador quando já é administrador ou sem acesso negado", async () => {
    render(<Seguranca />);
    await startQuick();
    await finish(
      summary({
        errors: [{ path: "C:\\a", reason: "Acesso negado. (os error 5)" }],
        errors_total: 1,
      }),
    );
    await screen.findByText(/^10 arquivos verificados/);
    expect(screen.queryByRole("button", { name: "Reiniciar como administrador" })).toBeNull();
    // Sem administrador, mas sem acesso negado nos erros: também não.
    cleanup();
    render(<Seguranca elevated={false} />);
    await startQuick();
    await finish(summary({ errors: [{ path: "C:\\a", reason: "em uso" }], errors_total: 1 }), 5);
    await screen.findByText(/^10 arquivos verificados/);
    expect(screen.queryByRole("button", { name: "Reiniciar como administrador" })).toBeNull();
  });

  it("explorer_request_starts_custom_scan", async () => {
    vi.mocked(api.scanStart).mockResolvedValue(5);
    const { rerender } = render(<Seguranca explorerRequest={null} />);
    expect(api.scanStart).not.toHaveBeenCalled();
    rerender(
      <JobsProvider>
        <Seguranca explorerRequest={{ id: 1, paths: ["C:\\Downloads\\x.exe"] }} />
      </JobsProvider>,
    );
    await vi.waitFor(() =>
      expect(api.scanStart).toHaveBeenCalledWith({
        kind: "custom",
        paths: ["C:\\Downloads\\x.exe"],
      }),
    );
    // O mesmo pedido não começa duas verificações.
    rerender(
      <JobsProvider>
        <Seguranca explorerRequest={{ id: 1, paths: ["C:\\Downloads\\x.exe"] }} />
      </JobsProvider>,
    );
    expect(api.scanStart).toHaveBeenCalledTimes(1);
  });
});
