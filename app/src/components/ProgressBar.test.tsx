import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import ProgressBar from "./ProgressBar";

const scanning = (over: Record<string, number | string> = {}) =>
  ({
    job: 1,
    phase: "scanning",
    files_done: 5,
    total_files: 10,
    bytes_done: 0,
    total_bytes: 0,
    current: "C:\\a",
    ...over,
  }) as never;

describe("ProgressBar", () => {
  it("usa a razão de arquivos quando total_bytes é 0, sem NaN", () => {
    render(<ProgressBar progress={scanning()} />);
    expect(screen.getByRole("progressbar")).toHaveAttribute(
      "aria-valuenow",
      "50",
    );
    expect(document.body.textContent).not.toMatch(/NaN|Infinity/);
  });

  it("com tudo zerado mostra 0%", () => {
    render(<ProgressBar progress={scanning({ total_files: 0 })} />);
    expect(screen.getByRole("progressbar")).toHaveAttribute(
      "aria-valuenow",
      "0",
    );
    expect(document.body.textContent).not.toMatch(/NaN|Infinity/);
  });

  it("calcula por bytes quando há total", () => {
    render(
      <ProgressBar progress={scanning({ bytes_done: 25, total_bytes: 100 })} />,
    );
    expect(screen.getByRole("progressbar")).toHaveAttribute(
      "aria-valuenow",
      "25",
    );
  });
});
