import { useCallback, useEffect, useMemo, useState } from "react";
import {
  lojaApply,
  lojaCursorSize,
  lojaSetCursorSize,
  lojaApplyShortcuts,
  lojaInstall,
  lojaList,
  lojaRemove,
  lojaRevert,
  lojaRevertShortcuts,
  restartExplorer,
} from "../api";
import type { LojaItem, LojaKind } from "../api";
import ConfirmDialog from "./ConfirmDialog";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

type Tab = "todos" | LojaKind;

const TABS: { id: Tab; label: string }[] = [
  { id: "todos", label: "Tudo" },
  { id: "cursores", label: "Ponteiros do mouse" },
  { id: "icones", label: "Pacotes de ícones" },
];

const KIND_LABEL: Record<LojaKind, string> = {
  cursores: "Ponteiros",
  icones: "Ícones",
};

const size = (b: number) => (b >= 1024 * 1024 ? `${(b / 1024 / 1024).toFixed(1)} MB` : `${Math.max(1, Math.round(b / 1024))} KB`);

const CURSOR_SIZES: { px: number; label: string }[] = [
  { px: 16, label: "Mini" },
  { px: 24, label: "Pequeno" },
  { px: 32, label: "Padrão" },
  { px: 48, label: "Médio" },
  { px: 64, label: "Grande" },
];

/** Loja: ponteiros e ícones para baixar, aplicar e reverter. */
export default function Loja() {
  const [items, setItems] = useState<LojaItem[]>([]);
  const [online, setOnline] = useState(false);
  const [onlineError, setOnlineError] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("todos");
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [askExplorer, setAskExplorer] = useState(false);
  const [cursorPx, setCursorPx] = useState<number | null>(null);

  useEffect(() => {
    void lojaCursorSize().then(setCursorPx, () => undefined);
  }, []);

  const changeCursorSize = (px: number | null) =>
    run("cursor-size", async () => {
      await lojaSetCursorSize(px);
      setCursorPx(px);
      setNote(px === null ? "Tamanho do ponteiro devolvido ao de antes." : `Ponteiro em ${px} px.`);
    });
  const [shortcutsActive, setShortcutsActive] = useState<string | null>(null);
  const [askShortcuts, setAskShortcuts] = useState<LojaItem | null>(null);

  const load = useCallback((wantOnline: boolean) => {
    return lojaList(wantOnline).then(
      (p) => {
        setItems(p.items);
        setOnline(p.online);
        setOnlineError(p.online_error);
        setShortcutsActive(p.shortcuts_active);
      },
      (e) => setError(errorText(e)),
    );
  }, []);

  useEffect(() => {
    void load(false);
  }, [load]);

  const byId = useMemo(() => new Map(items.map((i) => [i.id, i])), [items]);

  const shown = items.filter(
    (i) =>
      (tab === "todos" || i.kind === tab) &&
      (query.trim() === "" ||
        `${i.name} ${i.description} ${i.tags.join(" ")}`.toLowerCase().includes(query.trim().toLowerCase())),
  );

  /** Roda uma ação do item, mostra o erro e recarrega a lista. */
  const run = async (id: string, action: () => Promise<void>) => {
    setBusy(id);
    setError(null);
    setNote(null);
    try {
      await action();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(null);
      await load(online);
    }
  };

  const install = (i: LojaItem) =>
    run(i.id, async () => {
      await lojaInstall(i.id, online);
      setNote(`"${i.name}" foi baixado. Agora é só aplicar.`);
    });

  const applyPack = async (i: LojaItem) => {
    if (!i.installed) await lojaInstall(i.id, online);
    const r = await lojaApply(i.id, online);
    if (r.restart_explorer) setAskExplorer(true);
  };

  const apply = (i: LojaItem) =>
    run(i.id, async () => {
      await applyPack(i);
      setNote(`"${i.name}" aplicado. Dá para reverter quando quiser.`);
    });

  const revert = (i: LojaItem) =>
    run(i.id, async () => {
      const r = await lojaRevert(i.kind);
      if (r.failed.length > 0) throw new Error(`Não deu para devolver tudo: ${r.failed.join("; ")}`);
      setNote("Revertido ao que estava antes.");
      if (r.restart_explorer) setAskExplorer(true);
    });

  // Atalhos (.lnk): troca o ícone dos apps da Área de Trabalho e do Menu Iniciar; confirma antes.
  const confirmShortcuts = async () => {
    const i = askShortcuts;
    setAskShortcuts(null);
    if (!i) return;
    await run(i.id, async () => {
      const r = await lojaApplyShortcuts(i.id, online);
      if (r.failed.length > 0) {
        throw new Error(`${r.changed} atalho(s) trocados; falharam: ${r.failed.join("; ")}`);
      }
      setNote(`${r.changed} atalho(s) com os ícones de "${i.name}". "Restaurar atalhos" devolve os originais.`);
      if (r.restart_explorer) setAskExplorer(true);
    });
  };

  const restoreShortcuts = () =>
    run("atalhos", async () => {
      const r = await lojaRevertShortcuts();
      if (r.failed.length > 0) throw new Error(`Não deu para devolver tudo: ${r.failed.join("; ")}`);
      setNote(`${r.restored} atalho(s) voltaram ao ícone original.`);
      if (r.restart_explorer) setAskExplorer(true);
    });

  const remove = (i: LojaItem) =>
    run(i.id, async () => {
      await lojaRemove(i.id, online);
      setNote(`"${i.name}" foi removido da biblioteca.`);
    });

  const restart = async () => {
    setAskExplorer(false);
    try {
      await restartExplorer();
    } catch (e) {
      setError(errorText(e));
    }
  };

  return (
    <div className="loja">
      {error && (
        <p role="alert" className="form-error">
          {error}
        </p>
      )}
      {note && <div className="notice">{note}</div>}
      {shortcutsActive && (
        <div className="notice" role="status">
          <span>{`Ícones de aplicativos aplicados nos atalhos (${byId.get(shortcutsActive)?.name ?? shortcutsActive}). `}</span>
          <button type="button" className="btn" disabled={busy !== null} onClick={() => void restoreShortcuts()}>
            Restaurar atalhos
          </button>
        </div>
      )}
      <div className="loja-bar">
        <div role="tablist" aria-label="Categorias" className="loja-tabs">
          {TABS.map((t) => (
            <button
              key={t.id}
              type="button"
              role="tab"
              aria-selected={tab === t.id}
              className={tab === t.id ? "btn btn-primary" : "btn"}
              onClick={() => setTab(t.id)}
            >
              {t.label}
            </button>
          ))}
        </div>
        <label className="loja-search">
          <input
            type="search"
            aria-label="Buscar na Loja"
            placeholder="Buscar (neon, claro, grande…)"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </label>
        <button
          type="button"
          className="btn"
          disabled={busy !== null}
          onClick={() => void run("online", () => load(true))}
        >
          {online ? "Atualizar catálogo online" : "Buscar mais itens online"}
        </button>
      </div>
      {onlineError && (
        <p className="muted" role="status">
          {`Catálogo online indisponível agora (${onlineError}). Os itens do programa continuam funcionando.`}
        </p>
      )}
      {(tab === "todos" || tab === "cursores") && (
        <div className="loja-size" role="group" aria-label="Tamanho do ponteiro">
          <strong>Tamanho do ponteiro</strong>
          {CURSOR_SIZES.map((c) => (
            <button
              key={c.px}
              type="button"
              className={`btn${(cursorPx ?? 32) === c.px ? " btn-primary" : ""}`}
              aria-pressed={(cursorPx ?? 32) === c.px}
              disabled={busy !== null}
              onClick={() => void changeCursorSize(c.px === 32 && cursorPx === null ? null : c.px)}
            >
              {c.label}
            </button>
          ))}
          {cursorPx !== null && (
            <button type="button" className="btn" disabled={busy !== null} onClick={() => void changeCursorSize(null)}>
              Desfazer tamanho
            </button>
          )}
        </div>
      )}
      {shown.length === 0 && <p className="muted">Nada encontrado.</p>}
      <ul className="loja-grid" aria-label="Itens da Loja">
        {shown.map((i) => {
          const working = busy === i.id;
          return (
            <li key={i.id} className="loja-card">
              {i.preview ? (
                <img src={i.preview} alt={`Prévia de ${i.name}`} />
              ) : (
                <div className="loja-nopreview" aria-hidden="true" />
              )}
              <div className="clean-head">
                <strong>{i.name}</strong>
                <span className="tweak-badge">{KIND_LABEL[i.kind]}</span>
                {i.applied && <span className="tweak-badge">Aplicado</span>}
                {i.remote && <span className="tweak-badge">Online</span>}
                {i.installed && !i.applied && <span className="tweak-badge">Baixado</span>}
              </div>
              <p className="muted">{i.description}</p>
              <p className="muted loja-meta">
                {`${i.author} · ${i.license}${i.bytes ? ` · ${size(i.bytes)}` : ""}`}
              </p>
              <div className="loja-actions">
                {!i.installed ? (
                  <>
                    <button type="button" className="btn btn-primary" disabled={busy !== null} onClick={() => void install(i)} aria-label={`Baixar ${i.name}`}>
                      {working ? "Baixando…" : "Baixar"}
                    </button>
                    <button type="button" className="btn" disabled={busy !== null} onClick={() => void apply(i)} aria-label={`Baixar e aplicar ${i.name}`}>
                      Baixar e aplicar
                    </button>
                  </>
                ) : i.applied ? (
                  <>
                    <button type="button" className="btn" disabled={busy !== null} onClick={() => void revert(i)} aria-label={`Reverter ${i.name}`}>
                      {working ? "Aguarde…" : "Reverter"}
                    </button>
                    {i.tags.includes("atalhos") && (
                      <button type="button" className="btn" disabled={busy !== null} onClick={() => setAskShortcuts(i)} aria-label={`Aplicar ${i.name} nos atalhos`}>
                        Aplicar nos atalhos
                      </button>
                    )}
                  </>
                ) : (
                  <>
                    <button type="button" className="btn btn-primary" disabled={busy !== null} onClick={() => void apply(i)} aria-label={`Aplicar ${i.name}`}>
                      {working ? "Aguarde…" : "Aplicar"}
                    </button>
                    <button type="button" className="btn" disabled={busy !== null} onClick={() => void remove(i)} aria-label={`Remover ${i.name}`}>
                      Remover
                    </button>
                  </>
                )}
              </div>
            </li>
          );
        })}
      </ul>
      <p className="muted">
        Ponteiros e ícones mudam só a sua conta e guardam o valor anterior: "Reverter" devolve
        exatamente o que havia. Os itens do programa são originais do WON (CC0). Os pacotes
        de terceiros (Fluent, Phosphor, Tabler, Lucide, Bootstrap, Material, Font Awesome,
        game-icons, Twemoji) mantêm autor e licença de cada um; créditos em docs/loja-creditos.md.
      </p>
      {askShortcuts && (
        <ConfirmDialog
          title={`Trocar o ícone dos atalhos por "${askShortcuts.name}"?`}
          confirmLabel="Trocar ícones"
          onConfirm={() => void confirmShortcuts()}
          onCancel={() => setAskShortcuts(null)}
        >
          <p>
            Muda o ícone dos atalhos da sua Área de Trabalho e do seu Menu Iniciar para um desenho da
            categoria do programa (navegador, terminal, chat, jogo…). Pastas, páginas da web e apps
            da Microsoft Store ficam como estão.
          </p>
          <p className="muted">
            O WON guarda uma cópia de cada atalho antes. "Restaurar atalhos" devolve os arquivos
            originais.
          </p>
        </ConfirmDialog>
      )}
      {askExplorer && (
        <ConfirmDialog
          title="Reiniciar o Explorer?"
          confirmLabel="Reiniciar"
          onConfirm={() => void restart()}
          onCancel={() => setAskExplorer(false)}
        >
          <p>Para os ícones novos aparecerem em todo lugar, o Explorer precisa reiniciar. A barra some por alguns segundos e volta.</p>
        </ConfirmDialog>
      )}
    </div>
  );
}
