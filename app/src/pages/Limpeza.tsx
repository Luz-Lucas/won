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
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [confirming, setConfirming] = useState(false);
  const [outcome, setOutcome] = useState<CleanSummary | null>(null);
  const job = useJob<CleanSummary>();

  const loadPlan = useCallback(() => {
    setPlanError(null);
    cleanPlan([]).then(
      (p) => {
        setPlan(p);
        setChecked(new Set(p.categories.filter(selectable).map((c) => c.id)));
      },
      (e) => setPlanError(errorText(e)),
    );
  }, []);

  useEffect(loadPlan, [loadPlan]);

  // Terminado o job: guarda o resultado e refaz a simulação.
  useEffect(() => {
    if (job.state === "done" && job.result) {
      setOutcome(job.result);
      loadPlan();
    }
  }, [job.state, job.result, loadPlan]);

  const busy = job.state === "running";
  const chosen = (plan?.categories ?? []).filter(
    (c) => selectable(c) && checked.has(c.id),
  );
  const chosenFiles = chosen.reduce((n, c) => n + c.files, 0);
  const chosenBytes = chosen.reduce((n, c) => n + c.bytes, 0);
  const emptiesBin = chosen.some((c) => c.id === "recycle-bin" && c.files > 0);
  const skippedFiles = outcome
    ? outcome.categories.reduce((n, c) => n + c.skipped_files, 0)
    : 0;

  const toggle = (set: Set<string>, id: string) => {
    const next = new Set(set);
    if (!next.delete(id)) next.add(id);
    return next;
  };

  const confirm = () => {
    setConfirming(false);
    const selected = chosen.map((c) => c.id);
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
      {busy && <p className="muted">Apagando…</p>}
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
                        checked={selectable(c) && checked.has(c.id)}
                        disabled={!selectable(c) || busy}
                        onChange={() => setChecked((s) => toggle(s, c.id))}
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
            <button
              type="button"
              className="btn btn-primary"
              disabled={busy || chosen.length === 0 || chosenFiles === 0}
              onClick={() => setConfirming(true)}
            >
              Limpar
            </button>
          </div>
        </>
      )}
      {confirming && (
        <ConfirmDialog
          title="Confirmar limpeza"
          confirmLabel="Apagar"
          onConfirm={confirm}
          onCancel={() => setConfirming(false)}
        >
          <p>{`Apagar ${chosenFiles} arquivo(s), ${humanBytes(chosenBytes)} no total?`}</p>
          {emptiesBin && <p>A Lixeira será esvaziada permanentemente.</p>}
        </ConfirmDialog>
      )}
    </section>
  );
}
