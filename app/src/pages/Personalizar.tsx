import ShellMods from "../components/ShellMods";
import { useCallback, useEffect, useState } from "react";
import {
  relaunchElevated,
  wallpaperStatus,
  restartExplorer,
  tweaksApply,
  tweaksApplyChoice,
  tweaksList,
  tweaksUndo,
  terminalStatus,
  desktopStatus,
} from "../api";
import type {
  ApplyReport,
  DesktopStatus,
  TweakArea,
  TweakEffect,
  TweakItem,
  TerminalStatus,
  TweaksPage,
  UndoReport,
  WallpaperStatus,
} from "../api";
import ConfirmDialog from "../components/ConfirmDialog";
import Desktop from "../components/Desktop";
import Terminal from "../components/Terminal";
import Themes from "../components/Themes";
import StylePreview, { effectiveOn } from "../components/StylePreview";
import type { Page } from "../components/Sidebar";
import WallpaperGallery from "../components/WallpaperGallery";
import { AREA_INFO } from "../components/styleAreas";
import { useJob } from "../jobs";

/** Ajustes do grupo Visual que a aba Aparência também mostra. */
const VISUAL_IN_APPEARANCE = ["transparency-off", "animations-off"];

const EFFECT_NOTICE: Partial<Record<TweakEffect, string>> = {
  reboot: "Reinicie o computador para concluir.",
  relogon: "Saia e entre de novo no Windows para concluir.",
};

const EXPLORER_NOTICE = "Reinicie o Explorer para ver a mudança.";
const EXTERNAL = "Configurado fora do Sentinela.";
const ADMIN = "requer administrador";

const OTHER_ACCOUNT =
  "Esta janela está em outra conta do Windows. O histórico e os ajustes do usuário mostrados aqui são dessa conta.";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

const areaOf = (i: TweakItem): TweakArea | null => {
  if (i.tweak.group === "personalizacao") return i.tweak.area ?? null;
  return VISUAL_IN_APPEARANCE.includes(i.tweak.id) ? "aparencia" : null;
};

const applied = (i: TweakItem) => i.view.status === "applied";

/** Pode ser mexido agora (sem bloqueio e disponível nesta máquina). */
const usable = (i: TweakItem) =>
  i.view.blocked === null && i.view.status !== "unavailable";

type Row =
  | { kind: "switch"; item: TweakItem }
  | { kind: "choice"; id: string; label: string; options: TweakItem[] };

/** Linhas de uma aba na ordem do catálogo; cada lista de opções vira uma linha. */
const rowsOf = (items: TweakItem[]): Row[] => {
  const rows: Row[] = [];
  for (const i of items) {
    const choice = i.tweak.choice;
    if (!choice) {
      rows.push({ kind: "switch", item: i });
      continue;
    }
    const row = rows.find((r) => r.kind === "choice" && r.id === choice);
    if (row && row.kind === "choice") row.options.push(i);
    else
      rows.push({
        kind: "choice",
        id: choice,
        label: i.tweak.choice_label ?? choice,
        options: [i],
      });
  }
  return rows;
};

type Pending =
  | { kind: "on"; item: TweakItem }
  | { kind: "off"; item: TweakItem }
  | { kind: "choice"; item: TweakItem; label: string };

const EFFECT_ORDER: TweakEffect[] = ["immediate", "explorer", "relogon", "reboot"];

function Ajustes({ nav, area }: { nav: React.ReactNode; area: TweakArea }) {
  const [data, setData] = useState<TweaksPage | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  // A opção sob o cursor ou o foco: a prévia mostra como ficaria.
  const [hover, setHover] = useState<string | null>(null);
  const tab = area;
  const [pending, setPending] = useState<Pending | null>(null);
  const [relaunchError, setRelaunchError] = useState<string | null>(null);
  // O job "tweaks" é compartilhado com Jogos e Histórico: só mostra o
  // resultado de uma ação iniciada nesta página, com o efeito do ajuste.
  const [started, setStarted] = useState<{ effect: TweakEffect } | null>(null);
  const [askRestart, setAskRestart] = useState(false);
  const [restartState, setRestartState] = useState<
    { kind: "idle" } | { kind: "running" } | { kind: "done" } | { kind: "error"; message: string }
  >({ kind: "idle" });
  const job = useJob<ApplyReport | UndoReport>("tweaks");

  const load = useCallback(() => {
    setLoadError(null);
    tweaksList().then(setData, (e) => setLoadError(errorText(e)));
  }, []);

  useEffect(load, [load]);

  useEffect(() => {
    if (job.state === "done" && job.result) load();
  }, [job.state, job.result, load]);

  const running = job.state === "running";
  const items = (data?.tweaks ?? []).filter((i) => areaOf(i) !== null);
  const elevated = data?.elevated ?? false;
  const bannerNeeded =
    data !== null && !elevated && items.some((i) => i.tweak.needs_admin);

  const report = started && job.state === "done" && job.result ? job.result : null;
  const changed = report?.results.some((r) => r.ok && r.changed) ?? false;
  // O apply informa o efeito mais forte; o desfazer usa o do próprio ajuste.
  const effect: TweakEffect | null = !report
    ? null
    : "effect" in report
      ? EFFECT_ORDER[
          Math.max(
            EFFECT_ORDER.indexOf(report.effect),
            EFFECT_ORDER.indexOf(started!.effect),
          )
        ]
      : started!.effect;
  const showExplorer = changed && effect === "explorer";

  const titleOf = (id: string) =>
    items.find((i) => i.tweak.id === id)?.tweak.title ?? id;

  const relaunch = async () => {
    setRelaunchError(null);
    try {
      await relaunchElevated();
    } catch (e) {
      setRelaunchError(errorText(e));
    }
  };

  const confirm = () => {
    const p = pending;
    setPending(null);
    if (!p) return;
    const id = p.item.tweak.id;
    setStarted({ effect: p.item.tweak.effect });
    setRestartState({ kind: "idle" });
    if (p.kind === "on") void job.start(() => tweaksApply([id]));
    else if (p.kind === "off") void job.start(() => tweaksUndo([id]));
    else void job.start(() => tweaksApplyChoice(id));
  };

  const restart = async () => {
    setAskRestart(false);
    setRestartState({ kind: "running" });
    try {
      await restartExplorer();
      setRestartState({ kind: "done" });
    } catch (e) {
      setRestartState({ kind: "error", message: errorText(e) });
    }
  };

  const badges = (i: TweakItem) => (
    <>
      {i.tweak.needs_admin && <span className="tweak-badge">Requer administrador</span>}
      {i.tweak.effect === "explorer" && (
        <span className="tweak-badge">Reiniciar o Explorer</span>
      )}
      {i.tweak.effect === "relogon" && (
        <span className="tweak-badge">Vale após sair e entrar</span>
      )}
      {i.tweak.effect === "reboot" && <span className="tweak-badge">Exige reinício</span>}
    </>
  );

  const blockedNote = (i: TweakItem) =>
    i.view.blocked && i.view.blocked !== ADMIN ? (
      <p className="muted clean-warning">{i.view.blocked}</p>
    ) : null;

  const renderSwitch = (i: TweakItem) => {
    const t = i.tweak;
    const external = applied(i) && !i.in_journal;
    const disabled = running || !usable(i) || external;
    return (
      <li
        key={t.id}
        className="clean-row"
        onMouseEnter={() => setHover(t.id)}
        onMouseLeave={() => setHover(null)}
        onFocus={() => setHover(t.id)}
        onBlur={() => setHover(null)}
      >
        <div className="clean-head">
          <label className="clean-check">
            <input
              type="checkbox"
              role="switch"
              checked={applied(i)}
              disabled={disabled}
              onChange={() =>
                setPending({ kind: applied(i) ? "off" : "on", item: i })
              }
            />
            <span>{t.title}</span>
          </label>
          {applied(i) && <span className="tweak-badge">Aplicado</span>}
          {badges(i)}
        </div>
        {external && <p className="muted clean-warning">{EXTERNAL}</p>}
        {blockedNote(i)}
      </li>
    );
  };

  const renderChoice = (row: Extract<Row, { kind: "choice" }>) => (
    <li key={row.id} className="clean-row">
      <fieldset className="choice-group">
        <legend>{row.label}</legend>
        {row.options.map((i) => (
          <label
            key={i.tweak.id}
            className="clean-check"
            onMouseEnter={() => setHover(i.tweak.id)}
            onMouseLeave={() => setHover(null)}
          >
            <input
              type="radio"
              onFocus={() => setHover(i.tweak.id)}
              onBlur={() => setHover(null)}
              name={row.id}
              checked={applied(i)}
              disabled={running || !usable(i)}
              onChange={() => {
                if (!applied(i)) setPending({ kind: "choice", item: i, label: row.label });
              }}
            />
            <span>{i.tweak.title}</span>
          </label>
        ))}
      </fieldset>
      {row.options.some(applied) && <span className="tweak-badge">Aplicado</span>}
      {row.options[0] && badges(row.options[0])}
      {row.options.map((i) => (
        <div key={i.tweak.id}>{blockedNote(i)}</div>
      ))}
    </li>
  );

  const dialog = () => {
    if (!pending) return null;
    const t = pending.item.tweak;
    const preview = (
      <ul>
        {pending.item.view.changes.map((c, n) => (
          <li key={`${c.description}-${n}`}>{`${c.description}: ${c.current} → ${c.target}`}</li>
        ))}
      </ul>
    );
    if (pending.kind === "off")
      return (
        <ConfirmDialog
          title={`Desfazer "${t.title}"?`}
          confirmLabel="Desfazer"
          onConfirm={confirm}
          onCancel={() => setPending(null)}
        >
          <p>Volta ao valor que estava antes do Sentinela.</p>
        </ConfirmDialog>
      );
    return (
      <ConfirmDialog
        title={
          pending.kind === "choice"
            ? `${pending.label}: ${t.title}?`
            : `Aplicar "${t.title}"?`
        }
        confirmLabel="Aplicar"
        onConfirm={confirm}
        onCancel={() => setPending(null)}
      >
        {preview}
        {t.effect === "explorer" && <p>{EXPLORER_NOTICE}</p>}
      </ConfirmDialog>
    );
  };

  const tabItems = items.filter((i) => areaOf(i) === tab);

  return (
    <section className="page">
      {nav}
      {data && !data.hkcu_allowed && <p className="notice">{OTHER_ACCOUNT}</p>}
      {bannerNeeded && (
        <div className="notice">
          <p>Algumas personalizações exigem administrador.</p>
          <button type="button" className="btn" onClick={() => void relaunch()}>
            Reiniciar como administrador
          </button>
        </div>
      )}
      {relaunchError && (
        <p role="alert" className="form-error">
          {relaunchError}
        </p>
      )}
      {loadError && (
        <div>
          <p role="alert" className="form-error">
            Não foi possível carregar as personalizações: {loadError}
          </p>
          <button type="button" className="btn" onClick={load}>
            Tentar de novo
          </button>
        </div>
      )}
      {started && job.error && (
        <p role="alert" className="form-error">
          Não foi possível concluir: {job.error}
        </p>
      )}
      {!data && !loadError && <p className="muted">Carregando…</p>}
      {running && <p className="muted">Aplicando…</p>}
      {report && (
        <div className="notice">
          {report.notice && <p>{report.notice}</p>}
          <ul className="tweak-results">
            {report.results
              .filter((r) => !r.ok)
              .map((r) => (
                <li key={r.tweak_id} className="form-error">
                  {`${titleOf(r.tweak_id)}: ${r.message}`}
                </li>
              ))}
          </ul>
          {"conflicts" in report &&
            report.conflicts.map((c) => <p key={c}>{c}</p>)}
          {changed && effect && EFFECT_NOTICE[effect] && <p>{EFFECT_NOTICE[effect]}</p>}
          {showExplorer && restartState.kind !== "done" && (
            <div>
              <p>{EXPLORER_NOTICE}</p>
              <button
                type="button"
                className="btn"
                disabled={restartState.kind === "running"}
                onClick={() => setAskRestart(true)}
              >
                Reiniciar Explorer
              </button>
            </div>
          )}
          {restartState.kind === "done" && <p>Explorer reiniciado.</p>}
          {restartState.kind === "error" && (
            <p role="alert" className="form-error">
              {restartState.message}
            </p>
          )}
          {"effect" in report && report.restore_point !== null && (
            <p>{`Ponto de restauração: ${report.restore_point}`}</p>
          )}
        </div>
      )}
      {data && (
        <>
          <div className="style-preview" aria-label="Como vai ficar">
            <h2>Como vai ficar</h2>
            <StylePreview
              area={tab}
              on={effectiveOn(tabItems, hover)}
              items={tabItems}
            />
            <p className="muted">
              Ilustração do que esta área muda. Passe o mouse ou o foco sobre uma opção para ver
              o resultado antes de aplicar.
            </p>
          </div>
          <div role="group" aria-label={AREA_INFO[tab].title}>
            {tabItems.length === 0 && <p className="muted">Nada nesta área.</p>}
            <ul className="clean-list">
              {rowsOf(tabItems).map((row) =>
                row.kind === "switch" ? renderSwitch(row.item) : renderChoice(row),
              )}
            </ul>
          </div>
          <p className="muted tweak-footnote">
            Tudo o que o Sentinela muda aqui pode ser desfeito pelo Histórico.
          </p>
        </>
      )}
      {dialog()}
      {askRestart && (
        <ConfirmDialog
          title="Reiniciar o Explorer? As janelas de pastas abertas serão fechadas."
          confirmLabel="Reiniciar"
          onConfirm={() => void restart()}
          onCancel={() => setAskRestart(false)}
        >
          <p>A barra de tarefas some por alguns segundos e volta.</p>
        </ConfirmDialog>
      )}
    </section>
  );
}

type View = "hub" | TweakArea | "wallpaper" | "temas" | "terminal" | "desktop";

/** Estilização: um menu de áreas; cada uma com referência, prévia e seus controles. */
export default function Personalizar({ onNavigate }: { onNavigate?: (page: Page) => void }) {
  const [view, setView] = useState<View>("hub");
  const [engine, setEngine] = useState<WallpaperStatus | null>(null);
  const [term, setTerm] = useState<TerminalStatus | null>(null);
  const [desk, setDesk] = useState<DesktopStatus | null>(null);

  useEffect(() => {
    wallpaperStatus().then(setEngine, () => setEngine(null));
  }, []);
  const loadDesk = useCallback(() => {
    desktopStatus().then(setDesk, () => setDesk(null));
  }, []);
  useEffect(loadDesk, [loadDesk]);
  const loadTerm = useCallback(() => {
    terminalStatus().then(setTerm, () => setTerm(null));
  }, []);
  useEffect(loadTerm, [loadTerm]);

  /** Motivo de a área não poder abrir agora; `null` = disponível. */
  const unavailable = (id: View): string | null => {
    if (id === "wallpaper") return engine?.installed ? null : "Requer o Wallpaper Engine instalado.";
    if (id === "terminal")
      return term && (term.profiles.length > 0 || term.windows_terminal)
        ? null
        : "Nenhum PowerShell ou Windows Terminal encontrado.";
    if (id === "desktop") return desk?.installed ? null : "Componente de desktop não instalado.";
    return null;
  };

  const info = view === "hub" ? null : AREA_INFO[view];
  const back = (
    <div className="style-head">
      <button type="button" className="btn" onClick={() => setView("hub")}>
        ← Todas as áreas
      </button>
      {info && (
        <>
          <h2>{info.title}</h2>
          <p className="muted">{info.description}</p>
          {(view === "barra" || view === "iniciar") && (
            <ShellMods only={view === "iniciar" ? ["xaml"] : ["xaml", "icones", "rotulos"]} />
          )}
          <details className="style-reference">
            <summary>Referências: o que esta área muda</summary>
            <ul>
              {info.reference.map((r) => (
                <li key={r}>{r}</li>
              ))}
            </ul>
          </details>
        </>
      )}
    </div>
  );

  if (view === "hub")
    return (
      <section className="page">
        <h1>Estilização</h1>
        <p className="muted">
          Escolha o elemento do desktop que quer mudar. Cada área mostra uma referência e uma
          prévia de como ele vai ficar, e tudo pode ser desfeito pelo Histórico.
        </p>
        <ul className="area-grid" aria-label="Áreas">
          {HUB_ORDER.map((id) => {
            const a = AREA_INFO[id];
            const reason = unavailable(id);
            return (
              <li key={id}>
                <button
                  type="button"
                  className="area-card"
                  aria-label={a.title}
                  aria-describedby={`area-${id}-desc`}
                  disabled={reason !== null}
                  onClick={() => setView(id)}
                >
                  <span className="area-thumb" aria-hidden="true">
                    <StylePreview area={id} on={new Set()} />
                  </span>
                  <strong>{a.title}</strong>
                  <span id={`area-${id}-desc`} className="muted">
                    {reason ?? a.summary}
                  </span>
                </button>
              </li>
            );
          })}
          {onNavigate && (
            <li>
              <button type="button" className="area-card" onClick={() => onNavigate("mods")}>
                <span className="area-thumb area-thumb-text" aria-hidden="true">
                  ⚙
                </span>
                <strong>Mods avançados</strong>
                <span className="muted">
                  Ícones pequenos, rótulos e outros ajustes profundos do Explorer.
                </span>
              </button>
            </li>
          )}
        </ul>
      </section>
    );

  const isTweakArea = (v: View): v is TweakArea =>
    v === "barra" || v === "iniciar" || v === "explorador" || v === "aparencia" || v === "sistema";

  if (isTweakArea(view)) return <Ajustes nav={back} area={view} />;

  return (
    <section className="page">
      {back}
      <div className="style-preview" aria-label="Como vai ficar">
        <StylePreview area={view} on={new Set()} />
      </div>
      {view === "wallpaper" && engine && <WallpaperGallery status={engine} />}
      {view === "temas" && <Themes engine={engine?.installed ?? false} />}
      {view === "desktop" && desk && <Desktop status={desk} onChanged={loadDesk} />}
      {view === "terminal" && term && (
        <Terminal status={term} engine={engine?.installed ?? false} onChanged={loadTerm} />
      )}
    </section>
  );
}

const HUB_ORDER: Exclude<View, "hub">[] = [
  "barra",
  "iniciar",
  "explorador",
  "aparencia",
  "temas",
  "wallpaper",
  "terminal",
  "desktop",
  "sistema",
];
