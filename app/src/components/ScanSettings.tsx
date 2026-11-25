import { useEffect, useState } from "react";
import {
  explorerMenuGet,
  explorerMenuSet,
  scanAutoQuarantineSet,
  scanExceptionRemove,
  scanExceptionsList,
  scanScheduleGet,
  scanScheduleSet,
  type ExplorerMenuView,
  type ScanException,
  type ScheduleView,
} from "../api";
import { humanDate } from "../format";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

/** Ajustes do antivírus: verificação semanal, quarentena automática e exceções. */
export default function ScanSettings() {
  const [schedule, setSchedule] = useState<ScheduleView | null>(null);
  const [exceptions, setExceptions] = useState<ScanException[] | null>(null);
  const [explorer, setExplorer] = useState<ExplorerMenuView | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    scanScheduleGet().then(setSchedule, (e) => setError(errorText(e)));
    scanExceptionsList().then(setExceptions, (e) => setError(errorText(e)));
    explorerMenuGet().then(setExplorer, (e) => setError(errorText(e)));
  }, []);

  const run = async <T,>(f: () => Promise<T>, set: (v: T) => void) => {
    setBusy(true);
    setError(null);
    try {
      set(await f());
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <h2 className="section-title">Verificação agendada</h2>
      {schedule && (
        <>
          <p>
            <label>
              <input
                type="checkbox"
                checked={schedule.scheduled}
                disabled={busy}
                onChange={(e) => void run(() => scanScheduleSet(e.target.checked), setSchedule)}
              />{" "}
              Verificação rápida toda semana
            </label>
          </p>
          <p className="muted">
            Aos domingos ao meio-dia, pelo Agendador de Tarefas do Windows; se o computador estiver
            desligado, roda assim que ligar. O resultado aparece numa notificação.
          </p>
          <p>
            <label>
              <input
                type="checkbox"
                checked={schedule.auto_quarantine}
                disabled={busy}
                onChange={(e) =>
                  void run(() => scanAutoQuarantineSet(e.target.checked), setSchedule)
                }
              />{" "}
              Colocar malware confirmado por hash em quarentena automaticamente
            </label>
          </p>
          <p className="muted">
            Só na verificação agendada e só para arquivos na lista de malware conhecido; nunca
            dentro da pasta do Windows nem em programas abertos. O resto continua esperando você
            decidir.
          </p>
        </>
      )}

      <h2 className="section-title">Menu do Explorador</h2>
      {explorer && (
        <>
          <p>
            <label>
              <input
                type="checkbox"
                checked={explorer.enabled}
                disabled={busy || explorer.note !== null}
                onChange={(e) => void run(() => explorerMenuSet(e.target.checked), setExplorer)}
              />{" "}
              "Verificar com o WON" no clique direito
            </label>
          </p>
          <p className="muted">
            {explorer.note ??
              'Em arquivos, pastas e discos. No Windows 11, fica em "Mostrar mais opções". Desligar apaga o item do menu.'}
          </p>
        </>
      )}

      <h2 className="section-title">Exceções da verificação</h2>
      {exceptions && exceptions.length === 0 && (
        <p className="muted">Nenhum arquivo em "Ignorar sempre".</p>
      )}
      {exceptions && exceptions.length > 0 && (
        <ul className="quarantine-list" aria-label="Exceções da verificação">
          {exceptions.map((x) => (
            <li key={x.sha256} className="quarantine-row">
              <div>
                <span className="finding-path">{x.path}</span>
                <div className="muted">
                  {`${humanDate(x.added_at)} · ${x.rule} · ${x.sha256.slice(0, 16)}…`}
                </div>
              </div>
              <button
                type="button"
                className="btn"
                disabled={busy}
                aria-label={`Voltar a verificar ${x.path}`}
                onClick={() => void run(() => scanExceptionRemove(x.sha256), setExceptions)}
              >
                Remover
              </button>
            </li>
          ))}
        </ul>
      )}
      {error && (
        <p role="alert" className="form-error">
          {error}
        </p>
      )}
    </>
  );
}
