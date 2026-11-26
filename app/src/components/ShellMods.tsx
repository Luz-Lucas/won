import { useCallback, useEffect, useState } from "react";
import { shellModSet, shellModsStatus, xamlCfgWrite } from "../api";
import type { ShellModItem, ShellModsStatus } from "../api";
import { buildXamlCfg, parseXamlCfg } from "../xamlCfg";
import type { XamlStyle } from "../xamlCfg";
import { PRESETS_UNAVAILABLE, STYLE_PRESETS } from "../stylePresets";
import ConfirmDialog from "./ConfirmDialog";

type Tab = "barra" | "iniciar" | "pesquisa";
const TABS: { id: Tab; label: string }[] = [
  { id: "barra", label: "Barra do Windows" },
  { id: "iniciar", label: "Menu Iniciar" },
  { id: "pesquisa", label: "Pesquisa" },
];

function Group({ title, hint, children }: { title: string; hint?: string; children: React.ReactNode }) {
  return (
    <section className="est-group">
      <h4>{title}</h4>
      {hint && <p className="muted">{hint}</p>}
      <div className="est-items">{children}</div>
    </section>
  );
}

function Check({ checked, onChange, children }: { checked: boolean; onChange: (v: boolean) => void; children: React.ReactNode }) {
  return (
    <label className="est-check">
      <input type="checkbox" checked={checked} onChange={(e) => onChange(e.target.checked)} /> <span>{children}</span>
    </label>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="est-field">
      <span>{label}</span>
      {children}
    </label>
  );
}

function Slide({ label, value, min, max, onChange }: { label: string; value: number; min: number; max: number; onChange: (v: number) => void }) {
  return (
    <label className="est-field">
      <span>{`${label}: ${value}`}</span>
      <input type="range" aria-label={label} min={min} max={max} value={value} onChange={(e) => onChange(Number(e.target.value))} />
    </label>
  );
}

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

const INFO: Record<ShellModItem["id"], { title: string; text: string }> = {
  xaml: {
    title: "Estilo da barra e do Iniciar",
    text: "Cor, transparência, cantos arredondados e barra flutuante (ilha), no Explorer e no Iniciar.",
  },
  icones: { title: "Ícones pequenos na barra", text: "Reduz os botões da barra de tarefas." },
  rotulos: { title: "Rótulos nos botões da barra", text: "Mostra o nome do programa e nunca junta os botões." },
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
  const [tab, setTab] = useState<Tab>("barra");

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
        <section className="est" aria-labelledby="est-title">
          <h3 id="est-title">Aparência da barra e do Iniciar</h3>
          {!xaml.active && (
            <p className="est-warn">
              Ligue “Estilo da barra e do Iniciar” na lista acima para o estilo valer.
            </p>
          )}
          <fieldset disabled={busy} className="est-box">
            <legend className="sr-only">Aparência da barra e do Iniciar</legend>
            <div className="est-presets">
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
            </div>

            <div role="tablist" aria-label="Seções da aparência" className="est-tabs">
              {TABS.map((t) => (
                <button
                  key={t.id}
                  type="button"
                  role="tab"
                  id={`est-tab-${t.id}`}
                  aria-selected={tab === t.id}
                  aria-controls={`est-panel-${t.id}`}
                  className={tab === t.id ? "est-tab active" : "est-tab"}
                  onClick={() => setTab(t.id)}
                >
                  {t.label}
                </button>
              ))}
            </div>

            {tab === "barra" && (
              <div role="tabpanel" id="est-panel-barra" aria-labelledby="est-tab-barra" className="est-panel">
                <Group title="Cores e formato" hint="Como a barra de tarefas do Windows aparece.">
                  <Field label="Cor da barra">
                    <input type="color" value={style.fill} onChange={(e) => set({ fill: e.target.value })} />
                  </Field>
                  <Slide label="Opacidade" value={style.alpha} min={0} max={255} onChange={(v) => set({ alpha: v })} />
                  <Slide label="Cantos" value={style.radius} min={0} max={40} onChange={(v) => set({ radius: v })} />
                  <Slide
                    label="Barra flutuante (margem)"
                    value={style.margin[0]}
                    min={0}
                    max={40}
                    onChange={(v) => set({ margin: [v, v, v, v] })}
                  />
                  <Check checked={style.hideBorder} onChange={(v) => set({ hideBorder: v })}>
                    Esconder a borda de cima
                  </Check>
                </Group>
                <Group title="Ilhas" hint="Separa a barra em duas pílulas: apps abertos e bandeja.">
                  <Check checked={style.islands} onChange={(v) => set({ islands: v })}>
                    Duas ilhas na barra do Windows (apps e bandeja separados)
                  </Check>
                  {style.islands && (
                    <>
                      <Slide label="Folga dos apps" value={style.islandPad} min={0} max={40} onChange={(v) => set({ islandPad: v })} />
                      <Slide label="Folga da bandeja" value={style.islandTrayPad} min={0} max={40} onChange={(v) => set({ islandTrayPad: v })} />
                      <Slide label="Flutuar" value={style.islandV} min={0} max={20} onChange={(v) => set({ islandV: v })} />
                    </>
                  )}
                </Group>
                <Group title="Botões da barra" hint="O que aparece ou some na barra do Windows.">
                  <Check checked={style.hideStartButton} onChange={(v) => set({ hideStartButton: v })}>
                    Esconder o botão Iniciar da barra
                  </Check>
                  <Check checked={style.hideWidgets} onChange={(v) => set({ hideWidgets: v })}>
                    Esconder o botão de widgets
                  </Check>
                  <Check checked={style.hideSearchGleam} onChange={(v) => set({ hideSearchGleam: v })}>
                    Esconder o brilho da caixa de pesquisa
                  </Check>
                </Group>
                <Group title="Experimental" hint="Podem falhar em outras versões do Windows.">
                  <Check checked={style.startLeft} onChange={(v) => set({ startLeft: v })}>
                    Botão Iniciar sempre à esquerda
                  </Check>
                  <Check checked={style.dock} onChange={(v) => set({ dock: v })}>
                    Animação de dock nos ícones
                  </Check>
                </Group>
              </div>
            )}

            {tab === "iniciar" && (
              <div role="tabpanel" id="est-panel-iniciar" aria-labelledby="est-tab-iniciar" className="est-panel">
                <Group title="Aparência do menu" hint="Cor, transparência e cantos do menu Iniciar.">
                  <Check
                    checked={style.startFill !== null}
                    onChange={(v) => set({ startFill: v ? "#1C1C24" : null, startRadius: v ? 14 : null })}
                  >
                    Estilizar o menu Iniciar
                  </Check>
                  {style.startFill !== null && (
                    <>
                      <Field label="Cor do Iniciar">
                        <input
                          type="color"
                          aria-label="Cor do Iniciar"
                          value={style.startFill}
                          onChange={(e) => set({ startFill: e.target.value })}
                        />
                      </Field>
                      <Slide label="Opacidade do Iniciar" value={style.startAlpha} min={0} max={255} onChange={(v) => set({ startAlpha: v })} />
                      <Slide label="Cantos do Iniciar" value={style.startRadius ?? 14} min={0} max={40} onChange={(v) => set({ startRadius: v })} />
                    </>
                  )}
                </Group>
                <Group title="Conteúdo" hint="O que o menu Iniciar mostra.">
                  <Check checked={style.startHideAll} onChange={(v) => set({ startHideAll: v })}>
                    Iniciar só com os apps fixados (esconder "Todos os aplicativos")
                  </Check>
                  <Check checked={style.startHideList} onChange={(v) => set({ startHideList: v })}>
                    Esconder a lista A-Z inteira (experimental, pode esvaziar o Iniciar)
                  </Check>
                </Group>
                <Group title="Tamanho" hint="Experimental: muda a largura e a altura do menu.">
                  <Check
                    checked={style.startW !== null || style.startH !== null}
                    onChange={(v) => set({ startW: v ? 700 : null, startH: v ? 800 : null })}
                  >
                    Tamanho do menu Iniciar
                  </Check>
                  {style.startW !== null && (
                    <>
                      <Field label="Largura">
                        <input type="number" min={300} max={2400} value={style.startW} onChange={(e) => set({ startW: Number(e.target.value) })} />
                      </Field>
                      <Field label="Altura">
                        <input type="number" min={300} max={1600} value={style.startH ?? 800} onChange={(e) => set({ startH: Number(e.target.value) })} />
                      </Field>
                    </>
                  )}
                </Group>
              </div>
            )}

            {tab === "pesquisa" && (
              <div role="tabpanel" id="est-panel-pesquisa" aria-labelledby="est-tab-pesquisa" className="est-panel">
                <Group title="Pesquisa do Windows" hint="Vale depois de reiniciar a pesquisa.">
                  <Check checked={style.searchDark} onChange={(v) => set({ searchDark: v })}>
                    Forçar modo escuro na tela de resultados da pesquisa (experimental)
                  </Check>
                </Group>
              </div>
            )}

            <div className="est-actions">
              <button type="button" className="btn" onClick={() => void save()}>
                Aplicar estilo
              </button>
              <span className="muted">As mudanças só valem depois de aplicar.</span>
            </div>
          </fieldset>
        </section>
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
