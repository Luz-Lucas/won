import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import OrganizeAccount from "./OrganizeAccount";

vi.mock("../api", () => ({
  organizeStatus: vi.fn(),
  organizeLogin: vi.fn(),
  organizeRefresh: vi.fn(),
  organizeLogout: vi.fn(),
}));

const off = { connected: false, url: "", email: "", fetched_at: 0, events: 0, tasks: 0, error: "" };

describe("OrganizeAccount", () => {
  beforeEach(() => {
    vi.mocked(api.organizeStatus).mockResolvedValue(off);
  });

  it("só habilita Conectar com tudo preenchido e mostra o resultado", async () => {
    vi.mocked(api.organizeLogin).mockResolvedValue({
      connected: true, url: "https://x.supabase.co", email: "a@b.c", fetched_at: 1700000000, events: 12, tasks: 3, error: "",
    });
    render(<OrganizeAccount />);
    const btn = await screen.findByRole("button", { name: "Conectar" });
    expect(btn).toBeDisabled();
    await userEvent.type(screen.getByLabelText(/Endereço/), "https://x.supabase.co");
    await userEvent.type(screen.getByLabelText(/Chave pública/), "anon");
    await userEvent.type(screen.getByLabelText("E-mail"), "a@b.c");
    await userEvent.type(screen.getByLabelText("Senha"), "segredo");
    await userEvent.click(btn);
    await waitFor(() => expect(api.organizeLogin).toHaveBeenCalledWith("https://x.supabase.co", "anon", "a@b.c", "segredo"));
    expect(await screen.findByRole("status")).toHaveTextContent("12 eventos e 3 tarefas");
  });

  it("mostra o erro do login", async () => {
    vi.mocked(api.organizeLogin).mockRejectedValue("o servidor respondeu 400: credenciais inválidas");
    render(<OrganizeAccount />);
    await userEvent.type(await screen.findByLabelText(/Endereço/), "https://x.supabase.co");
    await userEvent.type(screen.getByLabelText(/Chave pública/), "anon");
    await userEvent.type(screen.getByLabelText("E-mail"), "a@b.c");
    await userEvent.type(screen.getByLabelText("Senha"), "x");
    await userEvent.click(screen.getByRole("button", { name: "Conectar" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("credenciais inválidas");
  });
});
