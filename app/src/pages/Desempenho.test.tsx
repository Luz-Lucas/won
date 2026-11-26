import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import * as api from "../api";
import Desempenho from "./Desempenho";

vi.mock("../api");

describe("Desempenho", () => {
  it("mostra o uso de cada parte", async () => {
    vi.mocked(api.perfSample).mockResolvedValue({
      groups: [
        { id: "desktop", label: "Barra, widgets e tiling", note: "", procs: 1, cpu: 1.2, cpu_avg: 1.1, cpu_peak: 4, ram_mb: 30, ram_avg_mb: 29, ram_peak_mb: 31 },
      ],
      total_cpu: 9,
      ram_used_mb: 8000,
      ram_total_mb: 16000,
      samples: 1,
    });
    render(<Desempenho />);
    expect(await screen.findByText("Barra, widgets e tiling")).toBeInTheDocument();
    expect(screen.getByText("30 MB")).toBeInTheDocument();
  });
});
