import { useMemo } from "react";
import {
  ACTIONS,
  ACTION_LABEL,
  DEFAULT_BORDERS,
  EMPTY_RULE,
  applyExtras,
  readExtras,
} from "../desktopRules";
import type { ActionName, Backdrop, Corners, Extras, HotkeyModel, RuleModel } from "../desktopRules";

/** Atalhos globais, regras de janela e cores das bordas, sem editar o arquivo à mão. */
export default function DesktopRules({
  text,
  onChange,
}: {
  text: string | null;
  onChange: (text: string) => void;
}) {
  const extras = useMemo(() => (text === null ? null : readExtras(text)), [text]);
  if (text === null || !extras) return null;
  const update = (p: Partial<Extras>) => onChange(applyExtras(text, { ...extras, ...p }));
  const setHotkey = (i: number, p: Partial<HotkeyModel>) =>
    update({ hotkeys: extras.hotkeys.map((h, j) => (j === i ? { ...h, ...p } : h)) });
  const setRule = (i: number, p: Partial<RuleModel>) =>
    update({ rules: extras.rules.map((r, j) => (j === i ? { ...r, ...p } : r)) });

  return (
    <div className="dv">
      <article className="card dv-card" aria-labelledby="dr-hotkeys">
        <header className="dv-head">
          <h2 id="dr-hotkeys">Atalhos</h2>
          <button
            type="button"
            className="btn"
            onClick={() =>
              update({
                hotkeys: [...extras.hotkeys, { keys: "Win+Alt+Z", action: "snap-layouts", command: "", args: "" }],
              })
            }
          >
            Adicionar atalho
          </button>
        </header>
        <p className="muted">
          Combinações como Win+Alt+T. O atalho “Abrir os layouts de encaixe do Windows” deixa o
          Snap nativo e o tiling trabalharem juntos: a janela que você encaixa fica onde está.
        </p>
        {extras.hotkeys.length === 0 && <p className="muted">Nenhum atalho.</p>}
        {extras.hotkeys.map((h, i) => (
          <div className="row" key={i}>
            <input
              aria-label={`Teclas do atalho ${i + 1}`}
              value={h.keys}
              onChange={(e) => setHotkey(i, { keys: e.target.value })}
            />
            <select
              aria-label={`Ação do atalho ${i + 1}`}
              value={h.action}
              onChange={(e) => setHotkey(i, { action: e.target.value as ActionName })}
            >
              {ACTIONS.map((a) => (
                <option key={a} value={a}>
                  {ACTION_LABEL[a]}
                </option>
              ))}
            </select>
            {h.action === "run" && (
              <>
                <input
                  aria-label={`Programa do atalho ${i + 1}`}
                  placeholder="Programa"
                  value={h.command}
                  onChange={(e) => setHotkey(i, { command: e.target.value })}
                />
                <input
                  aria-label={`Argumentos do atalho ${i + 1}`}
                  placeholder="Argumentos"
                  value={h.args}
                  onChange={(e) => setHotkey(i, { args: e.target.value })}
                />
              </>
            )}
            <button
              type="button"
              className="btn"
              aria-label={`Remover atalho ${i + 1}`}
              onClick={() => update({ hotkeys: extras.hotkeys.filter((_, j) => j !== i) })}
            >
              Remover
            </button>
          </div>
        ))}
      </article>

      <article className="card dv-card" aria-labelledby="dr-rules">
        <header className="dv-head">
          <h2 id="dr-rules">Regras de janela</h2>
          <button
            type="button"
            className="btn"
            onClick={() => update({ rules: [...extras.rules, { ...EMPTY_RULE, exe: "programa.exe" }] })}
          >
            Adicionar regra
          </button>
        </header>
        <p className="muted">
          Escolha o programa (e, se quiser, parte do título ou a classe) e o que fazer com a janela.
        </p>
        {extras.rules.length === 0 && <p className="muted">Nenhuma regra.</p>}
        {extras.rules.map((r, i) => (
          <fieldset key={i}>
            <legend>{`Regra ${i + 1}`}</legend>
            <div className="row">
              <input aria-label={`Programa da regra ${i + 1}`} placeholder="Programa (notepad.exe)" value={r.exe} onChange={(e) => setRule(i, { exe: e.target.value })} />
              <input aria-label={`Título da regra ${i + 1}`} placeholder="Título contém" value={r.title} onChange={(e) => setRule(i, { title: e.target.value })} />
              <input aria-label={`Classe da regra ${i + 1}`} placeholder="Classe da janela" value={r.class} onChange={(e) => setRule(i, { class: e.target.value })} />
            </div>
            <div className="row">
              <label>
                <input type="checkbox" checked={r.float} onChange={(e) => setRule(i, { float: e.target.checked })} /> Fora do tiling
              </label>
              <label>
                <input type="checkbox" checked={r.noTitlebar} onChange={(e) => setRule(i, { noTitlebar: e.target.checked })} /> Sem barra de título
              </label>
              <label>
                Sempre no topo{" "}
                <select aria-label={`Topo da regra ${i + 1}`} value={r.topmost} onChange={(e) => setRule(i, { topmost: e.target.value as RuleModel["topmost"] })}>
                  <option value="">Não mexer</option>
                  <option value="true">Sim</option>
                  <option value="false">Não</option>
                </select>
              </label>
              <label>
                Opacidade (0 a 255){" "}
                <input aria-label={`Opacidade da regra ${i + 1}`} type="number" min={0} max={255} value={r.opacity} onChange={(e) => setRule(i, { opacity: e.target.value })} />
              </label>
              <label>
                Cantos{" "}
                <select aria-label={`Cantos da regra ${i + 1}`} value={r.corners} onChange={(e) => setRule(i, { corners: e.target.value as Corners })}>
                  <option value="">Não mexer</option>
                  <option value="round">Redondos</option>
                  <option value="small">Pequenos</option>
                  <option value="square">Retos</option>
                </select>
              </label>
              <label>
                Fundo{" "}
                <select aria-label={`Fundo da regra ${i + 1}`} value={r.backdrop} onChange={(e) => setRule(i, { backdrop: e.target.value as Backdrop })}>
                  <option value="">Não mexer</option>
                  <option value="mica">Mica</option>
                  <option value="acrylic">Acrílico</option>
                  <option value="tabbed">Mica com abas</option>
                  <option value="none">Nenhum</option>
                </select>
              </label>
              <button type="button" className="btn" aria-label={`Remover regra ${i + 1}`} onClick={() => update({ rules: extras.rules.filter((_, j) => j !== i) })}>
                Remover
              </button>
            </div>
          </fieldset>
        ))}
      </article>

      <article className="card dv-card" aria-labelledby="dr-borders">
        <header className="dv-head">
          <h2 id="dr-borders">Bordas das janelas</h2>
          <label className="clean-check">
            <input
              type="checkbox"
              role="switch"
              aria-label="Colorir as bordas"
              checked={extras.borders !== null}
              onChange={(e) => update({ borders: e.target.checked ? { ...DEFAULT_BORDERS } : null })}
            />
            <span>{extras.borders ? "Ligadas" : "Desligadas"}</span>
          </label>
        </header>
        {extras.borders && (
          <div className="row">
            <label>
              Janela ativa{" "}
              <input type="color" aria-label="Cor da borda ativa" value={extras.borders.active} onChange={(e) => update({ borders: { ...extras.borders!, active: e.target.value.toUpperCase() } })} />
            </label>
            <label>
              Janela inativa{" "}
              <input type="color" aria-label="Cor da borda inativa" value={extras.borders.inactive} onChange={(e) => update({ borders: { ...extras.borders!, inactive: e.target.value.toUpperCase() } })} />
            </label>
          </div>
        )}
      </article>
    </div>
  );
}
