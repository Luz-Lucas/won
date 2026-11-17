import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import App from "./App";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("App", () => {
  it("mostra as cinco páginas e troca de página", async () => {
    render(<App />);
    for (const n of ["Início", "Limpeza", "Segurança", "Disco", "Ajustes"])
      expect(screen.getByRole("button", { name: n })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Limpeza" }));
    expect(
      screen.getByRole("heading", { level: 1, name: "Limpeza" }),
    ).toBeInTheDocument();
  });
});
