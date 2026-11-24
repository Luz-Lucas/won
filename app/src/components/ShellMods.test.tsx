import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import ShellMods from "./ShellMods";

vi.mock("../api");

const status = (active = false): api.ShellModsStatus => ({
  installed: true,
  anticheat_running: false,
  build: 26100,
  xaml_cfg: "enabled=true\nfill=#C81C1C24\nradius=14\nmargin=0,0,0,0\nhide_border=false\n",
  mods: [
    { id: "xaml", dll_exists: true, active },
    { id: "icones", dll_exists: true, active: false },
  ],
});

beforeEach(() => {
  vi.mocked(api.shellModsStatus).mockReset().mockResolvedValue(status());
  vi.mocked(api.shellModSet).mockReset().mockResolvedValue(1);
  vi.mocked(api.xamlCfgWrite).mockReset().mockResolvedValue(undefined);
});

describe("ShellMods", () => {
  it("pede confirmação e liga o efeito", async () => {
    render(<ShellMods />);
    await userEvent.click(await screen.findByRole("checkbox", { name: /Estilo da barra/ }));
    expect(api.shellModSet).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Ligar" }));
    await waitFor(() => expect(api.shellModSet).toHaveBeenCalledWith("xaml", true));
  });

  it("desliga sem confirmar", async () => {
    vi.mocked(api.shellModsStatus).mockResolvedValue(status(true));
    render(<ShellMods />);
    await userEvent.click(await screen.findByRole("checkbox", { name: /Estilo da barra/ }));
    await waitFor(() => expect(api.shellModSet).toHaveBeenCalledWith("xaml", false));
  });

  it("grava o estilo em formato do cfg", async () => {
    render(<ShellMods only={["xaml"]} />);
    await userEvent.click(await screen.findByRole("button", { name: "Aplicar estilo" }));
    await waitFor(() => expect(api.xamlCfgWrite).toHaveBeenCalled());
    const text = vi.mocked(api.xamlCfgWrite).mock.calls[0][0];
    expect(text).toContain("fill=#C81C1C24");
    expect(text).toContain("margin=0,0,0,0");
  });
});
