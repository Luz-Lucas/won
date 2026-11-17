import { useCallback, useEffect, useState } from "react";
import { cleanApply, cleanPlan } from "../api";
import type { CategorySummary, CleanSummary } from "../api";
import ConfirmDialog from "../components/ConfirmDialog";
import { humanBytes } from "../format";
import { useJob } from "../jobs";

const NAMES: Record<string, string> = {
  "user-temp": "Temporários do usuário",
  "windows-temp": "Temporários do Windows",
  "crash-dumps": "Despejos de erro",
  "browser-cache": "Cache de navegadores",
  "recycle-bin": "Lixeira",
  "windows-update": "Cache do Windows Update",
};

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

const selectable = (c: CategorySummary) => c.skipped_reason === null;

export default function Limpeza() {
  const [plan, setPlan] = useState<CleanSummary | null>(null);
  const [planError, setPlanError] = useState<string | null>(null);
  // Só guarda o que o usuário desmarcou: categorias novas entram marcadas.
  const [unchecked, setUnchecked] = useState<Set<string>>(new Set());
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [planning, setPlanning] = useState(false);
  const [nothing, setNothing] = useState(false);
  // Seleção revalidada por uma simulação nova, mostrada no diálogo.
  const [pending, setPending] = useState<{
    ids: string[];
    files: number;
    bytes: number;
    bin: boolean;
  } | null>(null);
  const [outcome, setOutcome] = useState<CleanSummary | null>(null);
  const job = useJob<CleanSummary>();

  const loadPlan = useCallback(() => {
    setPlanError(null);
    setPlanning(true);
    cleanPlan([]).then(
      (p) => setPlan(p),
      (e) => setPlanError(errorText(e)),
    ).finally(() => setPlanning(false));
  }, []);

  useEffect(loadPlan, [loadPlan]);

  // Terminado o job: guarda o resultado e refaz a simulação.
  useEffect(() => {
    if (job.state === "done" && job.result) {
      setOutcome(job.result);
      loadPlan();
    }
  }, [job.state, job.result, loadPlan]);

  const busy = job.state === "running" || planning;
  const isChosen = (c: CategorySummary) =>
    selectable(c) && !unchecked.has(c.id);
  const chosen = (plan?.categories ?? []).filter(isChosen);
  const chosenFiles = chosen.reduce((n, c) => n + c.files, 0);
  const chosenBytes = chosen.reduce((n, c) => n + c.bytes, 0);
  const skippedFiles = outcome
    ? outcome.categories.reduce((n, c) => n + c.skipped_files, 0)
    : 0;

  const toggle = (set: Set<string>, id: string) => {
    const next = new Set(set);
    if (!next.delete(id)) next.add(id);
    return next;
  };

  // Antes de confirmar, simula de novo: o diálogo mostra números frescos.
  const requestClean = async () => {
    const ids = chosen.map((c) => c.id);
    if (ids.length === 0) return;
    setNothing(false);
    setPlanError(null);
    setPlanning(true);
    try {
      const fresh = await cleanPlan(ids);
      setPlan((old) =>
        old
          ? {
              ...old,
              categories: old.categories.map(
                (c) => fresh.categories.find((f) => f.id === c.id) ?? c,
              ),
            }
          : fresh,
      );
      const ok = fresh.categories.filter(
        (c) => ids.includes(c.id) && selectable(c) && c.files > 0,
      );
      if (ok.length === 0) {
        setNothing(true);
        return;
      }
      setPending({
        ids: ok.map((c) => c.id),
        files: ok.reduce((n, c) => n + c.files, 0),
        bytes: ok.reduce((n, c) => n + c.bytes, 0),
        bin: ok.some((c) => c.id === "recycle-bin"),
      });
    } catch (e) {
      setPlanError(errorText(e));
    } finally {
      setPlanning(false);
    }
  };

  const confirm = () => {
    const selected = pending?.ids ?? [];
    setPending(null);
    if (selected.length === 0) return; // nunca envia seleção vazia
    setOutcome(null);
    void job.start(() => cleanApply(selected));
  };

  return (
    <section className="page">
      <h1>Limpeza</h1>
      {planError && (
        <p role="alert" className="form-error">
          Não foi possível simular a limpeza: {planError}
        </p>
      )}
      {job.error && (
        <p role="alert" className="form-error">
          Não foi possível limpar: {job.error}
        </p>
      )}
      {!plan && !planError && <p className="muted">Calculando…</p>}
      {job.state === "running" && <p className="muted">Apagando…</p>}
      {planning && plan && <p className="muted">Simulando…</p>}
      {nothing && <p className="notice">Nada para apagar.</p>}
      {outcome && (
        <p className="notice">
          {`Apagados: ${outcome.deleted} arquivo(s), ${humanBytes(outcome.freed)} liberados`}
          {skippedFiles > 0 &&
            ` · ${skippedFiles} arquivo(s) pulados (em uso ou sem permissão)`}
        </p>
      )}
      {plan && (
        <>
          <ul className="clean-list">
            {plan.categories.map((c) => {
              const name = NAMES[c.id] ?? c.id;
              const open = expanded.has(c.id);
              return (
                <li key={c.id} className="clean-row">
                  <div className="clean-head">
                    <label className="clean-check">
                      <input
                        type="checkbox"
                        checked={isChosen(c)}
                        disabled={!selectable(c) || job.state === "running"}
                        onChange={() => setUnchecked((s) => toggle(s, c.id))}
                      />
                      <span>{name}</span>
                    </label>
                    {selectable(c) ? (
                      <span className="muted">
                        {`${c.files} arquivo(s) · ${humanBytes(c.bytes)}`}
                      </span>
                    ) : (
                      <span className="muted">{c.skipped_reason}</span>
                    )}
                    {selectable(c) && c.sample.length > 0 && (
                      <button
                        type="button"
                        className="link-btn"
                        aria-expanded={open}
                        onClick={() => setExpanded((s) => toggle(s, c.id))}
                      >
                        {open ? "ocultar amostra" : "ver amostra"}
                      </button>
                    )}
                  </div>
                  {c.warnings.map((w) => (
                    <p key={w} className="muted clean-warning">
                      {w}
                    </p>
                  ))}
                  {open && (
                    <ul className="sample">
                      {c.sample.map((s) => (
                        <li key={s.path}>
                          <span>{s.path}</span>{" "}
                          <span className="muted">{humanBytes(s.size)}</span>
                        </li>
                      ))}
                      {c.files > c.sample.length && (
                        <li className="muted">
                          {`… e mais ${c.files - c.sample.length} arquivo(s)`}
                        </li>
                      )}
                    </ul>
                  )}
                </li>
              );
            })}
          </ul>
          <div className="clean-footer">
            <span>
              {`${chosenFiles} arquivo(s) · ${humanBytes(chosenBytes)} selecionados`}
            </span>
            <div className="clean-actions">
              <button
                type="button"
                className="btn"
                disabled={busy}
                onClick={loadPlan}
              >
                Simular novamente
              </button>
              <button
                type="button"
                className="btn btn-primary"
                disabled={busy || chosen.length === 0 || chosenFiles === 0}
                onClick={() => void requestClean()}
              >
                Limpar
              </button>
            </div>
          </div>
        </>
      )}
      {pending && (
        <ConfirmDialog
          title="Confirmar limpeza"
          confirmLabel="Apagar"
          onConfirm={confirm}
          onCancel={() => setPending(null)}
        >
          <p>{`Apagar ${pending.files} arquivo(s), ${humanBytes(pending.bytes)} no total?`}</p>
          {pending.bin && <p>A Lixeira será esvaziada permanentemente.</p>}
        </ConfirmDialog>
      )}
    </section>
  );
}
