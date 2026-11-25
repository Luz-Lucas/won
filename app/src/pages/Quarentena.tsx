import { useCallback, useEffect, useState } from "react";
import {
  quarantineDelete,
  quarantineList,
  quarantineRestore,
  type QuarantineEntry,
} from "../api";
import ConfirmDialog from "../components/ConfirmDialog";
import { humanBytes, humanDate } from "../format";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

/** Arquivos em quarentena: restaurar ou apagar definitivamente. */
export default function Quarentena({ onRestored }: { onRestored?: () => void }) {
  const [list, setList] = useState<QuarantineEntry[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [confirming, setConfirming] = useState<{
    entry: QuarantineEntry;
    action: "restore" | "delete";
  } | null>(null);

  const load = useCallback(() => {
    quarantineList().then(
      (l) => {
        setList(l);
        setLoadError(null);
      },
      (e) => setLoadError(errorText(e)),
    );
  }, []);
  useEffect(load, [load]);

  // Só age depois de o usuário confirmar no diálogo.
  const act = async (entry: QuarantineEntry, action: "restore" | "delete") => {
    setConfirming(null);
    setError(null);
    setMessage(null);
    setBusy(entry.id);
    try {
      if (action === "restore") {
        const path = await quarantineRestore(entry.id);
        setMessage(`Restaurado: ${path}`);
        onRestored?.();
      } else {
        const path = await quarantineDelete(entry.id);
        setMessage(`Apagado definitivamente: ${path}`);
        onRestored?.();
      }
      load();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
    }
  };

  return (
    <section className="page">
      <h1>Quarentena</h1>
      <p className="muted">
        Os arquivos ficam guardados codificados, sem poder rodar. Restaurar devolve o arquivo ao
        lugar original (se não houver outro no lugar) conferindo o conteúdo; apagar não tem volta.
      </p>
      {loadError && (
        <p role="alert" className="form-error">
          {loadError}
        </p>
      )}
      {error && (
        <p role="alert" className="form-error">
          {error}
        </p>
      )}
      {message && <p className="notice">{message}</p>}
      {!list && !loadError && <p className="muted">Carregando…</p>}
      {list && list.length === 0 && <p className="muted">Nenhum arquivo em quarentena.</p>}
      {list && list.length > 0 && (
        <ul className="quarantine-list">
          {list.map((q) => (
            <li key={q.id} className="quarantine-row">
              <div>
                <span className="finding-path">{q.original_path}</span>
                <div className="muted">
                  {`${humanDate(q.quarantined_at)} · ${humanBytes(q.size)} · ${q.rule}`}
                </div>
              </div>
              <div className="scan-actions">
                <button
                  type="button"
                  className="btn"
                  disabled={busy !== null}
                  onClick={() => setConfirming({ entry: q, action: "restore" })}
                >
                  Restaurar
                </button>
                <button
                  type="button"
                  className="btn"
                  disabled={busy !== null}
                  onClick={() => setConfirming({ entry: q, action: "delete" })}
                >
                  Apagar
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}
      {confirming?.action === "restore" && (
        <ConfirmDialog
          title="Restaurar da quarentena"
          confirmLabel="Restaurar"
          onConfirm={() => void act(confirming.entry, "restore")}
          onCancel={() => setConfirming(null)}
        >
          <p>
            {`Restaurar ${confirming.entry.original_path}? Este arquivo foi colocado em quarentena por: ${confirming.entry.rule}.`}
          </p>
        </ConfirmDialog>
      )}
      {confirming?.action === "delete" && (
        <ConfirmDialog
          title="Apagar definitivamente?"
          confirmLabel="Apagar definitivamente"
          onConfirm={() => void act(confirming.entry, "delete")}
          onCancel={() => setConfirming(null)}
        >
          <p>
            {`${confirming.entry.original_path} será apagado de vez. Não tem volta: nem o WON nem a Lixeira conseguem recuperá-lo.`}
          </p>
        </ConfirmDialog>
      )}
    </section>
  );
}
