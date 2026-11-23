import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  appsInstall,
  appsPage,
  appsPageRefresh,
  appsUpgrade,
  STORE_APP_INSTALLER_URL,
} from "../api";
import type { AppItem, AppOutcome, AppsPage } from "../api";
import ConfirmDialog from "../components/ConfirmDialog";
import ProgressBar from "../components/ProgressBar";
import UpdatesBlockedNotice from "../components/UpdatesBlockedNotice";
import { useJob } from "../jobs";

type Action = "install" | "upgrade";

const LICENSE = "Ao continuar, você aceita os termos de licença de cada aplicativo.";
const NO_WINGET =
  "O Sentinela usa o winget, o instalador oficial da Microsoft, para baixar cada aplicativo do fabricante e conferir o arquivo.";
const UNKNOWN = "Não foi possível verificar o que já está instalado.";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

/** Minúsculas e sem acentos, para a busca. */
const norm = (s: string) => s.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();

export default function Aplicativos() {
  const [data, setData] = useState<AppsPage | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState("");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [pending, setPending] = useState<{ action: Action; ids: string[] } | null>(null);
  const [openError, setOpenError] = useState<string | null>(null);
  const [results, setResults] = useState<AppOutcome[] | null>(null);
  const [skipped, setSkipped] = useState(0);
  const [cancelling, setCancelling] = useState(false);
  // Só a resposta do pedido mais recente vale (e nenhuma depois de desmontar).
  const request = useRef(0);
  const job = useJob<AppOutcome[]>("apps");

  const load = useCallback((refresh: boolean) => {
    const n = ++request.current;
    setLoadError(null);
    setChecking(true);
    (refresh ? appsPageRefresh : appsPage)().then(
      (p) => {
        if (n !== request.current) return;
        setChecking(false);
        setData(p);
        // Quem já está instalado (sem atualização) sai da seleção.
        const done = new Set(p.apps.filter((a) => a.state === "installed").map((a) => a.app.id));
        setSelected((s) => (done.size === 0 ? s : new Set([...s].filter((id) => !done.has(id)))));
      },
      (e) => {
        if (n !== request.current) return;
        setChecking(false);
        setLoadError(errorText(e));
      },
    );
  }, []);

  useEffect(() => {
    load(false);
    return () => {
      request.current++;
    };
  }, [load]);

  // A janela voltou da bandeja: o backend descartou o cache; relê os estados.
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void listen("app:shown", () => load(false)).then((un) => {
      if (disposed) un();
      else unlisten = un;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [load]);

  // Terminada a fila (mesmo se a página foi aberta depois de iniciá-la; só esta
  // página inicia filas de aplicativos): mostra o resultado, recarrega o estado
  // e tira da seleção o que deu certo. Cada fila é tratada uma única vez.
  const { state: jobState, result: jobResult, job: jobId, progress: jobProgress, claim } = job;
  useEffect(() => {
    if (jobState !== "done" || !Array.isArray(jobResult) || jobId === null) return;
    if (!claim(jobId)) return;
    const total = jobProgress?.phase === "apps" ? jobProgress.total : jobResult.length;
    setResults(jobResult);
    setSkipped(Math.max(0, total - jobResult.length));
    const ok = new Set(jobResult.filter((r) => r.ok).map((r) => r.id));
    setSelected((s) => new Set([...s].filter((id) => !ok.has(id))));
    load(true);
  }, [jobState, jobResult, jobId, jobProgress, claim, load]);

  const running = job.state === "running";

  // O cancelamento vale só para o job em andamento.
  useEffect(() => {
    if (job.state !== "running") setCancelling(false);
  }, [job.state]);

  const stop = () => {
    setCancelling(true);
    job.cancel().catch(() => setCancelling(false));
  };
  const apps = data?.apps ?? [];
  const byId = (id: string) => apps.find((a) => a.app.id === id);
  const nameOf = (id: string) => byId(id)?.app.name ?? id;

  const unknown = data !== null && data.availability.available && apps.length > 0 && apps.every((a) => a.state === null);

  const q = norm(query.trim());
  const visible = apps.filter(
    (a) =>
      (category === "" || a.app.category === category) &&
      (q === "" || norm(a.app.name).includes(q) || norm(a.app.description).includes(q)),
  );

  const chosen = apps.filter((a) => selected.has(a.app.id));
  // Estado desconhecido (null): instalar é permitido, o winget decide.
  const toInstall = chosen.filter((a) => a.state === "not_installed" || a.state === null);
  const toUpgrade = chosen.filter((a) => a.state === "update_available");
  const allUpgradable = apps.filter((a) => a.state === "update_available");

  const toggle = (id: string) =>
    setSelected((s) => {
      const next = new Set(s);
      if (!next.delete(id)) next.add(id);
      return next;
    });

  const ask = (action: Action, list: AppItem[]) => {
    if (list.length === 0) return;
    setPending({ action, ids: list.map((a) => a.app.id) });
  };

  const confirm = () => {
    if (!pending) return;
    const { action, ids } = pending;
    setPending(null);
    setResults(null);
    setSkipped(0);
    void job.start(() => (action === "install" ? appsInstall(ids) : appsUpgrade(ids)));
  };

  const open = (url: string) => {
    setOpenError(null);
    openUrl(url).catch((e) => setOpenError(errorText(e)));
  };

  const progress = job.progress && job.progress.phase === "apps" ? job.progress : null;
  const verb = progress?.verb === "upgrade" ? "Atualizando" : "Instalando";

  return (
    <section className="page">
      <h1>Aplicativos</h1>
      <UpdatesBlockedNotice />
      {data && (
        <div className="apps-recheck">
          <button type="button" className="btn" disabled={running || checking} onClick={() => load(true)}>
            Verificar de novo
          </button>
        </div>
      )}
      {loadError && (
        <div>
          <p role="alert" className="form-error">
            Não foi possível carregar os aplicativos: {loadError}
          </p>
          <button type="button" className="btn" onClick={() => load(false)}>
            Tentar de novo
          </button>
        </div>
      )}
      {openError && (
        <p role="alert" className="form-error">
          {openError}
        </p>
      )}
      {job.error && (
        <p role="alert" className="form-error">
          Não foi possível concluir: {job.error}
        </p>
      )}
      {!data && !loadError && <p className="muted">Carregando…</p>}

      {data && !data.availability.available && (
        <div className="notice">
          {data.availability.reason && <p>{data.availability.reason}</p>}
          <p>{NO_WINGET}</p>
          <button type="button" className="btn btn-primary" onClick={() => open(STORE_APP_INSTALLER_URL)}>
            Abrir na Microsoft Store
          </button>
        </div>
      )}

      {data && data.availability.available && (
        <>
          {unknown && (
            <div className="notice">
              <p>{UNKNOWN}</p>
            </div>
          )}
          <div className="apps-toolbar">
            <input
              type="search"
              className="apps-search"
              aria-label="Buscar aplicativos"
              placeholder="Buscar aplicativos"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            <select
              className="apps-category"
              aria-label="Categoria"
              value={category}
              onChange={(e) => setCategory(e.target.value)}
            >
              <option value="">Todas</option>
              {data.categories.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.label}
                </option>
              ))}
            </select>
            <button
              type="button"
              className="btn"
              disabled={running || allUpgradable.length === 0}
              onClick={() => ask("upgrade", allUpgradable)}
            >
              Atualizar tudo
            </button>
          </div>

          {running && (
            <div className="notice" role="status">
              {progress && (
                <p>{`${verb} ${progress.index + 1} de ${progress.total}: ${progress.current}…`}</p>
              )}
              <ProgressBar progress={job.progress} />
              <button type="button" className="btn" disabled={cancelling} onClick={stop}>
                {cancelling ? "Parando…" : "Parar depois deste"}
              </button>
            </div>
          )}

          {results && (
            <div className="notice">
              <ul className="tweak-results">
                {results.map((r) => (
                  <li key={r.id} className={r.ok ? undefined : "form-error"}>
                    {`${r.ok ? "✓" : "✗"} ${nameOf(r.id)}: ${r.message}`}
                  </li>
                ))}
              </ul>
              {skipped > 0 && <p className="muted">{`${skipped} não processado(s).`}</p>}
            </div>
          )}

          {visible.length === 0 && <p className="muted">Nenhum aplicativo encontrado.</p>}
          <ul className="apps-grid">
            {visible.map((i) => {
              const a = i.app;
              const locked = i.state === "installed";
              return (
                <li key={a.id} className="app-card">
                  <label className="clean-check">
                    <input
                      type="checkbox"
                      checked={selected.has(a.id)}
                      disabled={locked || running}
                      onChange={() => toggle(a.id)}
                    />
                    <strong>{a.name}</strong>
                  </label>
                  <p className="muted app-desc">{a.description}</p>
                  <div className="app-badges">
                    {a.foss && <span className="tweak-badge">Código aberto</span>}
                    {i.state === "installed" && <span className="tweak-badge">Instalado</span>}
                    {i.state === "update_available" && (
                      <span className="tweak-badge">Atualização disponível</span>
                    )}
                    <button type="button" className="link-btn" onClick={() => open(a.link)}>
                      Site
                    </button>
                  </div>
                </li>
              );
            })}
          </ul>

          {chosen.length > 0 && (
            <div className="apps-actionbar">
              <span>{`${chosen.length} selecionado(s)`}</span>
              <div className="apps-actions">
                <button
                  type="button"
                  className="btn btn-primary"
                  disabled={running || toInstall.length === 0}
                  onClick={() => ask("install", toInstall)}
                >
                  Instalar
                </button>
                <button
                  type="button"
                  className="btn"
                  disabled={running || toUpgrade.length === 0}
                  onClick={() => ask("upgrade", toUpgrade)}
                >
                  Atualizar
                </button>
              </div>
            </div>
          )}
        </>
      )}

      {pending && (
        <ConfirmDialog
          title={`${pending.action === "install" ? "Instalar" : "Atualizar"} ${pending.ids.length} aplicativo(s)?`}
          confirmLabel={pending.action === "install" ? "Instalar" : "Atualizar"}
          onConfirm={confirm}
          onCancel={() => setPending(null)}
        >
          <ul>
            {pending.ids.map((id) => (
              <li key={id}>{nameOf(id)}</li>
            ))}
          </ul>
          <p>{LICENSE}</p>
        </ConfirmDialog>
      )}
    </section>
  );
}
