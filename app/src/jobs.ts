import { jobCancel, jobRunning, type JobKind } from "./api";
import { listen } from "@tauri-apps/api/event";
import {
  createContext,
  createElement,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from "react";

export type { JobKind } from "./api";

export type JobProgress =
  | { job: number; phase: "counting"; files_seen: number }
  | {
      job: number;
      phase: "scanning";
      files_done: number;
      total_files: number;
      bytes_done: number;
      total_bytes: number;
      current: string;
    };

type JobEvent =
  | { type: "progress"; payload: JobProgress }
  | { type: "done"; payload: { job: number; kind: string; result: unknown } }
  | { type: "error"; payload: { job: number; kind: string; message: string } };

export type JobState = "idle" | "running" | "done" | "error";

export interface JobView<T> {
  state: JobState;
  progress: JobProgress | null;
  result: T | null;
  error: string | null;
}

const KINDS: JobKind[] = ["scan", "clean", "find_large", "find_dupes", "update"];

const IDLE: JobView<unknown> = {
  state: "idle",
  progress: null,
  result: null,
  error: null,
};

interface Entry {
  view: JobView<unknown>;
  id: number | null;
  // Eventos recebidos enquanto o comando ainda não devolveu o id do job.
  pending: JobEvent[] | null;
  // True enquanto há um job vivo (inclusive aguardando o id).
  live: boolean;
  cancelRequested: boolean;
}

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

/**
 * Estado das operações longas da janela inteira, uma por tipo. Vive acima das
 * páginas: trocar de página não perde o job, o progresso nem o resultado.
 */
export class JobStore {
  private entries = new Map<JobKind, Entry>(
    KINDS.map((k) => [
      k,
      { view: IDLE, id: null, pending: null, live: false, cancelRequested: false },
    ]),
  );
  private listeners = new Set<() => void>();
  private settledListeners = new Set<() => void>();

  get(kind: JobKind): JobView<unknown> {
    return this.entry(kind).view;
  }

  subscribe = (fn: () => void) => {
    this.listeners.add(fn);
    return () => {
      this.listeners.delete(fn);
    };
  };

  /** Avisa quando qualquer operação termina (concluída ou com erro). */
  onSettled(fn: () => void) {
    this.settledListeners.add(fn);
    return () => {
      this.settledListeners.delete(fn);
    };
  }

  /**
   * Escuta os eventos dos jobs (uma vez para o app todo) e, com os ouvintes já
   * registrados, retoma jobs que seguem rodando no backend (janela recarregada).
   */
  attach(): () => void {
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    const register = (name: string, type: JobEvent["type"]) =>
      listen<unknown>(name, (e) =>
        this.handle({ type, payload: e.payload } as JobEvent),
      ).then((un) => {
        if (disposed) un();
        else unlisteners.push(un);
      });
    void Promise.all([
      register("job:progress", "progress"),
      register("job:done", "done"),
      register("job:error", "error"),
    ]).then(() => {
      if (!disposed) KINDS.forEach((k) => void this.reattach(k));
    });
    return () => {
      disposed = true;
      unlisteners.forEach((un) => un());
    };
  }

  async start(kind: JobKind, invokeFn: () => Promise<number>) {
    const e = this.entry(kind);
    if (e.live) return; // já há um job vivo: ignora cliques repetidos
    e.live = true;
    e.cancelRequested = false;
    e.id = null;
    e.pending = [];
    this.set(kind, { state: "running", progress: null, result: null, error: null });
    let id: number;
    try {
      id = await invokeFn();
    } catch (err) {
      e.pending = null;
      e.live = false;
      this.set(kind, { ...e.view, state: "error", error: errorText(err) });
      return;
    }
    this.adopt(kind, id);
    if (e.cancelRequested && e.live) {
      // Falha ao cancelar não encerra o job: o evento terminal limpa o estado.
      try {
        await jobCancel(id);
      } catch {
        /* o job segue em andamento */
      }
    }
  }

  async cancel(kind: JobKind) {
    const e = this.entry(kind);
    if (e.id !== null) await jobCancel(e.id);
    else if (e.live) e.cancelRequested = true;
  }

  private entry(kind: JobKind): Entry {
    return this.entries.get(kind)!;
  }

  private set(kind: JobKind, view: JobView<unknown>) {
    this.entry(kind).view = view;
    this.listeners.forEach((fn) => fn());
  }

  /** O id chegou: aplica os eventos guardados que são deste job. */
  private adopt(kind: JobKind, id: number) {
    const e = this.entry(kind);
    e.id = id;
    const buffered = e.pending ?? [];
    e.pending = null;
    buffered
      .filter((ev) => ev.payload.job === id)
      .forEach((ev) => this.apply(kind, ev));
  }

  private async reattach(kind: JobKind) {
    const e = this.entry(kind);
    if (e.live) return;
    e.live = true;
    e.pending = [];
    let id: number | null = null;
    try {
      id = (await jobRunning(kind)) ?? null;
    } catch {
      id = null;
    }
    if (typeof id !== "number") {
      e.pending = null;
      e.live = false;
      return;
    }
    this.set(kind, { state: "running", progress: null, result: null, error: null });
    this.adopt(kind, id);
  }

  private handle(ev: JobEvent) {
    for (const kind of KINDS) {
      const e = this.entry(kind);
      if (e.pending) e.pending.push(ev);
      else if (e.id !== null && ev.payload.job === e.id) this.apply(kind, ev);
    }
    if (ev.type !== "progress") this.settledListeners.forEach((fn) => fn());
  }

  private apply(kind: JobKind, ev: JobEvent) {
    const e = this.entry(kind);
    if (ev.type === "progress") {
      this.set(kind, { ...e.view, progress: ev.payload });
    } else if (ev.type === "done") {
      e.live = false;
      e.id = null;
      this.set(kind, { ...e.view, state: "done", result: ev.payload.result });
    } else {
      e.live = false;
      e.id = null;
      this.set(kind, { ...e.view, state: "error", error: ev.payload.message });
    }
  }
}

const JobsContext = createContext<JobStore | null>(null);

/** Guarda as operações do app; as páginas usam `useJob(tipo)`. */
export function JobsProvider({ children }: { children: ReactNode }) {
  const [store] = useState(() => new JobStore());
  useEffect(() => store.attach(), [store]);
  return createElement(JobsContext.Provider, { value: store }, children);
}

function useStore(): JobStore {
  const store = useContext(JobsContext);
  if (!store) throw new Error("useJob precisa de <JobsProvider>");
  return store;
}

export function useJob<T>(kind: JobKind) {
  const store = useStore();
  const view = useSyncExternalStore(store.subscribe, () =>
    store.get(kind),
  ) as JobView<T>;
  const start = useCallback(
    (invokeFn: () => Promise<number>) => store.start(kind, invokeFn),
    [store, kind],
  );
  const cancel = useCallback(() => store.cancel(kind), [store, kind]);
  return { ...view, start, cancel };
}

/** Chama `fn` sempre que qualquer operação termina. */
export function useJobSettled(fn: () => void) {
  const store = useStore();
  const ref = useRef(fn);
  ref.current = fn;
  useEffect(() => store.onSettled(() => ref.current()), [store]);
}
