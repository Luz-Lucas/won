import { useCallback, useEffect, useState } from "react";
import { updatesPreview, updatesResume, updatesSetMode, updatesStatus } from "../api";
import type { TweakChange, UpdateMode, UpdatesPage } from "../api";
import ConfirmDialog from "./ConfirmDialog";

export const MODE_LABEL: Record<UpdateMode, string> = {
  default: "Padrão",
  security: "Só segurança",
  paused: "Pausado",
  blocked: "Bloqueado",
};

const MODES: { id: UpdateMode; title: string; text: string }[] = [
  {
    id: "default",
    title: "Padrão",
    text: "O Windows decide, como antes do Sentinela.",
  },
  {
    id: "security",
    title: "Só segurança (recomendado)",
    text: "Fixa a versão atual do Windows e adia as atualizações de recursos; as correções de segurança continuam chegando.",
  },
  {
    id: "paused",
    title: "Pausado até…",
    text: "Pausa tudo até a data escolhida (no máximo 35 dias); depois o Windows volta a atualizar sozinho.",
  },
  {
    id: "blocked",
    title: "Bloqueado",
    text: "Desliga as atualizações automáticas, os serviços e as tarefas de atualização.",
  },
];

const BLOCKED_WARNING =
  "O Windows fica sem correções de segurança enquanto durar. A Microsoft Store e alguns instaladores (pacotes da Store no winget, .NET, drivers) falham nesse modo. Para manter a versão estável sem ficar sem correções, prefira \"Só segurança\".";
const NEEDS_ADMIN =
  "Mudar as atualizações do Windows exige administrador. Use \"Reiniciar como administrador\" na barra lateral.";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

const MAX_PAUSE_DAYS = 35;

const isoDay = (d: Date) => d.toISOString().slice(0, 10);
const plusDays = (n: number) => isoDay(new Date(Date.now() + n * 86_400_000));

const dateText = (secs: number) =>
  new Date(secs * 1000).toLocaleString("pt-BR", { dateStyle: "short", timeStyle: "short" });

/** Seção "Atualizações do Windows" (§6.5): quatro modos com prévia e desfazer. */
export default function WindowsUpdates() {
  const [data, setData] = useState<UpdatesPage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [choice, setChoice] = useState<UpdateMode>("default");
  const [until, setUntil] = useState(plusDays(7));
  const [preview, setPreview] = useState<TweakChange[] | null>(null);

  const apply = (p: UpdatesPage) => {
    setData(p);
    setChoice(p.state.mode);
    if (p.state.paused_until) setUntil(p.state.paused_until);
  };

  const load = useCallback(() => {
    setError(null);
    updatesStatus().then(apply, (e) => setError(errorText(e)));
  }, []);

  useEffect(load, [load]);

  const run = async (f: () => Promise<UpdatesPage>) => {
    setBusy(true);
    setError(null);
    try {
      apply(await f());
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const current = data?.state.mode ?? "default";
  const pending =
    choice !== current || (choice === "paused" && until !== data?.state.paused_until);
  const locked = !data?.elevated;
  const untilArg = choice === "paused" ? until : null;

  const openPreview = async () => {
    setError(null);
    try {
      setPreview(await updatesPreview(choice, untilArg));
    } catch (e) {
      setError(errorText(e));
    }
  };

  const confirm = () => {
    setPreview(null);
    void run(() => updatesSetMode(choice, untilArg));
  };

  return (
    <div>
      <h2 className="section-title">Atualizações do Windows</h2>
      {data?.version && <p className="muted">{`Versão instalada: ${data.version}`}</p>}
      {data && locked && <p className="notice">{NEEDS_ADMIN}</p>}
      {error && (
        <p role="alert" className="form-error">
          {error}
        </p>
      )}
      {!data && !error && <p className="muted">Carregando…</p>}
      {data?.suspended && (
        <div className="notice">
          <p>
            {`Atualizações liberadas temporariamente. Quando o Windows terminar de instalar, volte ao modo ${MODE_LABEL[data.suspended.mode]}.`}
          </p>
          <button
            type="button"
            className="btn"
            disabled={busy || locked}
            onClick={() => void run(updatesResume)}
          >
            {`Voltar a ${MODE_LABEL[data.suspended.mode]}`}
          </button>
        </div>
      )}
      {data && data.state.reverted.length > 0 && (
        <div className="notice">
          <p>{`Parcial: o Windows desfez parte do modo ${MODE_LABEL[current]}:`}</p>
          <ul>
            {data.state.reverted.map((r) => (
              <li key={r}>{r}</li>
            ))}
          </ul>
          <button
            type="button"
            className="btn"
            disabled={busy || locked}
            onClick={() =>
              void run(() => updatesSetMode(current, data.state.paused_until))
            }
          >
            Aplicar de novo
          </button>
        </div>
      )}
      {data && (
        <fieldset className="choice-group update-modes" disabled={busy || locked}>
          <legend>Modo</legend>
          {MODES.map((m) => (
            <label key={m.id} className="clean-check">
              <input
                type="radio"
                name="update-mode"
                checked={choice === m.id}
                onChange={() => setChoice(m.id)}
              />
              <span>
                <strong>{m.title}</strong>
                {current === m.id && " (atual)"}
                <br />
                <span className="muted">{m.text}</span>
              </span>
            </label>
          ))}
          {choice === "paused" && (
            <label className="clean-check">
              <span>Pausar até</span>
              <input
                type="date"
                aria-label="Pausar até"
                value={until}
                min={plusDays(1)}
                max={plusDays(MAX_PAUSE_DAYS)}
                onChange={(e) => setUntil(e.target.value)}
              />
            </label>
          )}
        </fieldset>
      )}
      {data && (current === "blocked" || current === "paused") && (
        <p className="muted">
          {data.defender.last_ok
            ? `Definições do Defender atualizadas pelo Sentinela em ${dateText(data.defender.last_ok)}.`
            : "O Sentinela atualiza as definições do Defender uma vez por dia enquanto este modo durar."}
          {data.defender.last_error && ` Última tentativa falhou: ${data.defender.last_error}`}
        </p>
      )}
      {data && (
        <button
          type="button"
          className="btn btn-primary"
          disabled={busy || locked || !pending}
          onClick={() => void openPreview()}
        >
          Aplicar modo
        </button>
      )}
      {preview && (
        <ConfirmDialog
          title={
            choice === "paused"
              ? `Pausar as atualizações até ${until}?`
              : `Mudar as atualizações para "${MODE_LABEL[choice]}"?`
          }
          confirmLabel="Aplicar"
          onConfirm={confirm}
          onCancel={() => setPreview(null)}
        >
          {preview.length === 0 ? (
            <p>Nada muda no sistema.</p>
          ) : (
            <ul>
              {preview.map((c, n) => (
                <li key={`${c.description}-${n}`}>{`${c.description}: ${c.current} → ${c.target}`}</li>
              ))}
            </ul>
          )}
          {choice === "blocked" && <p>{BLOCKED_WARNING}</p>}
          {choice === "default" && <p>Volta tudo ao que era antes do Sentinela.</p>}
        </ConfirmDialog>
      )}
    </div>
  );
}
