import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import type { SystemStatus, UpdateSummary } from "../api";
import Ajustes from "./Ajustes";

vi.mock("../api", () => ({
  systemStatus: vi.fn(),
  signaturesUpdate: vi.fn(),
  abuseKeySet: vi.fn(),
  abuseKeyClear: vi.fn(),
  abuseKeyPresent: vi.fn(),
  jobCancel: vi.fn(),
}));

type Listener = (e: { payload: unknown }) => void;
const listeners = new Map<string, Listener>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, cb: Listener) => {
    listeners.set(name, cb);
    return Promise.resolve(() => listeners.delete(name));
  },
}));

const openUrl = vi.fn().mockResolvedValue(undefined);
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: (...a: unknown[]) => openUrl(...a),
}));

const status = (over: Partial<SystemStatus["db"]> = {}): SystemStatus => ({
  version: "0.3.0",
  elevated: false,
  db: {
    dir: "C:\dados\db",
    initialized: true,
    sources: [
      {
        name: "MalwareBazaar",
        url: "u",
        fetched_at: "2026-03-05T12:00:00Z",
        count: 1234,
      },
    ],
    warnings: [],
    ...over,
  },
  quarantine_count: 0,
  activity: [],
});

const summary = (over: Partial<UpdateSummary> = {}): UpdateSummary => ({
  hashes: 500,
  rules_ok: 7,
  rules_rejected: 2,
  rules_count: 321,
  warnings: [],
  ...over,
});

const finish = async (result: UpdateSummary, job = 9) => {
  await act(async () => {
    listeners.get("job:done")!({ payload: { job, kind: "update", result } });
  });
};

const SECRET = "chave-super-secreta-123";

describe("Ajustes", () => {
  beforeEach(() => {
    listeners.clear();
    vi.mocked(api.systemStatus).mockReset().mockResolvedValue(status());
    vi.mocked(api.signaturesUpdate).mockReset().mockResolvedValue(9);
    vi.mocked(api.abuseKeySet).mockReset().mockResolvedValue(undefined);
    vi.mocked(api.abuseKeyClear).mockReset().mockResolvedValue(undefined);
    vi.mocked(api.abuseKeyPresent).mockReset().mockResolvedValue(false);
  });

  it("mostra pasta, fontes com data e contagem", async () => {
    render(<Ajustes />);
    expect(await screen.findByText("C:\dados\db")).toBeInTheDocument();
    expect(screen.getByText("MalwareBazaar")).toBeInTheDocument();
    expect(screen.getByText(/1\.234/)).toBeInTheDocument();
  });

  it("banco não inicializado", async () => {
    vi.mocked(api.systemStatus).mockResolvedValue(
      status({ initialized: false, sources: [] }),
    );
    render(<Ajustes />);
    expect(await screen.findByText("Banco não inicializado")).toBeInTheDocument();
  });

  it("Atualizar agora mostra o resultado e os avisos, e fica desabilitado enquanto roda", async () => {
    render(<Ajustes />);
    const btn = await screen.findByRole("button", { name: "Atualizar agora" });
    await userEvent.click(btn);
    expect(api.signaturesUpdate).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("button", { name: /Atualizando/ })).toBeDisabled();
    await finish(summary({ warnings: ["Hashes ignorados"] }));
    expect(await screen.findByText("Hashes: 500")).toBeInTheDocument();
    expect(
      screen.getByText(
        "Regras YARA: 7 arquivo(s) aceito(s), 2 rejeitado(s), 321 regra(s) no total",
      ),
    ).toBeInTheDocument();
    expect(screen.getByText("Hashes ignorados")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Atualizar agora" })).toBeEnabled();
    expect(api.systemStatus).toHaveBeenCalledTimes(2);
  });

  it("mostra o erro da atualização", async () => {
    vi.mocked(api.signaturesUpdate).mockRejectedValue(
      "A chave da abuse.ch foi recusada. Confira a chave em Ajustes.",
    );
    render(<Ajustes />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Atualizar agora" }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "A chave da abuse.ch foi recusada",
    );
  });

  it("estado da chave: sem chave / chave salva", async () => {
    const { unmount } = render(<Ajustes />);
    expect(
      await screen.findByText("Sem chave (só as regras YARA são atualizadas)"),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Remover" })).toBeDisabled();
    unmount();
    vi.mocked(api.abuseKeyPresent).mockResolvedValue(true);
    render(<Ajustes />);
    expect(await screen.findByText("Chave salva")).toBeInTheDocument();
  });

  it("depois de Salvar o campo é esvaziado e a chave não aparece na página", async () => {
    const { container } = render(<Ajustes />);
    const field = await screen.findByLabelText("Chave de autenticação");
    expect(field).toHaveAttribute("type", "password");
    await userEvent.type(field, SECRET);
    vi.mocked(api.abuseKeyPresent).mockResolvedValue(true);
    await userEvent.click(screen.getByRole("button", { name: "Salvar" }));
    expect(api.abuseKeySet).toHaveBeenCalledWith(SECRET);
    expect(await screen.findByText("Chave salva")).toBeInTheDocument();
    expect(screen.getByLabelText("Chave de autenticação")).toHaveValue("");
    expect(container.innerHTML).not.toContain(SECRET);
    expect(document.body.textContent).not.toContain(SECRET);
  });

  it("erro ao salvar mostra a mensagem e mantém o campo vazio de segredo na página", async () => {
    vi.mocked(api.abuseKeySet).mockRejectedValue("Digite a chave.");
    render(<Ajustes />);
    await userEvent.click(await screen.findByRole("button", { name: "Salvar" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Digite a chave.");
  });

  it("Remover apaga a chave", async () => {
    vi.mocked(api.abuseKeyPresent).mockResolvedValue(true);
    render(<Ajustes />);
    await screen.findByText("Chave salva");
    vi.mocked(api.abuseKeyPresent).mockResolvedValue(false);
    await userEvent.click(screen.getByRole("button", { name: "Remover" }));
    expect(api.abuseKeyClear).toHaveBeenCalledTimes(1);
    expect(
      await screen.findByText("Sem chave (só as regras YARA são atualizadas)"),
    ).toBeInTheDocument();
  });

  it("o link abre exatamente o endereço da abuse.ch", async () => {
    render(<Ajustes />);
    await userEvent.click(await screen.findByText("auth.abuse.ch"));
    expect(openUrl).toHaveBeenCalledWith("https://auth.abuse.ch/");
  });
});
