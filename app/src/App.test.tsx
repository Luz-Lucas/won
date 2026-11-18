import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SystemStatus } from "./api";
import App from "./App";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

type Listener = (e: { payload: unknown }) => void;
const listeners = new Map<string, Listener>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, cb: Listener) => {
    listeners.set(name, cb);
    return Promise.resolve(() => listeners.delete(name));
  },
}));

const emit = async (name: string, payload: unknown) => {
  await act(async () => {
    listeners.get(name)!({ payload });
  });
};

const statusCalls = () =>
  invokeMock.mock.calls.filter(([cmd]) => cmd === "system_status").length;

const scanSummary = {
  scanned: 3,
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
};

const status = (over: Partial<SystemStatus> = {}): SystemStatus => ({
  version: "0.3.0",
  elevated: false,
  db: {
    dir: "C:\db",
    initialized: true,
    sources: [
      { name: "a", url: "u", fetched_at: "2026-03-05T12:00:00Z", count: 3 },
    ],
    warnings: [],
  },
  quarantine_count: 2,
  activity: [
    { at: "2026-03-05T12:00:00Z", kind: "clean", summary: "Limpeza concluída" },
  ],
  ...over,
});

describe("App", () => {
  beforeEach(() => {
    listeners.clear();
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case "clean_plan":
          return {
            categories: [],
            total_files: 0,
            total_bytes: 0,
            deleted: 0,
            freed: 0,
            elevated: false,
          };
        case "job_running":
          return null;
        case "scan_start":
          return 5;
        case "quarantine_list":
          return [];
        default:
          return status();
      }
    });
  });

  it("sair de Segurança durante a verificação não perde o job", async () => {
    render(<App />);
    await userEvent.click(screen.getByRole("button", { name: "Segurança" }));
    await userEvent.click(screen.getByRole("button", { name: "Completa" }));
    expect(
      screen.getByRole("button", { name: "Cancelar" }),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Início" }));
    await userEvent.click(screen.getByRole("button", { name: "Segurança" }));
    expect(screen.getByRole("button", { name: "Completa" })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "Cancelar" }));
    expect(invokeMock).toHaveBeenCalledWith("job_cancel", { id: 5 });
    await emit("job:done", { job: 5, kind: "scan", result: scanSummary });
    expect(
      await screen.findByText(/3 arquivos verificados/),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Cancelar" }),
    ).not.toBeInTheDocument();
    expect(
      invokeMock.mock.calls.filter(([cmd]) => cmd === "scan_start"),
    ).toHaveLength(1);
  });

  it("Início recarrega o estado ao voltar e quando um job termina", async () => {
    render(<App />);
    expect(await screen.findByText("2 na quarentena")).toBeInTheDocument();
    expect(statusCalls()).toBe(1);
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "job_running" || cmd === "quarantine_list"
        ? cmd === "quarantine_list"
          ? []
          : null
        : status({ quarantine_count: 5 }),
    );
    await emit("job:done", { job: 1, kind: "update", result: {} });
    expect(await screen.findByText("5 na quarentena")).toBeInTheDocument();
    expect(statusCalls()).toBe(2);
    await userEvent.click(screen.getByRole("button", { name: "Segurança" }));
    await userEvent.click(screen.getByRole("button", { name: "Início" }));
    await waitFor(() => expect(statusCalls()).toBe(3));
  });

  it("restaurar da quarentena recarrega o estado do Início", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "job_running") return null;
      if (cmd === "quarantine_list")
        return [
          {
            id: "q1",
            original_path: "C:\\x\\a.exe",
            sha256: "00",
            size: 1,
            rule: "R",
            quarantined_at: "2026-03-05T12:00:00Z",
          },
        ];
      if (cmd === "quarantine_restore") return "C:\\x\\a.exe";
      return status();
    });
    render(<App />);
    await screen.findByText("2 na quarentena");
    await userEvent.click(screen.getByRole("button", { name: "Segurança" }));
    const before = statusCalls();
    await userEvent.click(
      await screen.findByRole("button", { name: "Restaurar" }),
    );
    await userEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", { name: "Restaurar" }),
    );
    await screen.findByText("Restaurado: C:\\x\\a.exe");
    await waitFor(() => expect(statusCalls()).toBe(before + 1));
  });

  it("mostra as cinco páginas e troca de página", async () => {
    render(<App />);
    for (const n of ["Início", "Limpeza", "Segurança", "Disco", "Ajustes"])
      expect(screen.getByRole("button", { name: n })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Limpeza" }));
    expect(
      screen.getByRole("heading", { level: 1, name: "Limpeza" }),
    ).toBeInTheDocument();
  });

  it("sem privilégio, oferece reiniciar como administrador", async () => {
    render(<App />);
    expect(
      await screen.findByRole("button", { name: "Reiniciar como administrador" }),
    ).toBeInTheDocument();
    expect(screen.getByText("Administrador: não")).toBeInTheDocument();
  });

  it("com privilégio, não oferece reiniciar", async () => {
    invokeMock.mockResolvedValue(status({ elevated: true }));
    render(<App />);
    expect(await screen.findByText("Administrador: sim")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Reiniciar como administrador" }),
    ).not.toBeInTheDocument();
  });

  it("Início mostra banco, quarentena, versão, atividade e aviso", async () => {
    render(<App />);
    expect(await screen.findByText(/Banco atualizado em/)).toBeInTheDocument();
    expect(screen.getByText("2 na quarentena")).toBeInTheDocument();
    expect(screen.getByText("Versão 0.3.0")).toBeInTheDocument();
    expect(screen.getByText("Limpeza concluída")).toBeInTheDocument();
    expect(
      screen.getByText(
        "O Sentinela não substitui um antivírus completo. Mantenha o Microsoft Defender ativo.",
      ),
    ).toBeInTheDocument();
  });

  it("banco não inicializado e atividade vazia", async () => {
    invokeMock.mockResolvedValue(
      status({
        db: { dir: "x", initialized: false, sources: [], warnings: [] },
        activity: [],
      }),
    );
    render(<App />);
    expect(await screen.findByText("Banco não inicializado")).toBeInTheDocument();
    expect(screen.getByText("Nenhuma atividade ainda.")).toBeInTheDocument();
  });

  it("cartões levam a Segurança e Limpeza", async () => {
    render(<App />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Ir para Segurança" }),
    );
    expect(
      screen.getByRole("heading", { level: 1, name: "Segurança" }),
    ).toBeInTheDocument();
  });

  it("falha em system_status mostra mensagem, não página em branco", async () => {
    invokeMock.mockRejectedValue("falhou");
    render(<App />);
    expect(
      await screen.findByText(/Não foi possível carregar o estado do sistema/),
    ).toBeInTheDocument();
  });

  it("recusar o UAC mostra a mensagem e mantém o botão", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "relaunch_elevated") throw "Elevação cancelada.";
      return status();
    });
    render(<App />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Reiniciar como administrador" }),
    );
    expect(await screen.findByText("Elevação cancelada.")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Reiniciar como administrador" }),
    ).toBeInTheDocument();
  });
});
