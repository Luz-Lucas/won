import { useEffect, useState } from "react";
import {
  desktopConfigCheck,
  desktopConfigRead,
  desktopConfigWrite,
  desktopStart,
  desktopStop,
} from "../api";
import type { DesktopStatus } from "../api";
import DesktopRules from "./DesktopRules";
import DesktopVisual from "./DesktopVisual";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

/** Atalhos globais, regras de janela e bordas, pelo arquivo desktop.toml. */
export default function Desktop({
  status,
  onChanged,
}: {
  status: DesktopStatus;
  onChanged: () => void;
}) {
  const [text, setText] = useState<string | null>(null);
  const [saved, setSaved] = useState("");
  const [problems, setProblems] = useState<string[]>([]);
  const [note, setNote] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    desktopConfigRead().then(
      (t) => {
        setText(t);
        setSaved(t);
      },
      (e) => setError(errorText(e)),
    );
  }, []);

  const edit = (value: string) => {
    setText(value);
    setNote(null);
    desktopConfigCheck(value).then(setProblems, () => setProblems([]));
  };

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

  const save = () =>
    run(async () => {
      if (text === null) return;
      await desktopConfigWrite(text);
      setSaved(text);
      setNote("Salvo. O sentinela-desktop aplica na hora se estiver rodando.");
      onChanged();
    });

  const toggle = () =>
    run(async () => {
      if (status.running) await desktopStop();
      else await desktopStart();
      onChanged();
    });

  const dirty = text !== null && text !== saved;

  return (
    <div className="desktop">
      {error && (
        <p role="alert" className="form-error">
          {error}
        </p>
      )}
      {note && <p className="notice">{note}</p>}

      <h2>Ambiente de desktop</h2>
      <p className="muted">
        Atalhos globais, regras de janela e bordas. Usa só recursos públicos do Windows, sem
        injetar nada em outros programas. Em jogos em tela cheia ele não mexe em nenhuma janela.
      </p>
      <p>
        {status.running ? "Em execução." : "Parado."}{" "}
        <button type="button" className="btn" disabled={busy} onClick={() => void toggle()}>
          {status.running ? "Parar" : "Iniciar"}
        </button>
      </p>
      <p className="muted">
        Saída de emergência: <kbd>Win</kbd>+<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Esc</kbd> desfaz
        tudo o que ele mudou nas janelas e o encerra.
      </p>

      {status.errors.length > 0 && !dirty && (
        <div role="alert" className="form-error">
          <p>O arquivo atual tem problemas; o programa continua com a configuração anterior:</p>
          <ul>
            {status.errors.map((e) => (
              <li key={e}>{e}</li>
            ))}
          </ul>
        </div>
      )}

      <DesktopVisual text={text} onChange={edit} />
      <DesktopRules text={text} onChange={edit} />

      <details className="style-reference dv-advanced">
        <summary>Avançado: atalhos e regras no arquivo desktop.toml</summary>
      <h2>desktop.toml</h2>
      <p className="muted">{status.config_path}</p>
      <textarea
        aria-label="desktop.toml"
        rows={18}
        spellCheck={false}
        value={text ?? ""}
        disabled={text === null}
        onChange={(e) => edit(e.target.value)}
      />
      </details>
      {problems.length > 0 && (
        <ul aria-label="Problemas" className="form-error">
          {problems.map((p) => (
            <li key={p}>{p}</li>
          ))}
        </ul>
      )}
      <p>
        <button
          type="button"
          className="btn"
          disabled={busy || !dirty || problems.length > 0}
          onClick={() => void save()}
        >
          Salvar
        </button>{" "}
        {dirty && (
          <button type="button" className="btn" disabled={busy} onClick={() => edit(saved)}>
            Descartar
          </button>
        )}
      </p>
      <p className="muted">
        {`${status.hotkeys} atalho(s) e ${status.rules} regra(s) no arquivo salvo. Uma cópia do arquivo anterior fica em desktop.toml.bak.`}
      </p>
    </div>
  );
}
