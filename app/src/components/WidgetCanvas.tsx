import { useEffect, useRef, useState } from "react";
import { WIDGET_KINDS, WIDGET_NAMES, newWidget } from "../desktopToml";
import type { WidgetKind, WidgetModel } from "../desktopToml";

/** Grade de encaixe (px reais da tela) para mover e redimensionar. */
export const MOVE_GRID = 20;
export const SIZE_GRID = 10;

export const snap = (v: number, grid: number) => Math.round(v / grid) * grid || 0;

type SizeName = "P" | "M" | "G";
export const SIZES: Record<"media" | "other", Record<SizeName, [number, number]>> = {
  other: { P: [140, 90], M: [200, 120], G: [300, 180] },
  media: { P: [240, 80], M: [320, 100], G: [440, 140] },
};
export const sizeOf = (k: WidgetKind, s: SizeName): [number, number] =>
  SIZES[k === "media" ? "media" : "other"][s];

const SWATCHES: { name: string; bg: string; fg: string }[] = [
  { name: "Noite", bg: "#14161A", fg: "#FFFFFF" },
  { name: "Azul WON", bg: "#0B1A3A", fg: "#6EBEFF" },
  { name: "Grafite", bg: "#2A2D33", fg: "#E6E8EB" },
  { name: "Claro", bg: "#F2F4F7", fg: "#14161A" },
  { name: "Roxo", bg: "#2A1A4A", fg: "#D9C2FF" },
  { name: "Verde", bg: "#0F2E22", fg: "#8FF0C0" },
  { name: "Vermelho", bg: "#3A1216", fg: "#FFB3B8" },
];

const RADII: { name: string; v: number }[] = [
  { name: "Reto", v: 0 },
  { name: "Suave", v: 16 },
  { name: "Redondo", v: 36 },
];

function sample(w: WidgetModel): { label: string; value: string } {
  const now = new Date();
  const p = (n: number, l = 2) => String(n).padStart(l, "0");
  switch (w.kind) {
    case "clock": {
      const value = w.format
        .replace(/%H/g, p(now.getHours()))
        .replace(/%M/g, p(now.getMinutes()))
        .replace(/%S/g, p(now.getSeconds()))
        .replace(/%d/g, p(now.getDate()))
        .replace(/%m/g, p(now.getMonth() + 1))
        .replace(/%Y/g, p(now.getFullYear(), 4))
        .replace(/%y/g, p(now.getFullYear() % 100));
      return { label: `${p(now.getDate())}/${p(now.getMonth() + 1)}/${now.getFullYear()}`, value };
    }
    case "cpu":
      return { label: "CPU", value: "23%" };
    case "ram":
      return { label: "RAM", value: "58%" };
    case "network":
      return { label: "REDE", value: "Wi-Fi" };
    case "media":
      return { label: "TOCANDO", value: "Artista - Música" };
  }
}

type Drag =
  | { mode: "move"; id: string; px: number; py: number; x: number; y: number }
  | { mode: "size"; id: string; px: number; py: number; w: number; h: number };

/** Editor visual dos widgets: miniatura da tela com os blocos para arrastar e redimensionar. */
export default function WidgetCanvas({
  widgets,
  onChange,
}: {
  widgets: WidgetModel[];
  onChange: (w: WidgetModel[]) => void;
}) {
  const screenW = Math.max(800, Math.round(window.screen.width * (window.devicePixelRatio || 1)));
  const screenH = Math.max(450, Math.round(window.screen.height * (window.devicePixelRatio || 1)));
  const boxRef = useRef<HTMLDivElement>(null);
  const [boxW, setBoxW] = useState(720);
  const [sel, setSel] = useState<string | null>(widgets[0]?.id ?? null);
  const drag = useRef<Drag | null>(null);

  useEffect(() => {
    const el = boxRef.current;
    if (!el) return;
    const measure = () => el.clientWidth > 0 && setBoxW(el.clientWidth);
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, []);

  const scale = boxW / screenW;
  const selected = widgets.find((w) => w.id === sel) ?? null;
  const patch = (id: string, p: Partial<WidgetModel>) =>
    onChange(widgets.map((w) => (w.id === id ? { ...w, ...p } : w)));

  const onMove = (e: React.PointerEvent) => {
    const d = drag.current;
    if (!d) return;
    const dx = (e.clientX - d.px) / scale;
    const dy = (e.clientY - d.py) / scale;
    if (d.mode === "move") {
      patch(d.id, {
        x: Math.max(0, Math.min(screenW - 60, snap(d.x + dx, MOVE_GRID))),
        y: Math.max(0, Math.min(screenH - 60, snap(d.y + dy, MOVE_GRID))),
      });
    } else {
      patch(d.id, {
        width: Math.max(60, Math.min(800, snap(d.w + dx, SIZE_GRID))),
        height: Math.max(60, Math.min(800, snap(d.h + dy, SIZE_GRID))),
      });
    }
  };
  const end = () => {
    drag.current = null;
  };

  return (
    <div className="wc">
      <div className="wc-add" role="group" aria-label="Adicionar widget">
        {WIDGET_KINDS.map((k) => (
          <button
            key={k}
            type="button"
            className="btn"
            disabled={widgets.length >= 12}
            onClick={() => {
              const w = newWidget(k, widgets);
              const [ww, hh] = sizeOf(k, "M");
              onChange([...widgets, { ...w, width: ww, height: hh }]);
              setSel(w.id);
            }}
          >
            + {WIDGET_NAMES[k]}
          </button>
        ))}
      </div>

      <div
        ref={boxRef}
        className="wc-screen"
        style={{ aspectRatio: `${screenW} / ${screenH}` }}
        onPointerMove={onMove}
        onPointerUp={end}
        onPointerCancel={end}
        aria-label="Pré-visualização da área de trabalho"
      >
        {widgets.length === 0 && (
          <p className="wc-empty">Adicione um widget acima e arraste para onde quiser.</p>
        )}
        {widgets.map((w) => {
          const s = sample(w);
          return (
            <div
              key={w.id}
              className={w.id === sel ? "wc-block selected" : "wc-block"}
              data-testid={`widget-${w.id}`}
              style={{
                left: w.x * scale,
                top: w.y * scale,
                width: w.width * scale,
                height: w.height * scale,
                background: w.background,
                color: w.foreground,
                borderRadius: w.radius * scale,
              }}
              onPointerDown={(e) => {
                setSel(w.id);
                (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
                drag.current = { mode: "move", id: w.id, px: e.clientX, py: e.clientY, x: w.x, y: w.y };
              }}
            >
              <span className="wc-label" style={{ fontSize: Math.max(8, (w.fontSize / 3) * scale) }}>
                {s.label}
              </span>
              <span className="wc-value" style={{ fontSize: Math.max(9, w.fontSize * scale) }}>
                {s.value}
              </span>
              {w.id === sel && (
                <span
                  className="wc-handle"
                  role="presentation"
                  onPointerDown={(e) => {
                    e.stopPropagation();
                    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
                    drag.current = { mode: "size", id: w.id, px: e.clientX, py: e.clientY, w: w.width, h: w.height };
                  }}
                />
              )}
            </div>
          );
        })}
      </div>
      <p className="muted">
        Arraste o bloco para mover (encaixa numa grade) e puxe o canto azul para mudar o tamanho.
        A miniatura mostra como fica na sua tela.
      </p>

      {selected && (
        <div className="wc-panel" aria-label={`Editar ${WIDGET_NAMES[selected.kind]}`}>
          <h3>{WIDGET_NAMES[selected.kind]}</h3>
          <div className="wc-row">
            <span>Tamanho</span>
            {(["P", "M", "G"] as SizeName[]).map((n) => (
              <button
                key={n}
                type="button"
                className="btn"
                aria-label={`Tamanho ${n}`}
                onClick={() => {
                  const [ww, hh] = sizeOf(selected.kind, n);
                  patch(selected.id, { width: ww, height: hh });
                }}
              >
                {n}
              </button>
            ))}
          </div>
          <div className="wc-row">
            <span>Cores</span>
            {SWATCHES.map((c) => (
              <button
                key={c.name}
                type="button"
                className="wc-swatch"
                aria-label={`Cor ${c.name}`}
                title={c.name}
                style={{ background: c.bg, color: c.fg }}
                onClick={() => patch(selected.id, { background: c.bg, foreground: c.fg })}
              >
                A
              </button>
            ))}
            <label>
              Fundo{" "}
              <input type="color" aria-label="Fundo personalizado" value={selected.background} onChange={(e) => patch(selected.id, { background: e.target.value.toUpperCase() })} />
            </label>
            <label>
              Texto{" "}
              <input type="color" aria-label="Texto personalizado" value={selected.foreground} onChange={(e) => patch(selected.id, { foreground: e.target.value.toUpperCase() })} />
            </label>
          </div>
          <div className="wc-row">
            <span>Cantos</span>
            {RADII.map((r) => (
              <button key={r.name} type="button" className="btn" onClick={() => patch(selected.id, { radius: r.v })}>
                {r.name}
              </button>
            ))}
            <span>Texto</span>
            <button type="button" className="btn" aria-label="Texto menor" onClick={() => patch(selected.id, { fontSize: Math.max(10, selected.fontSize - 4) })}>A−</button>
            <button type="button" className="btn" aria-label="Texto maior" onClick={() => patch(selected.id, { fontSize: Math.min(72, selected.fontSize + 4) })}>A+</button>
          </div>
          {selected.kind === "clock" && (
            <div className="wc-row">
              <span>Hora</span>
              {[
                ["24 h", "%H:%M"],
                ["Com segundos", "%H:%M:%S"],
              ].map(([n, f]) => (
                <button key={f} type="button" className="btn" onClick={() => patch(selected.id, { format: f! })}>
                  {n}
                </button>
              ))}
            </div>
          )}
          <div className="wc-row">
            <label className="clean-check">
              <input type="checkbox" checked={selected.locked} onChange={(e) => patch(selected.id, { locked: e.target.checked })} />
              <span>Travar (não arrastar na área de trabalho)</span>
            </label>
            <button
              type="button"
              className="btn"
              onClick={() => {
                const rest = widgets.filter((w) => w.id !== selected.id);
                onChange(rest);
                setSel(rest[0]?.id ?? null);
              }}
            >
              Remover
            </button>
          </div>
          <details>
            <summary>Avançado: posição e tamanho em pixels</summary>
            <div className="wc-row">
              {(
                [
                  ["Esquerda", "x"],
                  ["Topo", "y"],
                  ["Largura", "width"],
                  ["Altura", "height"],
                ] as const
              ).map(([n, k]) => (
                <label key={k}>
                  {n}{" "}
                  <input
                    type="number"
                    aria-label={n}
                    value={selected[k]}
                    onChange={(e) => patch(selected.id, { [k]: Number(e.target.value) })}
                  />
                </label>
              ))}
            </div>
          </details>
        </div>
      )}
    </div>
  );
}
