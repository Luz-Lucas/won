import { useCallback, useEffect, useMemo, useState } from "react";
import {
  lojaApply,
  lojaInstall,
  lojaList,
  lojaRemove,
  lojaRevert,
  restartExplorer,
  themeApply,
  themePreview,
  themeUndo,
} from "../api";
import type { LojaItem, LojaKind, ThemePreview } from "../api";
import ConfirmDialog from "./ConfirmDialog";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

type Tab = "todos" | LojaKind;

const TABS: { id: Tab; label: string }[] = [
  { id: "todos", label: "Tudo" },
  { id: "temas", label: "Temas" },
  { id: "cursores", label: "Ponteiros do mouse" },
  { id: "icones", label: "Pacotes de ícones" },
];

const KIND_LABEL: Record<LojaKind, string> = {
  cursores: "Ponteiros",
  icones: "Ícones",
  temas: "Tema",
};

const size = (b: number) => (b >= 1024 * 1024 ? `${(b / 1024 / 1024).toFixed(1)} MB` : `${Math.max(1, Math.round(b / 1024))} KB`);

interface PendingTheme {
  item: LojaItem;
  preview: ThemePreview;
}

/** Loja: temas, ponteiros e ícones para baixar, aplicar e reverter. */
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
  const [pending, setPending] = useState<PendingTheme | null>(null);

  const load = useCallback((wantOnline: boolean) => {
    return lojaList(wantOnline).then(
      (p) => {
        setItems(p.items);
        setOnline(p.online);
        setOnlineError(p.online_error);
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

  const remove = (i: LojaItem) =>
    run(i.id, async () => {
      await lojaRemove(i.id, online);
      setNote(`"${i.name}" foi removido da biblioteca.`);
    });

  // Tema: prévia do motor de temas primeiro; nada muda sem confirmação.
  const askTheme = (i: LojaItem) =>
    run(i.id, async () => {
      if (!i.toml) throw new Error("Tema sem conteúdo.");
      setPending({ item: i, preview: await themePreview(i.toml) });
    });

  const confirmTheme = async () => {
    const p = pending;
    setPending(null);
    if (!p?.item.toml) return;
    await run(p.item.id, async () => {
      const done = await themeApply(p.item.toml as string);
      const failed = done.results.filter((r) => !r.ok);
      for (const id of [p.item.cursor, p.item.icons]) {
        const part = id ? byId.get(id) : undefined;
        if (part) await applyPack(part);
      }
      setNote(
        failed.length > 0
          ? `Tema aplicado com ${failed.length} ajuste(s) que falharam.`
          : `Tema "${p.item.name}" aplicado com ponteiro e ícones.`,
      );
    });
  };

  const undoTheme = (i: LojaItem) =>
    run(i.id, async () => {
      await themeUndo().catch(() => undefined);
      for (const id of [i.cursor, i.icons]) {
        const part = id ? byId.get(id) : undefined;
        if (part?.applied) await lojaRevert(part.kind);
      }
      setNote("Tema desfeito e ponteiro/ícones revertidos.");
      if (i.icons && byId.get(i.icons)?.applied) setAskExplorer(true);
    });

  const restart = async () => {
    setAskExplorer(false);
    try {
      await restartExplorer();
    } catch (e) {
      setError(errorText(e));
    }
  };

  const changes = pending?.preview.items.filter((x) => x.change) ?? [];

  return (
    <div className="loja">
      {error && (
        <p role="alert" className="form-error">
          {error}
        </p>
      )}
      {note && <div className="notice">{note}</div>}
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
      {shown.length === 0 && <p className="muted">Nada encontrado.</p>}
      <ul className="loja-grid" aria-label="Itens da Loja">
        {shown.map((i) => {
          const working = busy === i.id;
          const pair = i.kind === "temas" ? [i.cursor, i.icons].map((id) => (id ? byId.get(id) : undefined)) : [];
          const themeLive = i.kind === "temas" && pair.length === 2 && pair.every((p) => p?.applied);
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
                {(i.applied || themeLive) && <span className="tweak-badge">Aplicado</span>}
                {i.remote && <span className="tweak-badge">Online</span>}
                {i.installed && i.kind !== "temas" && !i.applied && <span className="tweak-badge">Baixado</span>}
              </div>
              <p className="muted">{i.description}</p>
              {i.kind === "temas" && (
                <p className="muted">
                  {`Inclui: ${byId.get(i.cursor ?? "")?.name ?? "ponteiro"} + ${byId.get(i.icons ?? "")?.name ?? "ícones"}`}
                </p>
              )}
              <p className="muted loja-meta">
                {`${i.author} · ${i.license}${i.kind !== "temas" && i.bytes ? ` · ${size(i.bytes)}` : ""}`}
              </p>
              <div className="loja-actions">
                {i.kind === "temas" ? (
                  <>
                    <button type="button" className="btn btn-primary" disabled={busy !== null} onClick={() => void askTheme(i)} aria-label={`Aplicar tema ${i.name}`}>
                      {working ? "Aguarde…" : "Aplicar tema"}
                    </button>
                    <button type="button" className="btn" disabled={busy !== null} onClick={() => void undoTheme(i)} aria-label={`Desfazer tema ${i.name}`}>
                      Desfazer
                    </button>
                  </>
                ) : !i.installed ? (
                  <>
                    <button type="button" className="btn btn-primary" disabled={busy !== null} onClick={() => void install(i)} aria-label={`Baixar ${i.name}`}>
                      {working ? "Baixando…" : "Baixar"}
                    </button>
                    <button type="button" className="btn" disabled={busy !== null} onClick={() => void apply(i)} aria-label={`Baixar e aplicar ${i.name}`}>
                      Baixar e aplicar
                    </button>
                  </>
                ) : i.applied ? (
                  <button type="button" className="btn" disabled={busy !== null} onClick={() => void revert(i)} aria-label={`Reverter ${i.name}`}>
                    {working ? "Aguarde…" : "Reverter"}
                  </button>
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
        exatamente o que havia. Os itens do programa são originais do WON (CC0); créditos em
        docs/loja-creditos.md.
      </p>
      {pending && (
        <ConfirmDialog
          title={`Aplicar "${pending.item.name}"?`}
          confirmLabel="Aplicar tema"
          onConfirm={() => void confirmTheme()}
          onCancel={() => setPending(null)}
        >
          {changes.length === 0 && <p>Os ajustes do Windows já estão assim.</p>}
          <ul>
            {changes.map((x) => (
              <li key={`${x.kind}-${x.label}`}>{x.detail ? `${x.label}: ${x.detail}` : x.label}</li>
            ))}
            <li>{`Ponteiro: ${byId.get(pending.item.cursor ?? "")?.name ?? "—"}`}</li>
            <li>{`Ícones: ${byId.get(pending.item.icons ?? "")?.name ?? "—"}`}</li>
          </ul>
          <p className="muted">Dá para desfazer tudo de uma vez.</p>
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
