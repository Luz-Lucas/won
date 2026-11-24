import { useCallback, useEffect, useState } from "react";
import { systemFontSet, systemFontStatus } from "../api";
import type { SystemFontStatus } from "../api";
import ConfirmDialog from "./ConfirmDialog";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

/** Troca a fonte padrão do Windows (Segoe UI) pela JetBrains Mono, com reversão. */
export default function SystemFont() {
  const [status, setStatus] = useState<SystemFontStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [ask, setAsk] = useState(false);
  const [busy, setBusy] = useState(false);

  const load = useCallback(() => {
    Promise.resolve(systemFontStatus()).then(
      (s) => s && setStatus(s),
      (e) => setError(errorText(e)),
    );
  }, []);
  useEffect(load, [load]);

  const run = async (on: boolean) => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await systemFontSet(on);
      setNotice(
        on
          ? "Fonte trocada. Saia e entre de novo no Windows (ou reinicie) para valer em tudo."
          : "Fonte original restaurada. Saia e entre de novo no Windows (ou reinicie).",
      );
      load();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  if (!status && !error) return null;
  return (
    <section className="style-card" aria-label="Fonte padrão do sistema">
      <h3>Fonte padrão do sistema: JetBrains Mono</h3>
      <p className="muted">
        Troca a Segoe UI (a fonte do Windows) pela JetBrains Mono. É uma fonte
        monoespaçada, então alguns textos do sistema podem ficar diferentes ou
        desalinhados. Precisa de administrador e só vale depois de sair e entrar de novo.
      </p>
      {status && !status.bundled && (
        <p role="alert">As fontes não vieram com esta instalação. Reinstale o Sentinela.</p>
      )}
      {status && !status.elevated && (
        <p className="muted">Abra o Sentinela como administrador para trocar.</p>
      )}
      <div className="row">
        <button
          type="button"
          className="btn"
          disabled={busy || !status?.bundled || status.active}
          onClick={() => setAsk(true)}
        >
          Usar JetBrains Mono
        </button>
        <button
          type="button"
          className="btn"
          disabled={busy || !status?.active}
          onClick={() => run(false)}
        >
          Voltar à fonte padrão
        </button>
      </div>
      {status?.active && <p className="muted">Troca ativa. Cópia de segurança: {status.backup_path}</p>}
      {notice && <p role="status">{notice}</p>}
      {error && <p role="alert">{error}</p>}
      {ask && (
        <ConfirmDialog
          title="Trocar a fonte do sistema?"
          confirmLabel="Trocar"
          onCancel={() => setAsk(false)}
          onConfirm={() => {
            setAsk(false);
            void run(true);
          }}
        >
          <p>
            A Segoe UI será retirada da lista de fontes do Windows e substituída pela
            JetBrains Mono. Uma cópia de segurança é guardada e o botão "Voltar à fonte
            padrão" desfaz tudo.
          </p>
        </ConfirmDialog>
      )}
    </section>
  );
}
