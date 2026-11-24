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
import WallpaperGallery from "../components/WallpaperGallery";
import { useJob } from "../jobs";

const AREAS: { id: TweakArea; label: string }[] = [
  { id: "barra", label: "Barra de tarefas" },
  { id: "iniciar", label: "Menu Iniciar" },
  { id: "explorador", label: "Explorador" },
  { id: "aparencia", label: "Aparência" },
  { id: "sistema", label: "Sistema" },
];

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

function Ajustes({ nav }: { nav: React.ReactNode }) {
  const [data, setData] = useState<TweaksPage | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [tab, setTab] = useState<TweakArea>("barra");
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
      <li key={t.id} className="clean-row">
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
          <label key={i.tweak.id} className="clean-check">
            <input
              type="radio"
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
      <h1>Personalizar</h1>
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
          <div className="tweak-profiles" role="tablist" aria-label="Áreas">
            {AREAS.map(({ id, label }) => (
              <button
                key={id}
                type="button"
                role="tab"
                className="btn"
                aria-selected={tab === id}
                onClick={() => setTab(id)}
              >
                {label}
              </button>
            ))}
          </div>
          <div role="tabpanel" aria-label={AREAS.find((a) => a.id === tab)?.label}>
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

type Section = "ajustes" | "wallpaper" | "temas" | "terminal" | "desktop";

/** Ajustes do Windows, papel de parede (só com o Wallpaper Engine) e temas. */
export default function Personalizar() {
  const [section, setSection] = useState<Section>("ajustes");
  const [engine, setEngine] = useState<WallpaperStatus | null>(null);
  // A seção do terminal só aparece quando há PowerShell ou Windows Terminal.
  const [term, setTerm] = useState<TerminalStatus | null>(null);

  useEffect(() => {
    wallpaperStatus().then(setEngine, () => setEngine(null));
  }, []);

  const [desk, setDesk] = useState<DesktopStatus | null>(null);
  const loadDesk = useCallback(() => {
    desktopStatus().then(setDesk, () => setDesk(null));
  }, []);
  useEffect(loadDesk, [loadDesk]);

  const loadTerm = useCallback(() => {
    terminalStatus().then(setTerm, () => setTerm(null));
  }, []);
  useEffect(loadTerm, [loadTerm]);

  const tabs: { id: Section; label: string }[] = [
    { id: "ajustes", label: "Ajustes" },
    ...(engine?.installed ? [{ id: "wallpaper" as const, label: "Papel de parede" }] : []),
    { id: "temas", label: "Temas" },
    ...(term && (term.profiles.length > 0 || term.windows_terminal)
      ? [{ id: "terminal" as const, label: "Terminal" }]
      : []),
    ...(desk?.installed ? [{ id: "desktop" as const, label: "Desktop" }] : []),
  ];
  // Se o Wallpaper Engine sumir com a aba aberta, volta para os ajustes.
  const current: Section = tabs.some((t) => t.id === section) ? section : "ajustes";

  const nav = (
    <nav aria-label="Seção" className="tweak-profiles">
      {tabs.map((t) => (
        <button
          key={t.id}
          type="button"
          className="btn"
          aria-current={current === t.id ? "page" : undefined}
          onClick={() => setSection(t.id)}
        >
          {t.label}
        </button>
      ))}
    </nav>
  );

  if (current === "ajustes") return <Ajustes nav={nav} />;
  return (
    <section className="page">
      <h1>Personalizar</h1>
      {nav}
      {current === "wallpaper" && engine && <WallpaperGallery status={engine} />}
      {current === "temas" && <Themes engine={engine?.installed ?? false} />}
      {current === "desktop" && desk && <Desktop status={desk} onChanged={loadDesk} />}
      {current === "terminal" && term && (
        <Terminal status={term} engine={engine?.installed ?? false} onChanged={loadTerm} />
      )}
    </section>
  );
}
