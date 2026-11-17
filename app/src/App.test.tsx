import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SystemStatus } from "./api";
import App from "./App";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

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
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(status());
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
