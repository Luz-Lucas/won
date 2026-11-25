import { useCallback, useEffect, useState } from "react";
import {
  wallpaperGallery,
  wallpaperMonitors,
  wallpaperOpen,
  wallpaperPauseModeSet,
} from "../api";
import type {
  WallpaperItem,
  WallpaperMonitor,
  WallpaperPauseMode,
  WallpaperStatus,
} from "../api";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

const PAUSE_OPTIONS: { mode: WallpaperPauseMode; label: string; hint: string }[] = [
  { mode: "pause", label: "Pausar", hint: "congela a imagem e mantém a memória" },
  { mode: "stop", label: "Parar", hint: "libera também a memória de vídeo" },
  { mode: "off", label: "Não mexer", hint: "o WON não toca no Wallpaper Engine" },
];

const monitorLabel = (m: WallpaperMonitor) =>
  `Monitor ${m.index + 1}${m.primary ? " (principal)" : ""}: ${m.width}×${m.height}`;

/** Wallpapers da Oficina e a pausa durante jogos. Só lê a Oficina; nunca grava nela. */
export default function WallpaperGallery({ status }: { status: WallpaperStatus }) {
  const [items, setItems] = useState<WallpaperItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [mode, setMode] = useState<WallpaperPauseMode>(status.pause_mode);
  const [busy, setBusy] = useState<string | null>(null);
  const [monitors, setMonitors] = useState<WallpaperMonitor[]>([]);
  // `null` = todos os monitores.
  const [target, setTarget] = useState<number | null>(null);

  const load = useCallback(() => {
    wallpaperGallery().then(
      (g) => {
        setItems(g);
        setError(null);
      },
      (e) => setError(errorText(e)),
    );
  }, []);

  useEffect(load, [load]);

  useEffect(() => {
    wallpaperMonitors().then(setMonitors, () => setMonitors([]));
  }, []);

  const use = async (id: string) => {
    setBusy(id);
    setError(null);
    try {
      await wallpaperOpen(id, target);
      load();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
    }
  };

  const changeMode = async (m: WallpaperPauseMode) => {
    setError(null);
    try {
      const s = await wallpaperPauseModeSet(m);
      setMode(s.pause_mode);
    } catch (e) {
      setError(errorText(e));
    }
  };

  return (
    <div>
      <p className="muted">
        {status.running
          ? "O Wallpaper Engine está aberto."
          : "O Wallpaper Engine está fechado; abra-o para ver a troca."}
      </p>
      <fieldset className="choice-group">
        <legend>Quando um jogo estiver aberto</legend>
        {PAUSE_OPTIONS.map((o) => (
          <label key={o.mode} className="clean-check">
            <input
              type="radio"
              name="wallpaper-pause"
              checked={mode === o.mode}
              onChange={() => void changeMode(o.mode)}
            />
            <span>{`${o.label} (${o.hint})`}</span>
          </label>
        ))}
        <p className="muted">O wallpaper volta sozinho quando o jogo fechar ou o WON for fechado.</p>
      </fieldset>
      {monitors.length > 1 && (
        <fieldset className="choice-group">
          <legend>Aplicar em</legend>
          <label className="clean-check">
            <input
              type="radio"
              name="wallpaper-monitor"
              checked={target === null}
              onChange={() => setTarget(null)}
            />
            <span>Todos os monitores</span>
          </label>
          {monitors.map((m) => (
            <label key={m.index} className="clean-check">
              <input
                type="radio"
                name="wallpaper-monitor"
                checked={target === m.index}
                onChange={() => setTarget(m.index)}
              />
              <span>{monitorLabel(m)}</span>
            </label>
          ))}
        </fieldset>
      )}
      {error && (
        <p role="alert" className="form-error">
          {error}
        </p>
      )}
      {!items && !error && <p className="muted">Carregando a Oficina…</p>}
      {items && items.length === 0 && (
        <p className="muted">Nenhum wallpaper assinado na Oficina ainda.</p>
      )}
      {items && items.length > 0 && (
        <ul className="wallpaper-grid">
          {items.map((w) => (
            <li key={w.id} className="wallpaper-card">
              {w.thumb ? (
                <img src={w.thumb} alt="" width={192} height={108} />
              ) : (
                <div className="wallpaper-placeholder" aria-hidden="true">
                  {w.title.slice(0, 1).toUpperCase()}
                </div>
              )}
              <span className="wallpaper-title">{w.title}</span>
              {w.kind && <span className="tweak-badge">{w.kind}</span>}
              {w.active && (
                <span className="tweak-badge">
                  {monitors.length > 1 && w.active_on.length > 0
                    ? `Em uso: ${w.active_on.map((n) => `monitor ${n + 1}`).join(", ")}`
                    : "Em uso"}
                </span>
              )}
              {!(target === null ? w.active && w.active_on.length >= Math.max(monitors.length, 1) : w.active_on.includes(target)) && (
                <button
                  type="button"
                  className="btn"
                  disabled={busy !== null}
                  onClick={() => void use(w.id)}
                  aria-label={`Usar ${w.title}`}
                >
                  Usar
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
