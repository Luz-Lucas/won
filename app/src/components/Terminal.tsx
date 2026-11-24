import { useCallback, useEffect, useState } from "react";
import {
  terminalApply,
  terminalDisablePrompt,
  terminalHistory,
  terminalInstallFont,
  terminalInstallOmp,
  terminalPlan,
  terminalThemePreview,
  terminalThemes,
  terminalTiming,
  terminalUndo,
} from "../api";
import type {
  TerminalHistoryItem,
  TerminalPlan,
  TerminalRequest,
  TerminalStatus,
  TerminalTiming,
} from "../api";
import ConfirmDialog from "./ConfirmDialog";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

const EXTRAS = [
  { id: "terminal-icons", label: "Ícones no ls (Terminal-Icons)" },
  { id: "prediction-history", label: "Sugestões pelo histórico" },
];

/** Oh My Posh, fonte, cores do Windows Terminal e o que dá para desfazer. */
export default function Terminal({
  status,
  engine,
  onChanged,
}: {
  status: TerminalStatus;
  engine: boolean;
  onChanged: () => void;
}) {
  const [themes, setThemes] = useState<string[]>([]);
  const [theme, setTheme] = useState<string | null>(null);
  const [sample, setSample] = useState<string | null>(null);
  const [timing, setTiming] = useState<TerminalTiming | null>(null);
  const [extras, setExtras] = useState<string[]>([]);
  const [font, setFont] = useState<string | null>(null);
  const [scheme, setScheme] = useState(false);
  const [plan, setPlan] = useState<TerminalPlan | null>(null);
  const [confirmInstall, setConfirmInstall] = useState<
    { kind: "omp" | "update" } | { kind: "font"; name: string } | null
  >(null);
  const [history, setHistory] = useState<TerminalHistoryItem[]>([]);
  const [forceFor, setForceFor] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const loadHistory = useCallback(() => {
    terminalHistory().then(setHistory, () => setHistory([]));
  }, []);

  useEffect(() => {
    if (!status.installed) return;
    terminalThemes().then(setThemes, (e) => setError(errorText(e)));
  }, [status.installed]);
  useEffect(loadHistory, [loadHistory]);

  const run = async (f: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    setNote(null);
    try {
      await f();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const pick = (name: string | null) =>
    run(async () => {
      setTheme(name);
      setSample(null);
      setTiming(null);
      if (!name) return;
      setSample(await terminalThemePreview(name));
    });

  const measure = () =>
    run(async () => {
      if (theme) setTiming(await terminalTiming(theme));
    });

  const request = (): TerminalRequest => ({
    theme,
    extras: theme ? extras : [],
    font,
    scheme_from_wallpaper: scheme,
  });

  // Nada é gravado sem passar por aqui: primeiro a prévia do que muda.
  const preview = () =>
    run(async () => {
      const p = await terminalPlan(request());
      if (p.changes.length === 0 && p.warnings.length === 0) {
        setNote("Nada a mudar: já está assim.");
        return;
      }
      setPlan(p);
    });

  const apply = () =>
    run(async () => {
      setPlan(null);
      const r = await terminalApply(request());
      setNote(
        r.errors.length === 0
          ? `Pronto: ${r.changed} arquivo(s) alterado(s). Abra um terminal novo para ver.`
          : null,
      );
      if (r.errors.length > 0) setError(r.errors.join(" "));
      loadHistory();
      onChanged();
    });

  const undo = (force: boolean) =>
    run(async () => {
      setForceFor(null);
      const r = await terminalUndo(force);
      if (r.changed_after) {
        setForceFor(r.changed_after);
        return;
      }
      if (r.undone) setNote(`Desfeito: ${r.undone.label}.`);
      loadHistory();
      onChanged();
    });

  const install = () =>
    run(async () => {
      const c = confirmInstall;
      setConfirmInstall(null);
      if (!c) return;
      if (c.kind === "font") {
        await terminalInstallFont(c.name);
        setNote(`Fonte ${c.name} instalada. Escolha-a no Windows Terminal ou aplique abaixo.`);
      } else {
        setNote(await terminalInstallOmp(c.kind === "update"));
      }
      onChanged();
    });

  const disable = () =>
    run(async () => {
      const n = await terminalDisablePrompt();
      setNote(n > 0 ? "Bloco do Sentinela removido do perfil." : "Não havia bloco para remover.");
      loadHistory();
      onChanged();
    });

  const canApply = theme !== null || font !== null || scheme;
  const active = status.profiles.some((p) => p.active);

  return (
    <div className="terminal">
      {error && (
        <p role="alert" className="form-error">
          {error}
        </p>
      )}
      {note && <p className="notice">{note}</p>}

      <h2>Oh My Posh</h2>
      {status.installed ? (
        <p>
          {`Instalado${status.version ? ` (versão ${status.version})` : ""}.`}{" "}
          {status.update_available && (
            <button
              type="button"
              className="btn"
              disabled={busy}
              onClick={() => setConfirmInstall({ kind: "update" })}
            >
              Atualizar
            </button>
          )}
        </p>
      ) : (
        <div>
          <p className="muted">
            O Oh My Posh desenha o prompt do PowerShell. Ele é instalado pelo winget, só para o
            seu usuário.
          </p>
          <button
            type="button"
            className="btn"
            disabled={busy}
            onClick={() => setConfirmInstall({ kind: "omp" })}
          >
            Instalar o Oh My Posh
          </button>
        </div>
      )}

      {status.installed && (
        <>
          <h2>Tema do prompt</h2>
          <label>
            Tema
            <select
              value={theme ?? ""}
              disabled={busy}
              onChange={(e) => void pick(e.target.value || null)}
            >
              <option value="">Não mudar</option>
              {themes.map((t) => (
                <option key={t} value={t}>
                  {t}
                </option>
              ))}
            </select>
          </label>
          {sample && (
            <div>
              <pre aria-label="Amostra do prompt">{sample}</pre>
              <button type="button" className="btn" disabled={busy} onClick={() => void measure()}>
                Medir o tempo do prompt
              </button>
              {timing && (
                <p className={timing.slow ? "form-error" : "muted"}>
                  {`Mediana de ${timing.runs} execuções: ${timing.median_ms} ms.`}
                  {timing.slow && " Está lento; segmentos como o do git podem pesar."}
                </p>
              )}
            </div>
          )}
          {theme && (
            <fieldset>
              <legend>Extras do PowerShell</legend>
              {EXTRAS.map((x) => (
                <label key={x.id} className="clean-check">
                  <input
                    type="checkbox"
                    checked={extras.includes(x.id)}
                    onChange={(e) =>
                      setExtras((cur) =>
                        e.target.checked ? [...cur, x.id] : cur.filter((i) => i !== x.id),
                      )
                    }
                  />
                  <span>{x.label}</span>
                </label>
              ))}
            </fieldset>
          )}

          <h2>Fonte</h2>
          <p className="muted">
            Os temas usam ícones que só aparecem numa fonte Nerd Font. O Oh My Posh a instala
            para o seu usuário.
          </p>
          {status.fonts.map((f) => (
            <div key={f.name} className="clean-head">
              <label className="clean-check">
                <input
                  type="radio"
                  name="terminal-font"
                  checked={font === f.name}
                  onChange={() => setFont(f.name)}
                />
                <span>{f.label}</span>
              </label>
              <button
                type="button"
                className="btn"
                disabled={busy}
                onClick={() => setConfirmInstall({ kind: "font", name: f.name })}
              >
                {`Instalar ${f.label}`}
              </button>
            </div>
          ))}
          {font && (
            <button type="button" className="btn" onClick={() => setFont(null)}>
              Não mudar a fonte
            </button>
          )}
        </>
      )}

      <h2>Cores do Windows Terminal</h2>
      {status.windows_terminal ? (
        <label className="clean-check">
          <input
            type="checkbox"
            checked={scheme}
            disabled={!engine}
            onChange={(e) => setScheme(e.target.checked)}
          />
          <span>Gerar um esquema de cores a partir do papel de parede</span>
        </label>
      ) : (
        <p className="muted">O Windows Terminal não foi encontrado.</p>
      )}
      {status.windows_terminal && !engine && (
        <p className="muted">Precisa do Wallpaper Engine para tirar as cores.</p>
      )}

      <p>
        <button
          type="button"
          className="btn"
          disabled={busy || !canApply}
          onClick={() => void preview()}
        >
          Ver o que muda
        </button>{" "}
        {active && (
          <button type="button" className="btn" disabled={busy} onClick={() => void disable()}>
            Tirar o prompt do perfil
          </button>
        )}
      </p>

      <h2>Histórico</h2>
      {history.length === 0 ? (
        <p className="muted">Nada alterado ainda.</p>
      ) : (
        <>
          <ul>
            {history.slice(0, 5).map((h) => (
              <li key={h.seq}>{`${h.label} — ${h.path}`}</li>
            ))}
          </ul>
          <button type="button" className="btn" disabled={busy} onClick={() => void undo(false)}>
            Desfazer a última alteração
          </button>
        </>
      )}

      {plan && (
        <ConfirmDialog
          title="Aplicar no terminal?"
          confirmLabel="Aplicar"
          onConfirm={() => void apply()}
          onCancel={() => setPlan(null)}
        >
          {plan.warnings.map((w) => (
            <p key={w} className="form-error">
              {w}
            </p>
          ))}
          {plan.disabled.length > 0 && (
            <p>{`Desligados por segurança: ${plan.disabled.join("; ")}.`}</p>
          )}
          {plan.changes.map((c) => (
            <div key={c.path}>
              <strong>{c.label}</strong>
              <div className="muted">{`${c.path}${c.is_new ? " (novo)" : ""}`}</div>
              <pre aria-label={`Mudanças em ${c.label}`}>
                {c.lines.map(([add, line]) => `${add ? "+" : "-"} ${line}`).join("\n")}
              </pre>
            </div>
          ))}
          <p>Antes de gravar, o Sentinela guarda uma cópia de cada arquivo para desfazer.</p>
        </ConfirmDialog>
      )}

      {confirmInstall && (
        <ConfirmDialog
          title={
            confirmInstall.kind === "font"
              ? `Instalar a fonte ${confirmInstall.name}?`
              : confirmInstall.kind === "update"
                ? "Atualizar o Oh My Posh?"
                : "Instalar o Oh My Posh?"
          }
          confirmLabel={confirmInstall.kind === "update" ? "Atualizar" : "Instalar"}
          onConfirm={() => void install()}
          onCancel={() => setConfirmInstall(null)}
        >
          <p>
            {confirmInstall.kind === "font"
              ? "O Oh My Posh baixa a fonte e a instala só para o seu usuário."
              : "O winget instala o programa só para o seu usuário, sem administrador."}
          </p>
        </ConfirmDialog>
      )}

      {forceFor && (
        <ConfirmDialog
          title="O arquivo mudou depois do Sentinela"
          confirmLabel="Desfazer mesmo assim"
          onConfirm={() => void undo(true)}
          onCancel={() => setForceFor(null)}
        >
          <p>{`${forceFor} foi editado depois da alteração. Desfazer volta ao conteúdo de antes e perde essas edições.`}</p>
        </ConfirmDialog>
      )}
    </div>
  );
}
