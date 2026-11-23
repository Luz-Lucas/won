import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { gamesAdd, gamesGpuSet, gamesList, gamesRemove } from "../api";
import type { GameItem, GamesPage, GameSource } from "../api";
import ConfirmDialog from "./ConfirmDialog";

const SOURCE: Record<GameSource, string> = {
  steam: "Steam",
  epic: "Epic Games",
  riot: "Riot",
  manual: "Adicionado por você",
};

const EXTERNAL = "Configurado fora do Sentinela.";
const ONE_GPU = "Esta máquina tem uma GPU; a preferência não muda nada.";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

const fileName = (exe: string) => exe.split(/[\\/]/).pop() ?? exe;

type Pending = { item: GameItem; enabled: boolean } | { missing: string; title: string };

/** GPU de alto desempenho por jogo, como em Configurações → Sistema → Tela → Gráficos. */
export default function GpuPerGame() {
  const [data, setData] = useState<GamesPage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [pending, setPending] = useState<Pending | null>(null);

  const load = useCallback(() => {
    setError(null);
    gamesList().then(setData, (e) => setError(errorText(e)));
  }, []);

  useEffect(load, [load]);

  const run = async (f: () => Promise<GamesPage>) => {
    setBusy(true);
    setError(null);
    try {
      setData(await f());
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const confirm = () => {
    const p = pending;
    setPending(null);
    if (!p) return;
    if ("missing" in p) void run(() => gamesGpuSet(p.missing, false));
    else void run(() => gamesGpuSet(p.item.id, p.enabled));
  };

  const addGame = async () => {
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Programas", extensions: ["exe"] }],
    });
    if (typeof path === "string") void run(() => gamesAdd(path));
  };

  const dialog = () => {
    if (!pending) return null;
    if ("missing" in pending)
      return (
        <ConfirmDialog
          title={`Desfazer "${pending.title}"?`}
          confirmLabel="Desfazer"
          onConfirm={confirm}
          onCancel={() => setPending(null)}
        >
          <p>O jogo não foi encontrado; a preferência gravada volta ao que era antes.</p>
        </ConfirmDialog>
      );
    const { item, enabled } = pending;
    return enabled ? (
      <ConfirmDialog
        title={`Usar a GPU de alto desempenho em "${item.name}"?`}
        confirmLabel="Aplicar"
        onConfirm={confirm}
        onCancel={() => setPending(null)}
      >
        <p>{`${item.exe}: GpuPreference → 2 (alto desempenho)`}</p>
        <p>Vale na próxima vez que o jogo abrir.</p>
      </ConfirmDialog>
    ) : (
      <ConfirmDialog
        title={`Voltar "${item.name}" ao padrão do Windows?`}
        confirmLabel="Desfazer"
        onConfirm={confirm}
        onCancel={() => setPending(null)}
      >
        <p>A preferência volta ao que era antes do Sentinela.</p>
      </ConfirmDialog>
    );
  };

  return (
    <section aria-labelledby="gpu-title">
      <h2 id="gpu-title">GPU por jogo</h2>
      <p className="muted">
        Faz o Windows usar a placa de vídeo de alto desempenho no jogo, como em Configurações →
        Sistema → Tela → Gráficos.
      </p>
      {data && data.gpus <= 1 && <p className="notice">{ONE_GPU}</p>}
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
      {data && data.games.length === 0 && (
        <p className="muted">Nenhum jogo encontrado na Steam, na Epic ou na Riot.</p>
      )}
      {data && data.games.length > 0 && (
        <ul className="clean-list">
          {data.games.map((g) => (
            <li key={g.id} className="clean-row">
              <div className="clean-head">
                <label className="clean-check">
                  <input
                    type="checkbox"
                    role="switch"
                    aria-label={`Alto desempenho: ${g.name} (${fileName(g.exe)})`}
                    checked={g.enabled}
                    disabled={busy || g.external || g.blocked !== null}
                    onChange={() => setPending({ item: g, enabled: !g.enabled })}
                  />
                  <span>{g.name}</span>
                </label>
                <span className="muted">{SOURCE[g.source]}</span>
                {!g.exe_exists && <span className="tweak-badge">Arquivo não encontrado</span>}
                {g.source === "manual" && !g.enabled && (
                  <button
                    type="button"
                    className="btn"
                    disabled={busy}
                    onClick={() => void run(() => gamesRemove(g.exe))}
                  >
                    Remover da lista
                  </button>
                )}
              </div>
              <p className="muted clean-warning" title={g.exe}>
                {fileName(g.exe)}
              </p>
              {g.external && <p className="muted clean-warning">{EXTERNAL}</p>}
              {g.blocked && <p className="muted clean-warning">{g.blocked}</p>}
            </li>
          ))}
        </ul>
      )}
      {data && data.missing.length > 0 && (
        <ul className="clean-list">
          {data.missing.map((m) => (
            <li key={m.id} className="clean-row">
              <div className="clean-head">
                <span className="clean-check">{m.title}</span>
                <span className="tweak-badge">Jogo não encontrado</span>
                <button
                  type="button"
                  className="btn"
                  disabled={busy}
                  onClick={() => setPending({ missing: m.id, title: m.title })}
                >
                  Desfazer
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}
      <button type="button" className="btn" disabled={busy} onClick={() => void addGame()}>
        Adicionar jogo…
      </button>
      {dialog()}
    </section>
  );
}
