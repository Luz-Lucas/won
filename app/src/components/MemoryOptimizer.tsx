import { useCallback, useEffect, useState } from "react";
import { ramConfigGet, ramConfigSet, ramOptimize, ramStatus } from "../api";
import type { RamConfig, RamResult, RamStatus } from "../api";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));
const gb = (mb: number) => `${(mb / 1024).toFixed(1)} GB`;

/** Libera memória sob demanda ou por limiar, mostrando o antes e o depois. */
export default function MemoryOptimizer() {
  const [status, setStatus] = useState<RamStatus | null>(null);
  const [cfg, setCfg] = useState<RamConfig | null>(null);
  const [result, setResult] = useState<RamResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState("");

  const refresh = useCallback(() => ramStatus().then(setStatus, () => undefined), []);

  useEffect(() => {
    void refresh();
    ramConfigGet().then(setCfg, (e) => setError(errorText(e)));
    const t = window.setInterval(() => void refresh(), 3000);
    return () => window.clearInterval(t);
  }, [refresh]);

  const save = async (next: RamConfig) => {
    setCfg(next);
    try {
      setCfg(await ramConfigSet(next));
    } catch (e) {
      setError(errorText(e));
    }
  };

  const run = async () => {
    setBusy(true);
    setError(null);
    try {
      setResult(await ramOptimize());
      void refresh();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const addExclude = () => {
    const name = draft.trim();
    if (!cfg || !name || cfg.exclude.some((e) => e.toLowerCase() === name.toLowerCase())) return;
    setDraft("");
    void save({ ...cfg, exclude: [...cfg.exclude, name] });
  };

  return (
    <section className="card ram" aria-label="Otimizar memória RAM">
      <h2>Memória RAM</h2>
      {status && (
        <>
          <div
            className="ram-bar"
            role="progressbar"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={status.percent}
            aria-label="Uso de memória"
          >
            <span style={{ width: `${status.percent}%` }} />
          </div>
          <p>
            <strong>{status.percent}%</strong> em uso: {gb(status.used_mb)} de {gb(status.total_mb)} (
            {gb(status.available_mb)} livres)
          </p>
        </>
      )}
      <div className="ram-actions">
        <button type="button" className="btn btn-primary" disabled={busy} onClick={() => void run()}>
          {busy ? "Otimizando…" : "Otimizar agora"}
        </button>
      </div>
      {result && (
        <p role="status" className="ram-result">
          {result.freed_mb > 0
            ? `Liberou ${result.freed_mb} MB: de ${result.used_before_mb} MB para ${result.used_after_mb} MB em uso.`
            : `Sem ganho agora: ${result.used_before_mb} MB antes, ${result.used_after_mb} MB depois.`}{" "}
          {`${result.trimmed} processos reduzidos.`}
          {result.standby_cleared && " Lista de espera esvaziada."}
          {result.standby_note && ` Lista de espera não esvaziada: ${result.standby_note}.`}
        </p>
      )}
      {error && (
        <p role="alert" className="form-error">
          {error}
        </p>
      )}
      {cfg && (
        <div className="ram-settings">
          <label className="clean-check">
            <input type="checkbox" checked={cfg.auto} onChange={(e) => void save({ ...cfg, auto: e.target.checked })} />
            <span>Otimizar automaticamente quando o uso passar de</span>
          </label>
          <label>
            <input
              type="range"
              min={50}
              max={98}
              step={1}
              aria-label="Limiar de uso da memória"
              value={cfg.threshold}
              onChange={(e) => setCfg({ ...cfg, threshold: Number(e.target.value) })}
              onPointerUp={() => void save(cfg)}
              onKeyUp={() => void save(cfg)}
            />{" "}
            {cfg.threshold}%
          </label>
          <label className="clean-check">
            <input
              type="checkbox"
              checked={cfg.clear_standby}
              onChange={(e) => void save({ ...cfg, clear_standby: e.target.checked })}
            />
            <span>Esvaziar também a lista de espera do Windows (precisa de administrador)</span>
          </label>
          <div className="ram-exclude">
            <strong>Nunca mexer nestes programas</strong>
            <ul>
              {cfg.exclude.map((e) => (
                <li key={e}>
                  {e}{" "}
                  <button
                    type="button"
                    className="btn"
                    aria-label={`Remover ${e}`}
                    onClick={() => void save({ ...cfg, exclude: cfg.exclude.filter((x) => x !== e) })}
                  >
                    Remover
                  </button>
                </li>
              ))}
            </ul>
            <div className="ram-add">
              <input
                type="text"
                value={draft}
                placeholder="programa.exe"
                aria-label="Nome do programa a proteger"
                onChange={(e) => setDraft(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && addExclude()}
              />
              <button type="button" className="btn" onClick={addExclude}>
                Adicionar
              </button>
            </div>
          </div>
        </div>
      )}
      <p className="muted">
        Reduz a memória que os programas mantêm na RAM; o ganho dura até eles pedirem de novo, e
        limpar a lista de espera pode deixar o sistema um instante mais lento enquanto o cache é
        refeito. Processos do sistema e da segurança nunca são tocados.
      </p>
    </section>
  );
}
