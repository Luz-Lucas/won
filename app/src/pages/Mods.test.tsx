import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../api";
import Mods from "./Mods";

vi.mock("../api");
vi.mock("@tauri-apps/plugin-opener", () => ({ revealItemInDir: vi.fn() }));

const status = (over: Partial<api.ModsStatus> = {}): api.ModsStatus => ({
  installed: true,
  mods: [
    {
      name: "meu-mod",
      dll: "C:\\m.dll",
      targets: ["notepad.exe"],
      enabled: true,
      dll_exists: true,
      loaded: [],
    },
  ],
  anticheat_running: false,
  registry_path: "C:\\mods.toml",
  recovery_path: null,
  registry_error: null,
  ...over,
});

beforeEach(() => {
  vi.mocked(api.modsStatus).mockReset().mockResolvedValue(status());
  vi.mocked(api.modsInject).mockReset().mockResolvedValue(1);
  vi.mocked(api.modsEject).mockReset().mockResolvedValue(1);
  vi.mocked(api.modsEjectAll).mockReset().mockResolvedValue(1);
  vi.mocked(api.modsRecoveryWrite).mockReset().mockResolvedValue("C:\\r\\desfazer-mods.cmd");
});

describe("Mods", () => {
  it("pede confirmação antes de carregar", async () => {
    render(<Mods />);
    await userEvent.click(await screen.findByRole("button", { name: "Carregar meu-mod" }));
    expect(api.modsInject).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "Carregar" }));
    await waitFor(() => expect(api.modsInject).toHaveBeenCalledWith("meu-mod"));
  });

  it("com anticheat aberto, bloqueia o carregamento e avisa", async () => {
    vi.mocked(api.modsStatus).mockResolvedValue(status({ anticheat_running: true }));
    render(<Mods />);
    expect(await screen.findByText(/anticheat aberto/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Carregar meu-mod" })).toBeDisabled();
  });

  it("descarrega um mod carregado", async () => {
    vi.mocked(api.modsStatus).mockResolvedValue(
      status({
        mods: [
          {
            name: "meu-mod",
            dll: "C:\\m.dll",
            targets: ["notepad.exe"],
            enabled: true,
            dll_exists: true,
            loaded: [{ mod_name: "meu-mod", pid: 7, process: "notepad.exe", dll: "C:\\m.dll" }],
          },
        ],
      }),
    );
    render(<Mods />);
    await userEvent.click(await screen.findByRole("button", { name: "Descarregar meu-mod" }));
    await waitFor(() => expect(api.modsEject).toHaveBeenCalledWith("meu-mod"));
  });

  it("grava o script de recuperação", async () => {
    render(<Mods />);
    await userEvent.click(
      await screen.findByRole("button", { name: "Gravar e mostrar o script de recuperação" }),
    );
    await waitFor(() => expect(api.modsRecoveryWrite).toHaveBeenCalled());
  });

  it("sem o motor instalado, avisa", async () => {
    vi.mocked(api.modsStatus).mockResolvedValue(status({ installed: false }));
    render(<Mods />);
    expect(await screen.findByText(/não foi encontrado ao lado/)).toBeInTheDocument();
  });
});
