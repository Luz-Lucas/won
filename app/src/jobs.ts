import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";

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

export function useJob<T>() {
  const [state, setState] = useState<JobState>("idle");
  const [progress, setProgress] = useState<JobProgress | null>(null);
  const [result, setResult] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);

  const idRef = useRef<number | null>(null);
  // Eventos recebidos enquanto o comando ainda não devolveu o id do job.
  const pendingRef = useRef<JobEvent[] | null>(null);

  const apply = useCallback((ev: JobEvent) => {
    if (ev.type === "progress") setProgress(ev.payload);
    else if (ev.type === "done") {
      setResult(ev.payload.result as T);
      setState("done");
    } else {
      setError(ev.payload.message);
      setState("error");
    }
  }, []);

  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];
    const handle = (ev: JobEvent) => {
      if (pendingRef.current) pendingRef.current.push(ev);
      else if (ev.payload.job === idRef.current) apply(ev);
    };
    const register = <P>(name: string, type: JobEvent["type"]) =>
      listen<P>(name, (e) =>
        handle({ type, payload: e.payload } as unknown as JobEvent),
      ).then((un) => {
        if (disposed) un();
        else unlisteners.push(un);
      });
    void register("job:progress", "progress");
    void register("job:done", "done");
    void register("job:error", "error");
    return () => {
      disposed = true;
      unlisteners.forEach((un) => un());
    };
  }, [apply]);

  const start = useCallback(
    async (invokeFn: () => Promise<number>) => {
      setState("running");
      setProgress(null);
      setResult(null);
      setError(null);
      idRef.current = null;
      pendingRef.current = [];
      try {
        const id = await invokeFn();
        idRef.current = id;
        const buffered = pendingRef.current ?? [];
        pendingRef.current = null;
        buffered.filter((ev) => ev.payload.job === id).forEach(apply);
      } catch (e) {
        pendingRef.current = null;
        setError(typeof e === "string" ? e : String(e));
        setState("error");
      }
    },
    [apply],
  );

  const cancel = useCallback(async () => {
    if (idRef.current !== null) await invoke("job_cancel", { id: idRef.current });
  }, []);

  return { state, progress, result, error, start, cancel };
}
