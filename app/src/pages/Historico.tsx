import { useCallback, useEffect, useState } from "react";
import { tweaksJournal, tweaksUndo, tweaksUndoAll } from "../api";
import type { JournalEntry, UndoReport } from "../api";
import ConfirmDialog from "../components/ConfirmDialog";
import { humanDate } from "../format";
import { useJob } from "../jobs";

type Pending = { kind: "one"; entry: JournalEntry } | { kind: "all" };

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

export default function Historico() {
  const [entries, setEntries] = useState<JournalEntry[] | null>(null);
  const [titles, setTitles] = useState<Record<string, string>>({});
  const [loadError, setLoadError] = useState<string | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);
  const job = useJob<UndoReport>("tweaks");
  // Job compartilhado com Jogos: só mostra relatório de um desfazer iniciado aqui.
  const [started, setStarted] = useState(false);

  const load = useCallback(() => {
    setLoadError(null);
    tweaksJournal().then(
      (list) => {
        setEntries(list);
        setTitles((old) => ({
          ...old,
          ...Object.fromEntries(list.map((e) => [e.tweak_id, e.title])),
        }));
      },
      (e) => setLoadError(errorText(e)),
    );
  }, []);

  useEffect(load, [load]);

  // Terminado o job: recarrega o diário.
  useEffect(() => {
    if (job.state === "done" || job.state === "error") load();
  }, [job.state, load]);

  const running = job.state === "running";
  const list = entries ?? [];
  const titleOf = (id: string) => titles[id] ?? id;

  const confirm = () => {
    const p = pending;
    setPending(null);
    if (!p) return;
    setStarted(true);
    if (p.kind === "one") void job.start(() => tweaksUndo([p.entry.tweak_id]));
    else void job.start(() => tweaksUndoAll());
  };

  const report =
    started && job.state === "done" && job.result && "conflicts" in job.result
      ? job.result
      : null;

  return (
    <section className="page">
      <h1>Histórico</h1>
      {loadError && (
        <div>
          <p role="alert" className="form-error">
            Não foi possível carregar o histórico: {loadError}
          </p>
          <button type="button" className="btn" onClick={load}>
            Tentar de novo
          </button>
        </div>
      )}
      {job.error && (
        <p role="alert" className="form-error">
          Não foi possível desfazer: {job.error}
        </p>
      )}
      {!entries && !loadError && <p className="muted">Carregando…</p>}
      {running && <p className="muted">Desfazendo…</p>}
      {report && (
        <div className="notice">
          <ul className="tweak-results">
            {report.results.map((r) => (
              <li key={r.tweak_id} className={r.ok ? undefined : "form-error"}>
                {r.ok
                  ? `${titleOf(r.tweak_id)}: desfeito`
                  : `${titleOf(r.tweak_id)}: ${r.message}`}
              </li>
            ))}
          </ul>
          {report.conflicts.length > 0 && (
            <div>
              <p>Mantido porque mudou depois da otimização:</p>
              <ul>
                {report.conflicts.map((c, n) => (
                  <li key={`${c}-${n}`}>{c}</li>
                ))}
              </ul>
            </div>
          )}
        </div>
      )}
      {entries && list.length === 0 && (
        <p className="muted">Nenhuma otimização aplicada pelo Sentinela.</p>
      )}
      {list.length > 0 && (
        <>
          <div className="clean-footer">
            <span>{`${list.length} otimização(ões) aplicada(s)`}</span>
            <button
              type="button"
              className="btn btn-danger"
              disabled={running}
              onClick={() => setPending({ kind: "all" })}
            >
              Desfazer tudo
            </button>
          </div>
          <ul className="clean-list">
            {list.map((e) => (
              <li key={e.tweak_id} className="clean-row">
                <div className="clean-head">
                  <strong>{e.title}</strong>
                  <span className="muted">{humanDate(e.applied_at)}</span>
                  <span className="muted">{`${e.actions.filter((a) => a.done).length} mudança(s)`}</span>
                  <button
                    type="button"
                    className="btn"
                    disabled={running}
                    onClick={() => setPending({ kind: "one", entry: e })}
                  >
                    Desfazer
                  </button>
                </div>
              </li>
            ))}
          </ul>
        </>
      )}
      {pending && (
        <ConfirmDialog
          title={
            pending.kind === "one"
              ? `Desfazer "${pending.entry.title}"?`
              : `Desfazer todas as ${list.length} otimizações?`
          }
          confirmLabel={pending.kind === "one" ? "Desfazer" : "Desfazer tudo"}
          onConfirm={confirm}
          onCancel={() => setPending(null)}
        >
          <p>Os valores voltam ao que eram antes da otimização, exceto os que mudaram depois.</p>
        </ConfirmDialog>
      )}
    </section>
  );
}
