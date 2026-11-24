import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import GameSessionCard from "./GameSession";

const api = vi.hoisted(() => ({
  gameSessionStatus: vi.fn(),
  gameSessionStart: vi.fn(),
  gameSessionEnd: vi.fn(),
}));
vi.mock("../api", () => api);

beforeEach(() => {
  api.gameSessionStatus.mockReset().mockResolvedValue(null);
  api.gameSessionStart.mockReset().mockResolvedValue({ previous_scheme: "x", started_at: "t" });
  api.gameSessionEnd.mockReset().mockResolvedValue(undefined);
});

it("pede confirmação antes de iniciar", async () => {
  render(<GameSessionCard />);
  await userEvent.click(await screen.findByText("Iniciar sessão de jogo"));
  expect(api.gameSessionStart).not.toHaveBeenCalled();
  await userEvent.click(screen.getByText("Iniciar"));
  await waitFor(() => expect(api.gameSessionStart).toHaveBeenCalled());
});

it("encerra uma sessão em andamento", async () => {
  api.gameSessionStatus.mockResolvedValue({ previous_scheme: "x", started_at: "t" });
  render(<GameSessionCard />);
  await userEvent.click(await screen.findByText("Encerrar sessão de jogo"));
  await waitFor(() => expect(api.gameSessionEnd).toHaveBeenCalled());
});
