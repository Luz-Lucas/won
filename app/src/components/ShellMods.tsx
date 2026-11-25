import { useCallback, useEffect, useState } from "react";
import { shellModSet, shellModsStatus, xamlCfgWrite } from "../api";
import type { ShellModItem, ShellModsStatus } from "../api";
import { buildXamlCfg, parseXamlCfg } from "../xamlCfg";
import type { XamlStyle } from "../xamlCfg";
import { PRESETS_UNAVAILABLE, STYLE_PRESETS } from "../stylePresets";
import ConfirmDialog from "./ConfirmDialog";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

const INFO: Record<ShellModItem["id"], { title: string; text: string }> = {
  xaml: {
    title: "Estilo da barra e do Iniciar",
    text: "Cor, transparência, cantos arredondados e barra flutuante (ilha), no Explorer e no Iniciar.",
  },
  icones: { title: "Ícones pequenos na barra", text: "Reduz os botões da barra de tarefas." },
  rotulos: { title: "Rótulos nos botões da barra", text: "Mostra o nome do programa e nunca junta os botões." },
  tema: {
    title: "Temas sem assinatura (só nesta sessão)",
    text: "Permite temas de terceiros até o Explorer reiniciar. Não altera arquivos do Windows.",
  },
};

/** Liga e desliga os efeitos avançados do Explorer; `only` limita a quais aparecem. */
export default function ShellMods({ only }: { only?: ShellModItem["id"][] }) {
  const [status, setStatus] = useState<ShellModsStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [ask, setAsk] = useState<ShellModItem["id"] | null>(null);
  const [style, setStyle] = useState<XamlStyle | null>(null);
  const [picked, setPicked] = useState<string | null>(null);

  const load = useCallback(() => {
    Promise.resolve(shellModsStatus()).then(
      (s) => {
        if (!s) return;
        setStatus(s);
        setStyle((cur) => cur ?? parseXamlCfg(s.xaml_cfg));
      },
      (e) => setError(errorText(e)),
    );
  }, []);
  useEffect(load, [load]);

  const run = async (f: () => Promise<unknown>, done: string) => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await f();
      setNotice(done);
      load();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const toggle = (m: ShellModItem) =>
    m.active
      ? void run(() => shellModSet(m.id, false), `${INFO[m.id].title}: desligado.`)
      : setAsk(m.id);

  const save = () =>
    run(async () => {
      if (!style) return;
      await xamlCfgWrite(buildXamlCfg(style));
    }, "Estilo salvo.");

  if (!status) return error ? <p role="alert" className="form-error">{error}</p> : null;

  const items = status.mods.filter((m) => !only || only.includes(m.id));
  const blocked = !status.installed
    ? "O motor de mods (sentinela-mods.exe) não foi encontrado."
    : status.anticheat_running
      ? "Há um anticheat aberto: nada é ligado enquanto ele estiver rodando."
      : null;
  const xaml = items.find((m) => m.id === "xaml");
  const set = (p: Partial<XamlStyle>) => setStyle((s) => (s ? { ...s, ...p } : s));

  return (
    <div className="game-session shell-mods">
      <h2>Efeitos avançados do Explorer</h2>
      <p className="muted">
        Recursos experimentais, validados só no Windows de teste. Cada um pode ser desligado aqui,
        e o script de recuperação (página Mods) desfaz tudo mesmo sem o app abrir.
      </p>
      {blocked && <p role="alert" className="form-error">{blocked}</p>}
      {error && <p role="alert" className="form-error">{error}</p>}
      {notice && <p role="status">{notice}</p>}
      <ul className="list">
        {items.map((m) => (
          <li key={m.id}>
            <label>
              <input
                type="checkbox"
                checked={m.active}
                disabled={busy || blocked !== null || !m.dll_exists}
                onChange={() => toggle(m)}
              />{" "}
              <strong>{INFO[m.id].title}</strong>
            </label>
            <div className="muted">{INFO[m.id].text}</div>
            {!m.dll_exists && <div className="form-error">Arquivo do efeito não encontrado.</div>}
          </li>
        ))}
      </ul>
      {xaml && style && (
        <fieldset disabled={busy}>
          <legend>Aparência da barra e do Iniciar</legend>
          <div className="row">
            <label>
              Estilos prontos{" "}
              <select
                aria-label="Estilos prontos"
                value=""
                onChange={(e) => {
                  const p = STYLE_PRESETS.find((x) => x.id === e.target.value);
                  if (p) {
                    set({ startFill: null, startRadius: null, islands: false, ...p.style });
                    setPicked(p.id);
                  }
                }}
              >
                <option value="">Escolher…</option>
                {STYLE_PRESETS.map((p) => (
                  <option key={p.id} value={p.id}>
                    {`${p.name} (${p.repo})`}
                  </option>
                ))}
              </select>
            </label>
          </div>
          {picked && (
            <p className="muted">
              {(() => {
                const p = STYLE_PRESETS.find((x) => x.id === picked)!;
                return `${p.name}: origem ${p.repo}, ${p.credit}.${p.approx ? " Aproximado: " : " "}${p.note} Clique em Aplicar estilo para valer.`;
              })()}
            </p>
          )}
          <details>
            <summary>Estilos dos repositórios que não dá para aplicar</summary>
            <ul>
              {PRESETS_UNAVAILABLE.map((u) => (
                <li key={u.what}>
                  <strong>{u.what}:</strong> {u.why}
                </li>
              ))}
            </ul>
          </details>
          <div className="row">
            <label>
              Cor da barra{" "}
              <input type="color" value={style.fill} onChange={(e) => set({ fill: e.target.value })} />
            </label>
            <label>
              Opacidade{" "}
              <input
                type="range"
                min={0}
                max={255}
                value={style.alpha}
                onChange={(e) => set({ alpha: Number(e.target.value) })}
              />
            </label>
            <label>
              Cantos{" "}
              <input
                type="range"
                min={0}
                max={40}
                value={style.radius}
                onChange={(e) => set({ radius: Number(e.target.value) })}
              />
            </label>
            <label>
              Barra flutuante (margem){" "}
              <input
                type="range"
                min={0}
                max={40}
                value={style.margin[0]}
                onChange={(e) => {
                  const m = Number(e.target.value);
                  set({ margin: [m, m, m, m] });
                }}
              />
            </label>
            <label>
              <input
                type="checkbox"
                checked={style.hideBorder}
                onChange={(e) => set({ hideBorder: e.target.checked })}
              />{" "}
              Esconder a borda de cima
            </label>
          </div>
          <div className="row">
            <label>
              <input
                type="checkbox"
                checked={style.islands}
                onChange={(e) => set({ islands: e.target.checked })}
              />{" "}
              Duas ilhas na barra do Windows (apps e bandeja separados)
            </label>
            {style.islands && (
              <>
                <label>
                  Folga dos apps{" "}
                  <input
                    type="range"
                    min={0}
                    max={40}
                    value={style.islandPad}
                    onChange={(e) => set({ islandPad: Number(e.target.value) })}
                  />
                </label>
                <label>
                  Folga da bandeja{" "}
                  <input
                    type="range"
                    min={0}
                    max={40}
                    value={style.islandTrayPad}
                    onChange={(e) => set({ islandTrayPad: Number(e.target.value) })}
                  />
                </label>
                <label>
                  Flutuar{" "}
                  <input
                    type="range"
                    min={0}
                    max={20}
                    value={style.islandV}
                    onChange={(e) => set({ islandV: Number(e.target.value) })}
                  />
                </label>
              </>
            )}
          </div>
          <div className="row">
            <label>
              <input
                type="checkbox"
                checked={style.startHideAll}
                onChange={(e) => set({ startHideAll: e.target.checked })}
              />{" "}
              Iniciar só com os apps fixados (esconder "Todos os aplicativos")
            </label>
          </div>
          <div className="row">
            <label>
              <input
                type="checkbox"
                checked={style.startW !== null || style.startH !== null}
                onChange={(e) =>
                  set({ startW: e.target.checked ? 700 : null, startH: e.target.checked ? 800 : null })
                }
              />{" "}
              Tamanho do menu Iniciar <em>(experimental)</em>
            </label>
            {style.startW !== null && (
              <>
                <label>
                  Largura{" "}
                  <input
                    type="number"
                    min={300}
                    max={2400}
                    value={style.startW}
                    onChange={(e) => set({ startW: Number(e.target.value) })}
                  />
                </label>
                <label>
                  Altura{" "}
                  <input
                    type="number"
                    min={300}
                    max={1600}
                    value={style.startH ?? 800}
                    onChange={(e) => set({ startH: Number(e.target.value) })}
                  />
                </label>
              </>
            )}
          </div>
          <div className="row">
            <label>
              <input
                type="checkbox"
                checked={style.hideStartButton}
                onChange={(e) => set({ hideStartButton: e.target.checked })}
              />{" "}
              Esconder o botão Iniciar da barra
            </label>
            <label>
              <input
                type="checkbox"
                checked={style.hideWidgets}
                onChange={(e) => set({ hideWidgets: e.target.checked })}
              />{" "}
              Esconder o botão de widgets
            </label>
            <label>
              <input
                type="checkbox"
                checked={style.hideSearchGleam}
                onChange={(e) => set({ hideSearchGleam: e.target.checked })}
              />{" "}
              Esconder o brilho da caixa de pesquisa
            </label>
          </div>
          <div className="row">
            <label>
              <input
                type="checkbox"
                checked={style.startFill !== null}
                onChange={(e) =>
                  set({ startFill: e.target.checked ? "#1C1C24" : null, startRadius: e.target.checked ? 14 : null })
                }
              />{" "}
              Estilizar o menu Iniciar
            </label>
            {style.startFill !== null && (
              <>
                <input
                  type="color"
                  aria-label="Cor do Iniciar"
                  value={style.startFill}
                  onChange={(e) => set({ startFill: e.target.value })}
                />
                <input
                  type="range"
                  aria-label="Opacidade do Iniciar"
                  min={0}
                  max={255}
                  value={style.startAlpha}
                  onChange={(e) => set({ startAlpha: Number(e.target.value) })}
                />
                <input
                  type="range"
                  aria-label="Cantos do Iniciar"
                  min={0}
                  max={40}
                  value={style.startRadius ?? 14}
                  onChange={(e) => set({ startRadius: Number(e.target.value) })}
                />
              </>
            )}
          </div>
          <button type="button" className="btn" onClick={() => void save()}>
            Aplicar estilo
          </button>
          {!xaml.active && (
            <p className="muted">Ligue “Estilo da barra e do Iniciar” acima para o estilo valer.</p>
          )}
        </fieldset>
      )}
      {ask && (
        <ConfirmDialog
          title={`Ligar: ${INFO[ask].title}?`}
          confirmLabel="Ligar"
          onConfirm={() => {
            const id = ask;
            setAsk(null);
            void run(() => shellModSet(id, true), `${INFO[id].title}: ligado.`);
          }}
          onCancel={() => setAsk(null)}
        >
          <p>
            Isto injeta uma DLL no Explorer (build {status.build}). É experimental e a barra pode
            piscar. O script de recuperação é gravado antes e dá para desligar aqui a qualquer hora.
          </p>
        </ConfirmDialog>
      )}
    </div>
  );
}
