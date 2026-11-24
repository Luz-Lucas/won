import { openUrl } from "@tauri-apps/plugin-opener";
import { useCallback, useEffect, useState } from "react";
import {
  abuseKeyClear,
  abuseKeyPresent,
  abuseKeySet,
  settingsGet,
  settingsSet,
  signaturesUpdate,
  systemStatus,
  type DbStatus,
  type KeySource,
  type SettingsView,
  type UpdateSummary,
} from "../api";
import ScanSettings from "../components/ScanSettings";
import WindowsUpdates from "../components/WindowsUpdates";
import { humanDate } from "../format";
import { useJob } from "../jobs";

const AUTH_URL = "https://auth.abuse.ch/";
const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));
const number = new Intl.NumberFormat("pt-BR");

export default function Ajustes() {
  const job = useJob<UpdateSummary>("update");
  const [db, setDb] = useState<DbStatus | null>(null);
  const [dbError, setDbError] = useState<string | null>(null);
  const [keySource, setKeySource] = useState<KeySource | null>(null);
  const [keyInput, setKeyInput] = useState("");
  const [keyBusy, setKeyBusy] = useState(false);
  const [keyError, setKeyError] = useState<string | null>(null);

  const [prefs, setPrefs] = useState<SettingsView | null>(null);
  const [prefsError, setPrefsError] = useState<string | null>(null);
  const [prefsBusy, setPrefsBusy] = useState(false);

  useEffect(() => {
    settingsGet().then(setPrefs, (e) => setPrefsError(errorText(e)));
  }, []);

  const changePrefs = async (start: boolean, tray: boolean) => {
    setPrefsBusy(true);
    setPrefsError(null);
    try {
      setPrefs(await settingsSet(start, tray));
    } catch (e) {
      setPrefsError(errorText(e));
    } finally {
      setPrefsBusy(false);
    }
  };

  const running = job.state === "running";

  const loadDb = useCallback(() => {
    systemStatus().then(
      (s) => {
        setDb(s.db);
        setDbError(null);
      },
      (e) => setDbError(errorText(e)),
    );
  }, []);
  useEffect(loadDb, [loadDb]);

  const loadKey = useCallback(() => {
    abuseKeyPresent().then(
      (p) => setKeySource(p),
      () => setKeySource("none"),
    );
  }, []);
  useEffect(loadKey, [loadKey]);

  // Depois de uma atualização concluída, relê as fontes e as datas.
  useEffect(() => {
    if (job.state === "done") loadDb();
  }, [job.state, loadDb]);

  const saveKey = async () => {
    setKeyBusy(true);
    setKeyError(null);
    try {
      await abuseKeySet(keyInput);
      setKeyInput("");
      loadKey();
    } catch (e) {
      setKeyError(errorText(e));
    } finally {
      setKeyBusy(false);
    }
  };

  const removeKey = async () => {
    setKeyBusy(true);
    setKeyError(null);
    try {
      await abuseKeyClear();
      loadKey();
    } catch (e) {
      setKeyError(errorText(e));
    } finally {
      setKeyBusy(false);
    }
  };

  const openAuth = (e: React.MouseEvent) => {
    e.preventDefault();
    openUrl(AUTH_URL).catch(() => {
      /* sem navegador: o endereço continua visível na tela */
    });
  };

  const result = job.state === "done" ? job.result : null;

  return (
    <section className="page">
      <h1>Ajustes</h1>

      <h2 className="section-title">Inicialização</h2>
      {prefs && (
        <>
          <p>
            <label>
              <input
                type="checkbox"
                checked={prefs.start_with_windows}
                disabled={prefsBusy}
                onChange={(e) =>
                  void changePrefs(e.target.checked, prefs.close_to_tray)
                }
              />{" "}
              Iniciar com o Windows
            </label>
          </p>
          <p>
            <label>
              <input
                type="checkbox"
                checked={prefs.close_to_tray}
                disabled={prefsBusy}
                onChange={(e) =>
                  void changePrefs(prefs.start_with_windows, e.target.checked)
                }
              />{" "}
              Ao fechar, manter na área de notificação
            </label>
          </p>
          {prefs.note && <p className="muted">{prefs.note}</p>}
        </>
      )}
      {prefsError && (
        <p role="alert" className="form-error">
          {prefsError}
        </p>
      )}

      <ScanSettings />

      <h2 className="section-title">Banco de assinaturas</h2>
      {dbError && (
        <p role="alert" className="form-error">
          Não foi possível ler o banco: {dbError}
        </p>
      )}
      {db && (
        <>
          <p className="muted">
            Pasta: <span>{db.dir}</span>
          </p>
          {!db.initialized ? (
            <p>Banco não inicializado</p>
          ) : (
            <ul className="activity">
              {db.sources.map((s) => (
                <li key={s.name}>
                  <span>{s.name}</span>{" "}
                  <span className="muted">
                    {`${humanDate(s.fetched_at)} · ${number.format(s.count)} itens`}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </>
      )}
      <p>
        <button
          type="button"
          className="btn btn-primary"
          disabled={running}
          onClick={() => void job.start(signaturesUpdate)}
        >
          {running ? "Atualizando…" : "Atualizar agora"}
        </button>
      </p>
      {job.state === "error" && job.error && (
        <p role="alert" className="form-error">
          {job.error}
        </p>
      )}
      {result && (
        <div>
          <p>{`Hashes: ${result.hashes}`}</p>
          <p>
            {`Regras YARA: ${result.rules_ok} arquivo(s) aceito(s), ${result.rules_rejected} rejeitado(s), ${result.rules_count} regra(s) no total`}
          </p>
          {result.warnings.length > 0 && (
            <ul className="activity">
              {result.warnings.map((w, i) => (
                <li key={i} className="muted">
                  {w}
                </li>
              ))}
            </ul>
          )}
        </div>
      )}

      <h2 className="section-title">Chave da abuse.ch</h2>
      <p className="muted">
        Libera os hashes do MalwareBazaar. Crie a conta gratuita em{" "}
        <a href={AUTH_URL} onClick={openAuth}>
          auth.abuse.ch
        </a>
        . A chave fica no Gerenciador de Credenciais do Windows e nunca é
        mostrada de volta.
      </p>
      <p>
        {keySource === null
          ? "Carregando…"
          : keySource === "stored"
            ? "Chave salva"
            : keySource === "env"
              ? "Usando a variável ABUSE_CH_AUTH_KEY"
              : "Sem chave (só as regras YARA são atualizadas)"}
      </p>
      <p>
        <label>
          Chave de autenticação{" "}
          <input
            type="password"
            autoComplete="off"
            spellCheck={false}
            value={keyInput}
            onChange={(e) => setKeyInput(e.target.value)}
          />
        </label>{" "}
        <button
          type="button"
          className="btn"
          disabled={keyBusy}
          onClick={() => void saveKey()}
        >
          Salvar
        </button>{" "}
        <button
          type="button"
          className="btn"
          disabled={keyBusy || keySource !== "stored"}
          onClick={() => void removeKey()}
        >
          Remover
        </button>
      </p>
      {keyError && (
        <p role="alert" className="form-error">
          {keyError}
        </p>
      )}

      <WindowsUpdates />
    </section>
  );
}
