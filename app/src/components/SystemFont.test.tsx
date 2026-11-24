import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import SystemFont from "./SystemFont";

vi.mock("../api", () => ({ systemFontStatus: vi.fn(), systemFontSet: vi.fn() }));

const st = { bundled: true, active: false, elevated: true, backup_path: "C:\\x.reg" };

describe("SystemFont", () => {
  beforeEach(() => {
    vi.mocked(api.systemFontStatus).mockResolvedValue(st);
    vi.mocked(api.systemFontSet).mockReset().mockResolvedValue(undefined);
  });

  it("confirma antes de trocar e avisa sobre fonte monoespaçada", async () => {
    render(<SystemFont />);
    expect(await screen.findByText(/monoespaçada/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Usar JetBrains Mono" }));
    await userEvent.click(await screen.findByRole("button", { name: "Trocar" }));
    await waitFor(() => expect(api.systemFontSet).toHaveBeenCalledWith(true));
  });

  it("reverte quando a troca está ativa", async () => {
    vi.mocked(api.systemFontStatus).mockResolvedValue({ ...st, active: true });
    render(<SystemFont />);
    await userEvent.click(await screen.findByRole("button", { name: "Voltar à fonte padrão" }));
    await waitFor(() => expect(api.systemFontSet).toHaveBeenCalledWith(false));
  });
});
