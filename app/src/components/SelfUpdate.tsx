import { useEffect, useState } from "react";
import { updateCheck, updateConfigSet, updateInstall, updateStatus } from "../api";
import type { UpdateStatus } from "../api";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

/** Atualização automática do WON: busca, baixa, confere o hash e instala em modo silencioso. */
export default function SelfUpdate() {
  const [st, setSt] = useState<UpdateStatus | null>(null);
  const [url, setUrl] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [installing, setInstalling] = useState(false);

  useEffect(() => {
    updateStatus().then(
      (s) => {
        setSt(s);
        setUrl(s.manifest_url);
        // Ao abrir a tela já confere se há versão nova (o botão "Baixar e instalar" aparece sozinho).
        void Promise.resolve(updateCheck()).then(
          (r) => {
            if (r) setSt(r);
          },
          () => undefined,
        );
      },
      (e) => setError(errorText(e)),
    );
  }, []);

  const run = async (f: () => Promise<UpdateStatus>) => {
    setBusy(true);
    setError(null);
    try {
      const s = await f();
      setSt(s);
      setUrl(s.manifest_url);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const save = (autoCheck: boolean, autoInstall: boolean) =>
    run(() => updateConfigSet(autoCheck, autoInstall, url));

  const install = async () => {
    setBusy(true);
    setError(null);
    try {
      await updateInstall();
      setInstalling(true);
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="card" aria-label="Atualização do WON">
      <h2>Atualização do WON</h2>
      <p className="muted">
        Versão instalada: <strong>{st?.current || "desconhecida"}</strong>. O WON confere o SHA-256 do
        instalador antes de instalar. O instalador não é assinado, então o Windows pode mostrar um
        aviso (SmartScreen) se você abri-lo à mão; pela atualização do WON ele roda direto.
      </p>
      {st?.available && !installing && (
        <p role="status">
          Nova versão <strong>{st.latest}</strong> disponível.{st.notes ? ` ${st.notes}` : ""}
        </p>
      )}
      {st && !st.available && st.last_check > 0 && !st.error && (
        <p role="status">Você está na versão mais recente.</p>
      )}
      {installing && (
        <p role="status">Baixando e instalando… o WON fecha, atualiza tudo (barra, widgets, mods) e abre de novo sozinho em instantes.</p>
      )}
      {(error || st?.error) && (
        <p role="alert" className="form-error">
          {error ?? st?.error}
        </p>
      )}
      <div className="ram-actions">
        <button type="button" className="btn" disabled={busy} onClick={() => void run(updateCheck)}>
          Verificar atualizações
        </button>
        {st?.available && (
          <button type="button" className="btn primary" disabled={busy || installing} onClick={() => void install()}>
            Baixar e instalar agora
          </button>
        )}
      </div>
      <label className="check">
        <input
          type="checkbox"
          checked={st?.auto_check ?? true}
          disabled={busy || !st}
          onChange={(e) => void save(e.target.checked, st?.auto_install ?? false)}
        />
        Procurar atualizações automaticamente (a cada 30 minutos)
      </label>
      <label className="check">
        <input
          type="checkbox"
          checked={st?.auto_install ?? false}
          disabled={busy || !st || !st.auto_check}
          onChange={(e) => void save(true, e.target.checked)}
        />
        Instalar sozinho quando houver versão nova
      </label>
      <label className="field">
        Fonte das atualizações
        <input
          type="url"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onBlur={() => st && url !== st.manifest_url && void save(st.auto_check, st.auto_install)}
        />
      </label>
    </section>
  );
}
