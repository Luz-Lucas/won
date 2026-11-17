import { open } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useState } from "react";
import {
  quarantineList,
  quarantineRestore,
  scanStart,
  type QuarantineEntry,
  type ScanSummary,
  type ScanTarget,
} from "../api";
import FindingList from "../components/FindingList";
import ProgressBar from "../components/ProgressBar";
import type { Page } from "../components/Sidebar";
import { humanBytes, humanDate } from "../format";
import { useJob } from "../jobs";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));
const number = new Intl.NumberFormat("pt-BR");

export default function Seguranca({
  onNavigate,
}: {
  onNavigate?: (page: Page) => void;
}) {
  const job = useJob<ScanSummary>();
  const [picking, setPicking] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [showErrors, setShowErrors] = useState(false);
  const [quarantine, setQuarantine] = useState<QuarantineEntry[] | null>(null);
  const [quarantineError, setQuarantineError] = useState<string | null>(null);
  const [restoreError, setRestoreError] = useState<string | null>(null);
  const [restoring, setRestoring] = useState<string | null>(null);

  const running = job.state === "running";
  const busy = running || picking;

  useEffect(() => {
    if (!running) setCancelling(false);
  }, [running]);

  const loadQuarantine = useCallback(() => {
    quarantineList().then(
      (list) => {
        setQuarantine(list);
        setQuarantineError(null);
      },
      (e) => setQuarantineError(errorText(e)),
    );
  }, []);
  useEffect(loadQuarantine, [loadQuarantine]);

  const scan = (target: ScanTarget) => {
    setShowErrors(false);
    void job.start(() => scanStart(target));
  };

  const pickFolder = async () => {
    setPicking(true);
    try {
      const dir = await open({ directory: true });
      if (typeof dir === "string" && dir)
        scan({ kind: "custom", paths: [dir] });
    } catch {
      /* seletor fechado ou indisponível: nada a verificar */
    } finally {
      setPicking(false);
    }
  };

  const cancel = () => {
    setCancelling(true);
    job.cancel().catch(() => setCancelling(false));
  };

  const restore = async (id: string) => {
    setRestoreError(null);
    setRestoring(id);
    try {
      await quarantineRestore(id);
      loadQuarantine();
    } catch (e) {
      setRestoreError(errorText(e));
    } finally {
      setRestoring(null);
    }
  };

  const result = job.state === "done" ? job.result : null;
  const nothingFound =
    result !== null && result.malicious === 0 && result.suspicious === 0;

  return (
    <section className="page">
      <h1>Segurança</h1>
      <div className="scan-actions">
        <button
          type="button"
          className="btn btn-primary"
          disabled={busy}
          onClick={() => scan({ kind: "quick" })}
        >
          Rápida
        </button>
        <button
          type="button"
          className="btn"
          disabled={busy}
          onClick={() => scan({ kind: "full" })}
        >
          Completa
        </button>
        <button
          type="button"
          className="btn"
          disabled={busy}
          onClick={() => void pickFolder()}
        >
          Pasta…
        </button>
      </div>
      <p className="muted">
        A verificação completa lê todos os discos fixos e pode demorar bastante.
        Sem administrador, algumas pastas do sistema ficam de fora.
      </p>

      {job.error && (
        <p role="alert" className="form-error">
          {job.error}
        </p>
      )}

      {running && (
        <div className="scan-running">
          <ProgressBar progress={job.progress} />
          <button
            type="button"
            className="btn"
            disabled={cancelling}
            onClick={cancel}
          >
            Cancelar
          </button>
        </div>
      )}

      {result && (
        <div className="scan-result">
          {!result.db_initialized && (
            <div className="notice">
              <p>
                Banco de assinaturas não inicializado: só a assinatura de teste
                EICAR e as heurísticas estão ativas.
              </p>
              <button
                type="button"
                className="btn"
                onClick={() => onNavigate?.("ajustes")}
              >
                Ir para Ajustes
              </button>
            </div>
          )}
          {result.interrupted && (
            <p className="notice">Verificação interrompida: resultado parcial.</p>
          )}
          <p>
            {`${number.format(result.scanned)} arquivos verificados · ${result.malicious} maliciosos · ${result.suspicious} suspeitos`}
          </p>
          {nothingFound && (
            <p className="notice">
              Nenhuma ameaça conhecida encontrada nos arquivos verificados.
            </p>
          )}
          {result.missing.map((p) => (
            <p key={p} className="notice">
              {`Não foi possível verificar: ${p}`}
            </p>
          ))}
          {result.warnings.map((w) => (
            <p key={w} className="muted">
              {w}
            </p>
          ))}
          <FindingList items={result.items} />
          {result.items_total > result.items.length && (
            <p className="muted">
              {`Mostrando ${number.format(result.items.length)} de ${number.format(result.items_total)}`}
            </p>
          )}
          {result.errors_total > 0 && (
            <div className="scan-errors">
              <button
                type="button"
                className="link-btn"
                aria-expanded={showErrors}
                onClick={() => setShowErrors((v) => !v)}
              >
                {`Não verificados (${number.format(result.errors_total)})`}
              </button>
              {showErrors && (
                <ul className="sample">
                  {result.errors.map((e, i) => (
                    <li key={`${e.path}-${i}`}>
                      <span>{e.path}</span>{" "}
                      <span className="muted">{e.reason}</span>
                    </li>
                  ))}
                  {result.errors_total > result.errors.length && (
                    <li className="muted">
                      {`… e mais ${number.format(result.errors_total - result.errors.length)}`}
                    </li>
                  )}
                </ul>
              )}
            </div>
          )}
        </div>
      )}

      <h2>Quarentena</h2>
      {quarantineError && (
        <p role="alert" className="form-error">
          {quarantineError}
        </p>
      )}
      {restoreError && (
        <p role="alert" className="form-error">
          {restoreError}
        </p>
      )}
      {quarantine && quarantine.length === 0 && (
        <p className="muted">Nenhum arquivo em quarentena.</p>
      )}
      {quarantine && quarantine.length > 0 && (
        <ul className="quarantine-list">
          {quarantine.map((q) => (
            <li key={q.id} className="quarantine-row">
              <div>
                <span className="finding-path">{q.original_path}</span>
                <div className="muted">
                  {`${humanDate(q.quarantined_at)} · ${humanBytes(q.size)} · ${q.rule}`}
                </div>
              </div>
              <button
                type="button"
                className="btn"
                disabled={restoring !== null}
                onClick={() => void restore(q.id)}
              >
                Restaurar
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
