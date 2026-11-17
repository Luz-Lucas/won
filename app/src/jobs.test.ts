import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const handlers = new Map<string, (e: { payload: unknown }) => void>();
const unlisten = vi.fn();
const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, cb: (e: { payload: unknown }) => void) => {
    handlers.set(name, cb);
    return unlisten;
  }),
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { useJob } from "./jobs";

function emit(name: string, payload: unknown) {
  act(() => handlers.get(name)!({ payload }));
}

async function setup() {
  const hook = renderHook(() => useJob<number>());
  await act(async () => {}); // deixa os listeners registrarem
  return hook;
}

describe("useJob", () => {
  beforeEach(() => {
    handlers.clear();
    unlisten.mockClear();
    invokeMock.mockReset();
  });

  it("começa ocioso", async () => {
    const { result } = await setup();
    expect(result.current.state).toBe("idle");
  });

  it("ignora eventos de outro job e conclui no próprio", async () => {
    const { result } = await setup();
    await act(async () => {
      await result.current.start(async () => 5);
    });
    expect(result.current.state).toBe("running");

    emit("job:done", { job: 4, kind: "scan", result: 1 });
    expect(result.current.state).toBe("running");

    emit("job:progress", {
      job: 5,
      phase: "counting",
      files_seen: 10,
    });
    expect(result.current.progress).toMatchObject({ files_seen: 10 });

    emit("job:done", { job: 5, kind: "scan", result: 42 });
    expect(result.current.state).toBe("done");
    expect(result.current.result).toBe(42);
  });

  it("vira erro com a mensagem do job", async () => {
    const { result } = await setup();
    await act(async () => {
      await result.current.start(async () => 1);
    });
    emit("job:error", { job: 1, kind: "scan", message: "falhou" });
    expect(result.current.state).toBe("error");
    expect(result.current.error).toBe("falhou");
  });

  it("vira erro se o comando recusar iniciar", async () => {
    const { result } = await setup();
    await act(async () => {
      await result.current.start(async () => {
        throw "Já existe uma operação deste tipo em andamento.";
      });
    });
    expect(result.current.state).toBe("error");
    expect(result.current.error).toBe(
      "Já existe uma operação deste tipo em andamento.",
    );
  });

  it("não perde eventos que chegam antes do id", async () => {
    const { result } = await setup();
    await act(async () => {
      await result.current.start(async () => {
        emit("job:done", { job: 9, kind: "scan", result: 3 });
        return 9;
      });
    });
    expect(result.current.state).toBe("done");
    expect(result.current.result).toBe(3);
  });

  it("ignora start enquanto há um job vivo", async () => {
    const { result } = await setup();
    await act(async () => {
      await result.current.start(async () => 1);
    });
    const second = vi.fn(async () => 2);
    await act(async () => {
      await result.current.start(second);
    });
    expect(second).not.toHaveBeenCalled();
    expect(result.current.state).toBe("running");
    emit("job:done", { job: 1, kind: "scan", result: 11 });
    expect(result.current.state).toBe("done");
    expect(result.current.result).toBe(11);
    // terminado, um novo start volta a funcionar
    await act(async () => {
      await result.current.start(second);
    });
    expect(second).toHaveBeenCalledTimes(1);
  });

  it("cancel pedido antes do id é enviado quando o id chega", async () => {
    const { result } = await setup();
    let resolveId!: (id: number) => void;
    let started!: Promise<void>;
    await act(async () => {
      started = result.current.start(
        () => new Promise<number>((r) => (resolveId = r)),
      );
    });
    await act(async () => {
      await result.current.cancel();
    });
    expect(invokeMock).not.toHaveBeenCalled();
    await act(async () => {
      resolveId(3);
      await started;
    });
    expect(invokeMock).toHaveBeenCalledWith("job_cancel", { id: 3 });
  });

  it("cancel chama job_cancel com o id", async () => {
    const { result } = await setup();
    await act(async () => {
      await result.current.start(async () => 7);
    });
    await act(async () => {
      await result.current.cancel();
    });
    expect(invokeMock).toHaveBeenCalledWith("job_cancel", { id: 7 });
  });

  it("remove os listeners ao desmontar", async () => {
    const { unmount } = await setup();
    unmount();
    await act(async () => {});
    expect(unlisten).toHaveBeenCalledTimes(3);
  });
});
