import { useEffect, useState } from "react";
import { updatesMode, updatesUpdateNow } from "../api";
import ConfirmDialog from "./ConfirmDialog";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

/** Na página Aplicativos: com as atualizações bloqueadas, avisa que instalações
 *  da Store e alguns instaladores falham, e oferece liberar temporariamente. */
export default function UpdatesBlockedNotice() {
  const [blocked, setBlocked] = useState(false);
  const [ask, setAsk] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    updatesMode().then(
      (m) => setBlocked(m === "blocked"),
      () => setBlocked(false),
    );
  }, []);

  if (!blocked) return null;

  const release = async () => {
    setAsk(false);
    setBusy(true);
    setError(null);
    try {
      await updatesUpdateNow();
      setBlocked(false);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="notice" role="status">
      <p>
        As atualizações do Windows estão bloqueadas: instalações da Microsoft Store e alguns
        instaladores (pacotes da Store no winget, .NET, drivers) podem falhar.
      </p>
      <button type="button" className="btn" disabled={busy} onClick={() => setAsk(true)}>
        Liberar temporariamente
      </button>
      {error && (
        <p role="alert" className="form-error">
          {error}
        </p>
      )}
      {ask && (
        <ConfirmDialog
          title="Liberar as atualizações agora?"
          confirmLabel="Liberar"
          onConfirm={() => void release()}
          onCancel={() => setAsk(false)}
        >
          <p>
            O Sentinela volta as atualizações ao padrão do Windows e abre o Windows Update. Depois,
            volte ao modo Bloqueado em Ajustes.
          </p>
        </ConfirmDialog>
      )}
    </div>
  );
}
