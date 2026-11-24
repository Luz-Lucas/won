import { useMemo } from "react";
import {
  DEFAULT_BAR,
  DEFAULT_LAUNCHER,
  DEFAULT_TILING,
  LAYOUTS,
  MODULES,
  applySections,
  readSections,
  tileBoxes,
  type BarModel,
  type Layout,
  type LauncherModel,
  type Module,
  type Sections,
  type TilingModel,
  type Zone,
} from "../desktopToml";

const MODULE_LABEL: Record<Module, string> = {
  title: "Título da janela",
  cpu: "CPU",
  ram: "Memória",
  clock: "Relógio",
};
const MODULE_SAMPLE: Record<Module, string> = {
  title: "Janela em foco",
  cpu: "CPU 12%",
  ram: "RAM 48%",
  clock: "14:32",
};
const ZONES: { id: Zone; label: string }[] = [
  { id: "left", label: "Esquerda" },
  { id: "center", label: "Centro" },
  { id: "right", label: "Direita" },
];
const CLOCKS: [string, string][] = [
  ["%H:%M", "14:32"],
  ["%H:%M:%S", "14:32:07"],
  ["%d/%m %H:%M", "24/11 14:32"],
  ["%d/%m/%Y %H:%M", "24/11/2026 14:32"],
];
const LAYOUT_LABEL: Record<Layout, string> = {
  "master-stack": "Principal e pilha",
  columns: "Colunas",
  grid: "Grade",
  monocle: "Uma janela por vez",
};
const HOTKEYS = ["Alt+Space", "Win+Space", "Ctrl+Alt+Space", "Win+Alt+Space", "Ctrl+Space"];

const zoneOf = (b: BarModel, m: Module): Zone | null =>
  b.left.includes(m) ? "left" : b.center.includes(m) ? "center" : b.right.includes(m) ? "right" : null;

function BarPreview({ bar }: { bar: BarModel }) {
  const clock = CLOCKS.find(([f]) => f === bar.clockFormat)?.[1] ?? "14:32";
  const sample = (m: Module) => (m === "clock" ? clock : MODULE_SAMPLE[m]);
  const zone = (z: Zone) => (
    <span className={`dv-zone dv-zone-${z}`}>
      {bar[z].map((m) => (
        <span key={m}>{sample(m)}</span>
      ))}
    </span>
  );
  return (
    <div className="sp-desk dv-preview" role="img" aria-label="Prévia da barra própria">
      <div
        className="dv-bar"
        data-edge={bar.edge}
        style={{
          height: `${(bar.height / 96) * 40 + 4}%`,
          background: bar.background,
          color: bar.foreground,
          fontFamily: bar.font,
          fontSize: `${Math.max(7, bar.fontSize * 0.7)}px`,
        }}
      >
        {zone("left")}
        {zone("center")}
        {zone("right")}
      </div>
      {!bar.hideNative && <div className="sp-taskbar dv-native">Barra nativa do Windows</div>}
      <div className="sp-window sp-window-small" style={{ top: bar.edge === "top" ? "24%" : "8%" }}>
        <div className="sp-titlebar">Aplicativo</div>
      </div>
    </div>
  );
}

function TilingPreview({ t }: { t: TilingModel }) {
  const W = 192;
  const H = 108;
  const boxes = tileBoxes(t, 4, W, H, 10);
  return (
    <div className="sp-desk dv-preview" role="img" aria-label="Prévia do tiling">
      <svg viewBox={`0 0 ${W} ${H}`} width="100%" height="100%" preserveAspectRatio="none">
        {boxes.map((b, i) => (
          <rect
            key={i}
            x={b.x}
            y={b.y}
            width={Math.max(b.w, 1)}
            height={Math.max(b.h, 1)}
            rx="2"
            fill="rgba(20,22,30,0.7)"
            stroke="var(--accent)"
            strokeWidth="1"
            opacity={t.layout === "monocle" ? 1 : 1}
          />
        ))}
      </svg>
      {t.layout === "monocle" && <span className="dv-note">4 janelas, uma visível por vez</span>}
    </div>
  );
}

function LauncherPreview({ l }: { l: LauncherModel }) {
  const names = ["Navegador", "Bloco de Notas", "Terminal", "Calculadora", "Configurações", "Explorador", "Paint", "Steam", "Discord", "Spotify", "VS Code", "Word", "Excel", "Fotos", "Mapas"];
  return (
    <div className="sp-desk dv-preview" role="img" aria-label="Prévia do lançador">
      <div className="dv-launcher">
        <div className="sp-search-field">{`Buscar apps, janelas e contas (${l.hotkey})`}</div>
        {names.slice(0, l.maxResults).map((n) => (
          <div key={n} className="dv-result">
            {n}
          </div>
        ))}
      </div>
    </div>
  );
}

function Range({
  label,
  value,
  min,
  max,
  step = 1,
  onChange,
  format,
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (v: number) => void;
  format?: (v: number) => string;
}) {
  return (
    <label className="dv-field">
      <span>{`${label}: ${format ? format(value) : value}`}</span>
      <input
        type="range"
        aria-label={label}
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
      />
    </label>
  );
}

/** Editor visual da barra própria, do tiling e do lançador, sobre o texto do desktop.toml. */
export default function DesktopVisual({
  text,
  onChange,
}: {
  text: string | null;
  onChange: (text: string) => void;
}) {
  const sections = useMemo(() => (text === null ? null : readSections(text)), [text]);
  if (text === null) return null;
  if (!sections)
    return (
      <p role="alert" className="form-error">
        O desktop.toml tem erro de sintaxe, então o editor visual está pausado. Corrija no
        arquivo (seção Avançado) e ele volta.
      </p>
    );
  const s: Sections = sections;
  const update = (patch: Partial<Sections>) => onChange(applySections(text, { ...s, ...patch }));
  const bar = s.bar;
  const setBar = (p: Partial<BarModel>) => bar && update({ bar: { ...bar, ...p } });
  const til = s.tiling;
  const setTil = (p: Partial<TilingModel>) => til && update({ tiling: { ...til, ...p } });
  const lau = s.launcher;
  const setLau = (p: Partial<LauncherModel>) => lau && update({ launcher: { ...lau, ...p } });

  const moveModule = (m: Module, to: Zone | "off") => {
    if (!bar) return;
    const strip = (a: Module[]) => a.filter((x) => x !== m);
    const next = { left: strip(bar.left), center: strip(bar.center), right: strip(bar.right) };
    if (to !== "off") next[to] = MODULES.filter((x) => x === m || next[to].includes(x));
    setBar(next);
  };

  return (
    <div className="dv">
      <article className="card dv-card" aria-labelledby="dv-bar-title">
        <header className="dv-head">
          <h2 id="dv-bar-title">Barra própria</h2>
          <label className="clean-check">
            <input
              type="checkbox"
              role="switch"
              aria-label="Usar a barra própria"
              checked={bar !== null}
              onChange={(e) => update({ bar: e.target.checked ? { ...DEFAULT_BAR } : null })}
            />
            <span>{bar ? "Ligada" : "Desligada"}</span>
          </label>
        </header>
        <p className="muted">
          Uma barra no topo ou embaixo da tela com título da janela, CPU, memória e relógio.
        </p>
        {bar && (
          <div className="dv-body">
            <BarPreview bar={bar} />
            <div className="dv-controls">
              <fieldset className="choice-group">
                <legend>Posição</legend>
                {(["top", "bottom"] as const).map((e) => (
                  <label key={e} className="clean-check">
                    <input type="radio" name="bar-edge" checked={bar.edge === e} onChange={() => setBar({ edge: e })} />
                    <span>{e === "top" ? "Topo" : "Embaixo"}</span>
                  </label>
                ))}
              </fieldset>
              <Range label="Altura da barra" value={bar.height} min={20} max={96} onChange={(v) => setBar({ height: v })} format={(v) => `${v} px`} />
              <Range label="Tamanho da fonte" value={bar.fontSize} min={8} max={32} onChange={(v) => setBar({ fontSize: v })} format={(v) => `${v} pt`} />
              <div className="dv-colors">
                <label>
                  Fundo{" "}
                  <input type="color" aria-label="Cor de fundo" value={bar.background} onChange={(e) => setBar({ background: e.target.value.toUpperCase() })} />
                </label>
                <label>
                  Texto{" "}
                  <input type="color" aria-label="Cor do texto" value={bar.foreground} onChange={(e) => setBar({ foreground: e.target.value.toUpperCase() })} />
                </label>
              </div>
              <label className="dv-field">
                <span>Formato do relógio</span>
                <select
                  aria-label="Formato do relógio"
                  value={bar.clockFormat}
                  onChange={(e) => setBar({ clockFormat: e.target.value })}
                >
                  {!CLOCKS.some(([f]) => f === bar.clockFormat) && (
                    <option value={bar.clockFormat}>{`Personalizado (${bar.clockFormat})`}</option>
                  )}
                  {CLOCKS.map(([f, ex]) => (
                    <option key={f} value={f}>
                      {ex}
                    </option>
                  ))}
                </select>
              </label>
              <label className="clean-check">
                <input
                  type="checkbox"
                  checked={bar.hideNative}
                  onChange={(e) => setBar({ hideNative: e.target.checked })}
                />
                <span>Ocultar a barra de tarefas do Windows</span>
              </label>
              <fieldset className="choice-group">
                <legend>Módulos</legend>
                {MODULES.map((m) => (
                  <label key={m} className="dv-field dv-inline">
                    <span>{MODULE_LABEL[m]}</span>
                    <select
                      aria-label={`Posição de ${MODULE_LABEL[m]}`}
                      value={zoneOf(bar, m) ?? "off"}
                      onChange={(e) => moveModule(m, e.target.value as Zone | "off")}
                    >
                      <option value="off">Não mostrar</option>
                      {ZONES.map((z) => (
                        <option key={z.id} value={z.id}>
                          {z.label}
                        </option>
                      ))}
                    </select>
                  </label>
                ))}
              </fieldset>
            </div>
          </div>
        )}
      </article>

      <article className="card dv-card" aria-labelledby="dv-til-title">
        <header className="dv-head">
          <h2 id="dv-til-title">Tiling de janelas</h2>
          <label className="clean-check">
            <input
              type="checkbox"
              role="switch"
              aria-label="Usar o tiling"
              checked={til !== null}
              onChange={(e) => update({ tiling: e.target.checked ? { ...DEFAULT_TILING } : null })}
            />
            <span>{til ? "Ligado" : "Desligado"}</span>
          </label>
        </header>
        <p className="muted">Organiza as janelas lado a lado, sem sobreposição.</p>
        {til && (
          <div className="dv-body">
            <TilingPreview t={til} />
            <div className="dv-controls">
              <fieldset className="choice-group">
                <legend>Disposição</legend>
                {LAYOUTS.map((l) => (
                  <label key={l} className="clean-check">
                    <input type="radio" name="til-layout" checked={til.layout === l} onChange={() => setTil({ layout: l })} />
                    <span>{LAYOUT_LABEL[l]}</span>
                  </label>
                ))}
              </fieldset>
              <Range label="Espaço entre janelas" value={til.gap} min={0} max={100} onChange={(v) => setTil({ gap: v })} format={(v) => `${v} px`} />
              <Range label="Espaço na borda da tela" value={til.outerGap} min={0} max={100} onChange={(v) => setTil({ outerGap: v })} format={(v) => `${v} px`} />
              {til.layout === "master-stack" && (
                <Range
                  label="Largura da janela principal"
                  value={til.masterRatio}
                  min={0.2}
                  max={0.8}
                  step={0.05}
                  onChange={(v) => setTil({ masterRatio: Math.round(v * 100) / 100 })}
                  format={(v) => `${Math.round(v * 100)}%`}
                />
              )}
            </div>
          </div>
        )}
      </article>

      <article className="card dv-card" aria-labelledby="dv-lau-title">
        <header className="dv-head">
          <h2 id="dv-lau-title">Lançador</h2>
          <label className="clean-check">
            <input
              type="checkbox"
              role="switch"
              aria-label="Usar o lançador"
              checked={lau !== null}
              onChange={(e) => update({ launcher: e.target.checked ? { ...DEFAULT_LAUNCHER } : null })}
            />
            <span>{lau ? "Ligado" : "Desligado"}</span>
          </label>
        </header>
        <p className="muted">Uma caixa de busca para abrir apps e janelas e fazer contas rápidas.</p>
        {lau && (
          <div className="dv-body">
            <LauncherPreview l={lau} />
            <div className="dv-controls">
              <label className="dv-field">
                <span>Atalho para abrir</span>
                <select
                  aria-label="Atalho do lançador"
                  value={lau.hotkey}
                  onChange={(e) => setLau({ hotkey: e.target.value })}
                >
                  {!HOTKEYS.includes(lau.hotkey) && <option value={lau.hotkey}>{lau.hotkey}</option>}
                  {HOTKEYS.map((h) => (
                    <option key={h} value={h}>
                      {h}
                    </option>
                  ))}
                </select>
              </label>
              <Range label="Resultados na lista" value={lau.maxResults} min={3} max={15} onChange={(v) => setLau({ maxResults: v })} />
              {lau.apps.length > 0 && (
                <p className="muted">{`${lau.apps.length} app(s) extra(s) definidos no arquivo são mantidos.`}</p>
              )}
            </div>
          </div>
        )}
      </article>
    </div>
  );
}
