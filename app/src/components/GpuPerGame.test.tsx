import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import type { GameItem, GamesPage } from "../api";
import GpuPerGame from "./GpuPerGame";

vi.mock("../api", () => ({
  gamesList: vi.fn(),
  gamesGpuSet: vi.fn(),
  gamesAdd: vi.fn(),
  gamesRemove: vi.fn(),
}));

const openDialog = vi.fn();
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...a: unknown[]) => openDialog(...a),
}));

const game = (id: string, over: Partial<GameItem> = {}): GameItem => ({
  id,
  name: `Jogo ${id}`,
  source: "steam",
  exe: `D:\\Jogos\\${id}\\${id}.exe`,
  exe_exists: true,
  enabled: false,
  in_journal: false,
  external: false,
  blocked: null,
  ...over,
});

const page = (games: GameItem[], over: Partial<GamesPage> = {}): GamesPage => ({
  games,
  missing: [],
  gpus: 2,
  warnings: [],
  ...over,
});

describe("GpuPerGame", () => {
  beforeEach(() => {
    vi.mocked(api.gamesList).mockReset();
    vi.mocked(api.gamesGpuSet).mockReset();
    vi.mocked(api.gamesAdd).mockReset();
    vi.mocked(api.gamesRemove).mockReset();
    openDialog.mockReset();
  });

  it("gpu_toggle_asks_confirmation", async () => {
    vi.mocked(api.gamesList).mockResolvedValue(page([game("a")]));
    vi.mocked(api.gamesGpuSet).mockResolvedValue(page([game("a", { enabled: true, in_journal: true })]));
    render(<GpuPerGame />);
    const sw = await screen.findByRole("switch", { name: "Alto desempenho: Jogo a (a.exe)" });
    await userEvent.click(sw);
    expect(api.gamesGpuSet).not.toHaveBeenCalled();
    expect(
      screen.getByRole("dialog", { name: 'Usar a GPU de alto desempenho em "Jogo a"?' }),
    ).toBeInTheDocument();
    expect(screen.getByText("D:\\Jogos\\a\\a.exe: GpuPreference → 2 (alto desempenho)")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Aplicar" }));
    expect(api.gamesGpuSet).toHaveBeenCalledWith("a", true);
    await waitFor(() => expect(screen.getByRole("switch")).toBeChecked());
  });

  it("turning off asks to go back to the Windows default", async () => {
    vi.mocked(api.gamesList).mockResolvedValue(
      page([game("a", { enabled: true, in_journal: true })]),
    );
    vi.mocked(api.gamesGpuSet).mockResolvedValue(page([game("a")]));
    render(<GpuPerGame />);
    await userEvent.click(await screen.findByRole("switch"));
    await userEvent.click(screen.getByRole("button", { name: "Desfazer" }));
    expect(api.gamesGpuSet).toHaveBeenCalledWith("a", false);
  });

  it("external value locks the switch", async () => {
    vi.mocked(api.gamesList).mockResolvedValue(
      page([game("a", { enabled: true, external: true })]),
    );
    render(<GpuPerGame />);
    expect(await screen.findByRole("switch")).toBeDisabled();
    expect(screen.getByText("Configurado fora do Sentinela.")).toBeInTheDocument();
  });

  it("add_game_uses_file_picker", async () => {
    vi.mocked(api.gamesList).mockResolvedValue(page([]));
    vi.mocked(api.gamesAdd).mockResolvedValue(
      page([game("b", { source: "manual", name: "b" })]),
    );
    openDialog.mockResolvedValue("C:\\Battle.net\\Overwatch\\Overwatch.exe");
    render(<GpuPerGame />);
    await userEvent.click(await screen.findByRole("button", { name: "Adicionar jogo…" }));
    expect(openDialog).toHaveBeenCalledWith(
      expect.objectContaining({
        multiple: false,
        filters: [{ name: "Programas", extensions: ["exe"] }],
      }),
    );
    expect(api.gamesAdd).toHaveBeenCalledWith("C:\\Battle.net\\Overwatch\\Overwatch.exe");
    expect(await screen.findByText("Adicionado por você")).toBeInTheDocument();
    // Jogo manual desligado pode sair da lista.
    vi.mocked(api.gamesRemove).mockResolvedValue(page([]));
    await userEvent.click(screen.getByRole("button", { name: "Remover da lista" }));
    expect(api.gamesRemove).toHaveBeenCalledWith("D:\\Jogos\\b\\b.exe");
  });

  it("cancelled file picker adds nothing", async () => {
    vi.mocked(api.gamesList).mockResolvedValue(page([]));
    openDialog.mockResolvedValue(null);
    render(<GpuPerGame />);
    await userEvent.click(await screen.findByRole("button", { name: "Adicionar jogo…" }));
    expect(api.gamesAdd).not.toHaveBeenCalled();
  });

  it("missing_game_label", async () => {
    vi.mocked(api.gamesList).mockResolvedValue(
      page([game("x", { exe_exists: false, source: "manual" })], {
        missing: [{ id: "gpu-1234abcd", title: "GPU de alto desempenho: Antigo" }],
      }),
    );
    vi.mocked(api.gamesGpuSet).mockResolvedValue(page([]));
    render(<GpuPerGame />);
    expect(await screen.findByText("Arquivo não encontrado")).toBeInTheDocument();
    expect(screen.getByText("Jogo não encontrado")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Desfazer" }));
    await userEvent.click(
      screen.getAllByRole("button", { name: "Desfazer" }).slice(-1)[0],
    );
    expect(api.gamesGpuSet).toHaveBeenCalledWith("gpu-1234abcd", false);
  });

  it("single_gpu_notice", async () => {
    vi.mocked(api.gamesList).mockResolvedValue(page([game("a")], { gpus: 1 }));
    render(<GpuPerGame />);
    expect(
      await screen.findByText("Esta máquina tem uma GPU; a preferência não muda nada."),
    ).toBeInTheDocument();
    // Continua disponível (o usuário pode trocar de placa depois).
    expect(screen.getByRole("switch")).toBeEnabled();
  });

  it("shows the error when toggling fails", async () => {
    vi.mocked(api.gamesList).mockResolvedValue(page([game("a")]));
    vi.mocked(api.gamesGpuSet).mockRejectedValue("falhou: acesso negado");
    render(<GpuPerGame />);
    await userEvent.click(await screen.findByRole("switch"));
    await userEvent.click(screen.getByRole("button", { name: "Aplicar" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("falhou: acesso negado");
  });
});
