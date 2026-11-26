import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import SelfUpdate from "./SelfUpdate";

vi.mock("../api");

const base = {
  current: "20261126-0353",
  latest: "",
  available: false,
  notes: "",
  auto_check: true,
  auto_install: false,
  manifest_url: "https://x/latest.json",
  last_check: 0,
  error: "",
};

describe("SelfUpdate", () => {
  beforeEach(() => {
    vi.mocked(api.updateStatus).mockResolvedValue(base);
  });

  it("mostra a nova versão e instala", async () => {
    vi.mocked(api.updateCheck).mockResolvedValue({ ...base, latest: "20261126-0428", available: true, last_check: 1 });
    vi.mocked(api.updateInstall).mockResolvedValue(undefined);
    render(<SelfUpdate />);
    await userEvent.click(await screen.findByRole("button", { name: "Verificar atualizações" }));
    expect(await screen.findByText(/20261126-0428/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Instalar agora" }));
    await waitFor(() => expect(api.updateInstall).toHaveBeenCalled());
    expect(await screen.findByText(/Instalando/)).toBeInTheDocument();
  });

  it("liga a instalação automática", async () => {
    vi.mocked(api.updateConfigSet).mockResolvedValue({ ...base, auto_install: true });
    render(<SelfUpdate />);
    await userEvent.click(await screen.findByRole("checkbox", { name: /Instalar sozinho/ }));
    expect(api.updateConfigSet).toHaveBeenCalledWith(true, true, "https://x/latest.json");
  });
});
