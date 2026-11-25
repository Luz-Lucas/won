import type { TweakArea, TweakItem } from "../api";

/** Ids de ajustes "ligados" na prévia (o estado atual, com a opção sob o cursor). */
export type On = ReadonlySet<string>;

/**
 * Estado que a prévia desenha: o que está aplicado agora; se `hover` é um
 * interruptor, o oposto do estado dele; se é uma opção de lista, ela no lugar
 * das irmãs. Assim a prévia mostra "como vai ficar" antes de aplicar.
 */
export function effectiveOn(items: TweakItem[], hover: string | null): Set<string> {
  const on = new Set(
    items.filter((i) => i.view.status === "applied").map((i) => i.tweak.id),
  );
  const h = items.find((i) => i.tweak.id === hover);
  if (!h) return on;
  const choice = h.tweak.choice;
  if (choice) {
    for (const i of items) if (i.tweak.choice === choice) on.delete(i.tweak.id);
    on.add(h.tweak.id);
  } else if (!on.delete(h.tweak.id)) on.add(h.tweak.id);
  return on;
}

export type PreviewArea = TweakArea | "temas" | "wallpaper" | "terminal" | "desktop" | "loja";

const ICONS = ["#2f7bff", "#6ebeff", "#1446c8", "#9fd0ff"];

function Taskbar({ on, light }: { on: On; light: boolean }) {
  const left = on.has("taskbar-align-left");
  const search = on.has("taskbar-search-hidden")
    ? "none"
    : on.has("taskbar-search-box")
      ? "box"
      : "icon";
  const never = on.has("taskbar-combine-never");
  return (
    <div className={`sp-taskbar${light ? " sp-light" : ""}`} data-align={left ? "left" : "center"}>
      <div className="sp-tb-icons">
        <span className="sp-start" />
        {search === "icon" && <span className="sp-ico sp-ico-search" />}
        {search === "box" && <span className="sp-searchbox">Pesquisar</span>}
        {!on.has("taskbar-taskview-off") && <span className="sp-ico sp-ico-view" />}
        {ICONS.map((c, n) => (
          <span key={c} className={`sp-app${never ? " sp-app-wide" : ""}`} style={{ background: c }}>
            {never && n < 2 ? `Janela ${n + 1}` : null}
          </span>
        ))}
      </div>
      <div className="sp-tray">
        {!on.has("taskbar-widgets-off") && <span className="sp-ico sp-ico-widget" />}
        <span className="sp-clock">{on.has("taskbar-clock-seconds") ? "14:32:07" : "14:32"}</span>
      </div>
    </div>
  );
}

function Frame({
  label,
  light,
  children,
}: {
  label: string;
  light?: boolean;
  children: React.ReactNode;
}) {
  return (
    <div className={`sp-desk${light ? " sp-light" : ""}`} role="img" aria-label={label}>
      {children}
    </div>
  );
}

function StartMenu({ on }: { on: On }) {
  const morePins = on.has("start-layout-more-pins");
  const moreRecs = on.has("start-layout-more-recommendations");
  const noRecs = on.has("start-recommendations-off");
  const pinRows = morePins ? 4 : moreRecs ? 2 : 3;
  const recRows = noRecs ? 0 : moreRecs ? 4 : morePins ? 1 : 2;
  return (
    <Frame label="Prévia do menu Iniciar">
      <div className="sp-start-menu">
        <div className="sp-search-field">
          {on.has("start-bing-off") ? "Pesquisar em apps e arquivos" : "Pesquisar apps, arquivos e a web"}
        </div>
        <div className="sp-section-title">Fixados</div>
        <div className="sp-grid">
          {Array.from({ length: pinRows * 6 }, (_, n) => (
            <span key={n} style={{ background: ICONS[n % ICONS.length] }} />
          ))}
        </div>
        {recRows > 0 && (
          <>
            <div className="sp-section-title">Recomendados</div>
            <div className="sp-recs">
              {Array.from({ length: recRows }, (_, n) => (
                <div key={n} className="sp-rec">
                  <span style={{ background: ICONS[(n + 1) % ICONS.length] }} />
                  Arquivo recente {n + 1}
                </div>
              ))}
            </div>
          </>
        )}
      </div>
      <Taskbar on={new Set()} light={false} />
    </Frame>
  );
}

function Explorer({ on }: { on: On }) {
  const compact = on.has("explorer-compact");
  const ext = on.has("explorer-show-extensions");
  const hidden = on.has("explorer-show-hidden");
  const files = [
    ["Relatório", ".docx", false],
    ["Foto", ".png", false],
    ["config", ".ini", true],
    ["Planilha", ".xlsx", false],
  ] as const;
  return (
    <Frame label="Prévia do Explorador">
      <div className="sp-explorer">
        <div className="sp-pane">
          {!on.has("explorer-hide-home") && <span>Início</span>}
          {!on.has("explorer-hide-gallery") && <span>Galeria</span>}
          <span>Este Computador</span>
          {on.has("explorer-open-this-pc") && <em>abre aqui</em>}
        </div>
        <ul className={`sp-files${compact ? " sp-compact" : ""}`}>
          {files
            .filter(([, , h]) => hidden || !h)
            .map(([name, e, h]) => (
              <li key={name} className={h ? "sp-dim" : undefined}>
                {name}
                {ext ? e : ""}
              </li>
            ))}
        </ul>
        <div className={`sp-menu${on.has("explorer-classic-menu") ? " sp-menu-classic" : ""}`}>
          {on.has("explorer-classic-menu") ? (
            <>
              <span>Abrir</span>
              <span>Copiar</span>
              <span>Excluir</span>
              <span>Propriedades</span>
            </>
          ) : (
            <>
              <span>Abrir</span>
              <span>Mostrar mais opções</span>
            </>
          )}
        </div>
      </div>
    </Frame>
  );
}

function Appearance({ on }: { on: On }) {
  const appsLight = on.has("theme-apps-light");
  const sysLight = on.has("theme-system-light");
  const solid = on.has("transparency-off");
  return (
    <Frame label="Prévia de cores e aparência" light={sysLight}>
      <div className={`sp-window${appsLight ? " sp-light" : ""}${solid ? " sp-solid" : ""}`}>
        <div className="sp-titlebar">Aplicativo</div>
        <div className="sp-body">
          <span />
          <span />
          <span />
        </div>
      </div>
      <Taskbar on={new Set()} light={sysLight} />
    </Frame>
  );
}

function System({ on, items }: { on: On; items: TweakItem[] }) {
  return (
    <Frame label="Prévia das opções do sistema">
      <ul className="sp-status">
        {items.map((i) => (
          <li key={i.tweak.id} data-on={on.has(i.tweak.id)}>
            <span className="sp-dot" />
            {i.tweak.title}
          </li>
        ))}
      </ul>
    </Frame>
  );
}

/** Miniatura do desktop com o elemento da área; ilustrativa, não captura a tela real. */
export default function StylePreview({
  area,
  on,
  items = [],
}: {
  area: PreviewArea;
  on: On;
  items?: TweakItem[];
}) {
  switch (area) {
    case "barra":
      return (
        <Frame label="Prévia da barra de tarefas">
          <div className="sp-window sp-window-small">
            <div className="sp-titlebar">Aplicativo</div>
          </div>
          <Taskbar on={on} light={false} />
        </Frame>
      );
    case "iniciar":
      return <StartMenu on={on} />;
    case "explorador":
      return <Explorer on={on} />;
    case "aparencia":
      return <Appearance on={on} />;
    case "sistema":
      return <System on={on} items={items} />;
    case "temas":
      return (
        <Frame label="Prévia de temas">
          <div className="sp-window sp-window-small">
            <div className="sp-titlebar">Tema</div>
          </div>
          <Taskbar on={new Set()} light={false} />
        </Frame>
      );
    case "loja":
      return (
        <Frame label="Prévia da Loja">
          <div className="sp-store">
            <div className="sp-store-head">
              <span className="sp-store-bag" />
              <span className="sp-store-search" />
            </div>
            <div className="sp-shop">
              {ICONS.map((c, i) => (
                <span key={c}>
                  <i style={{ background: c }} className={i === 1 ? "sp-pointer" : undefined} />
                  <b className="sp-get" />
                </span>
              ))}
            </div>
          </div>
        </Frame>
      );
    case "wallpaper":
      return (
        <Frame label="Prévia do papel de parede">
          <div className="sp-wallpaper" />
          <Taskbar on={new Set()} light={false} />
        </Frame>
      );
    case "terminal":
      return (
        <Frame label="Prévia do terminal">
          <div className="sp-terminal">
            <span>~ main ❯ </span>
            <span className="sp-cursor" />
          </div>
        </Frame>
      );
    case "desktop":
      return (
        <Frame label="Prévia do ambiente de desktop">
          <div className="sp-tiles">
            <span />
            <span />
            <span />
          </div>
          <Taskbar on={new Set()} light={false} />
        </Frame>
      );
  }
}
