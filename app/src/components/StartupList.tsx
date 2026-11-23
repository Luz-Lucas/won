import { useCallback, useEffect, useState } from "react";
import { startupList, startupSetEnabled } from "../api";
import type { StartupItem, StartupPage, StartupSource } from "../api";
import ConfirmDialog from "./ConfirmDialog";

const SOURCE: Record<StartupSource, string> = {
  run_hkcu: "Registro (usuário)",
  run_hklm: "Registro (todos os usuários)",
  run_hklm32: "Registro (todos os usuários, 32 bits)",
  startup_folder_user: "Pasta Inicializar (usuário)",
  startup_folder_common: "Pasta Inicializar (todos os usuários)",
};

const SENTINELA_NOTE = "Este é o Sentinela. Para desligar, use Ajustes → Inicialização.";

const MAX_COMMAND = 60;

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

const shorten = (s: string) =>
  s.length > MAX_COMMAND ? `${s.slice(0, MAX_COMMAND - 1)}…` : s;

/** Programas que iniciam com o Windows: liga e desliga como o Gerenciador de Tarefas. */
export default function StartupList() {
  const [data, setData] = useState<StartupPage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [disabling, setDisabling] = useState<StartupItem | null>(null);

  const load = useCallback(() => {
    setError(null);
    startupList().then(setData, (e) => setError(errorText(e)));
  }, []);

  useEffect(load, [load]);

  const set = async (item: StartupItem, enabled: boolean) => {
    setBusy(true);
    setError(null);
    try {
      setData(await startupSetEnabled(item.id, enabled));
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const toggle = (item: StartupItem) => {
    // Desligar pede confirmação; religar é imediato.
    if (item.enabled) setDisabling(item);
    else void set(item, true);
  };

  return (
    <section aria-labelledby="startup-title">
      <h2 id="startup-title">Programas que iniciam com o Windows</h2>
      <p className="muted">
        Desativar só impede o programa de abrir com o Windows, como no Gerenciador de
        Tarefas. Nada é apagado.
      </p>
      {error && (
        <p role="alert" className="form-error">
          {error}
        </p>
      )}
      {!data && !error && <p className="muted">Carregando…</p>}
      {data?.warnings.map((w) => (
        <p key={w} className="muted clean-warning">
          {w}
        </p>
      ))}
      {data && data.items.length === 0 && (
        <p className="muted">Nenhum programa inicia com o Windows.</p>
      )}
      {data && data.items.length > 0 && (
        <ul className="clean-list">
          {data.items.map((i) => (
            <li key={i.id} className="clean-row">
              <div className="clean-head">
                {i.sentinela ? (
                  <span className="clean-check">{i.name}</span>
                ) : (
                  <label className="clean-check">
                    <input
                      type="checkbox"
                      role="switch"
                      checked={i.enabled}
                      disabled={busy || i.locked !== null}
                      onChange={() => toggle(i)}
                    />
                    <span>{i.name}</span>
                  </label>
                )}
                <span className="muted">{SOURCE[i.source]}</span>
                {!i.file_exists && (
                  <span className="tweak-badge">Arquivo não encontrado</span>
                )}
                {i.locked && (
                  <span className="tweak-badge" title={i.locked}>
                    🔒 Bloqueado
                  </span>
                )}
              </div>
              <p className="muted clean-warning" title={i.command}>
                {shorten(i.command)}
              </p>
              {i.sentinela && <p className="muted clean-warning">{SENTINELA_NOTE}</p>}
              {i.locked && <p className="muted clean-warning">{i.locked}</p>}
            </li>
          ))}
        </ul>
      )}
      {disabling && (
        <ConfirmDialog
          title={`Desativar "${disabling.name}" na inicialização? O programa continua instalado.`}
          confirmLabel="Desativar"
          onConfirm={() => {
            const item = disabling;
            setDisabling(null);
            void set(item, false);
          }}
          onCancel={() => setDisabling(null)}
        >
          <p>Você pode ativar de novo quando quiser.</p>
        </ConfirmDialog>
      )}
    </section>
  );
}
