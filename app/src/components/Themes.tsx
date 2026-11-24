import { useCallback, useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  themeApply,
  themeExport,
  themeImport,
  themeList,
  themePreview,
  themeUndo,
} from "../api";
import type {
  ImportedTheme,
  ThemeApplied,
  ThemeInfo,
  ThemePreview,
  ThemeRecord,
  ThemeUndone,
} from "../api";
import ConfirmDialog from "./ConfirmDialog";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

const EFFECT_NOTICE: Record<string, string> = {
  explorer: "Reinicie o Explorer para ver tudo.",
  relogon: "Saia e entre de novo no Windows para a cor de destaque valer em tudo.",
  reboot: "Reinicie o computador para concluir.",
};

interface Pending {
  theme: ThemeInfo;
  preview: ThemePreview;
}

/** Temas prontos, importados e o histórico de um passo só. */
export default function Themes({ engine }: { engine: boolean }) {
  const [themes, setThemes] = useState<ThemeInfo[]>([]);
  const [history, setHistory] = useState<ThemeRecord[]>([]);
  const [imported, setImported] = useState<ThemeInfo[]>([]);
  const [importNote, setImportNote] = useState<ImportedTheme | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);
  const [done, setDone] = useState<ThemeApplied | ThemeUndone | null>(null);
  const [exportName, setExportName] = useState("");
  const [exported, setExported] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(() => {
    themeList().then(
      (p) => {
        setThemes(p.themes);
        setHistory(p.history);
      },
      (e) => setError(errorText(e)),
    );
  }, []);

  useEffect(load, [load]);

  // O tema só aparece depois da prévia: nada é aplicado sem ela.
  const ask = async (theme: ThemeInfo) => {
    setError(null);
    setDone(null);
    try {
      setPending({ theme, preview: await themePreview(theme.toml) });
    } catch (e) {
      setError(errorText(e));
    }
  };

  const confirm = async () => {
    const p = pending;
    setPending(null);
    if (!p) return;
    setBusy(true);
    try {
      setDone(await themeApply(p.theme.toml));
      load();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const undo = async () => {
    setBusy(true);
    setError(null);
    try {
      setDone(await themeUndo());
      load();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const importFile = async () => {
    setError(null);
    setImportNote(null);
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Tema do Sentinela", extensions: ["sentinela-theme"] }],
    });
    if (typeof path !== "string") return;
    try {
      const t = await themeImport(path);
      setImportNote(t);
      setImported((cur) => [
        ...cur.filter((c) => c.name !== t.name),
        { name: t.name, description: t.description, toml: t.toml, builtin: false },
      ]);
    } catch (e) {
      setError(errorText(e));
    }
  };

  const exportFile = async () => {
    setError(null);
    setExported(null);
    const path = await save({
      defaultPath: `${exportName.trim()}.sentinela-theme`,
      filters: [{ name: "Tema do Sentinela", extensions: ["sentinela-theme"] }],
    });
    if (typeof path !== "string") return;
    try {
      await themeExport(exportName.trim(), path);
      setExported(path);
    } catch (e) {
      setError(errorText(e));
    }
  };

  const last = history[history.length - 1];
  const changes = pending?.preview.items.filter((i) => i.change) ?? [];
  const unchanged = pending?.preview.items.filter((i) => !i.change) ?? [];

  const notice = done && "effect" in done ? EFFECT_NOTICE[done.effect] : undefined;
  const failures = done?.results.filter((r) => !r.ok) ?? [];

  return (
    <div>
      {error && (
        <p role="alert" className="form-error">
          {error}
        </p>
      )}
      {done && (
        <div className="notice">
          {"record" in done && (
            <p>{done.record ? `Tema aplicado: ${done.record.theme}.` : "Nada mudou: tudo já estava assim."}</p>
          )}
          {"remaining" in done && <p>{done.remaining ? "Desfeito em parte." : "Tema desfeito."}</p>}
          {notice && <p>{notice}</p>}
          {done.warnings.map((w) => (
            <p key={w}>{w}</p>
          ))}
          {failures.map((r) => (
            <p key={r.tweak_id} className="form-error">{`${r.tweak_id}: ${r.message}`}</p>
          ))}
          {"conflicts" in done && done.conflicts.map((c) => <p key={c}>{c}</p>)}
        </div>
      )}
      {last && (
        <div className="notice">
          <p>{`Último tema aplicado: ${last.theme}.`}</p>
          <button type="button" className="btn" disabled={busy} onClick={() => void undo()}>
            Desfazer tema
          </button>
        </div>
      )}
      <ul className="clean-list">
        {[...themes, ...imported].map((t) => (
          <li key={`${t.builtin ? "b" : "i"}-${t.name}`} className="clean-row">
            <div className="clean-head">
              <strong>{t.name}</strong>
              {!t.builtin && <span className="tweak-badge">Importado</span>}
              {t.applied && <span className="tweak-badge">Aplicado</span>}
            </div>
            <p className="muted">{t.description}</p>
            <button
              type="button"
              className="btn"
              disabled={busy}
              onClick={() => void ask(t)}
              aria-label={`Ver mudanças de ${t.name}`}
            >
              Ver mudanças
            </button>
          </li>
        ))}
      </ul>
      <h2>Importar e exportar</h2>
      <p className="muted">
        Um arquivo de tema só traz a lista de ajustes, o wallpaper (pelo número da Oficina) e a cor.
        Nada que execute programas.
      </p>
      <button type="button" className="btn" onClick={() => void importFile()}>
        Importar tema…
      </button>
      {importNote && (
        <div className="notice">
          <p>{`"${importNote.name}" foi lido e está na lista acima.`}</p>
          {importNote.disabled.map((d) => (
            <p key={d} className="clean-warning">{`Desligado: ${d}`}</p>
          ))}
          {importNote.warnings.map((w) => (
            <p key={w}>{w}</p>
          ))}
        </div>
      )}
      <div className="clean-head">
        <label>
          Nome do tema atual
          <input
            type="text"
            value={exportName}
            onChange={(e) => setExportName(e.target.value)}
          />
        </label>
        <button
          type="button"
          className="btn"
          disabled={exportName.trim() === ""}
          onClick={() => void exportFile()}
        >
          Exportar o visual atual…
        </button>
      </div>
      {exported && <p className="muted">{`Salvo em ${exported}.`}</p>}
      {!engine && (
        <p className="muted">
          Sem o Wallpaper Engine, os temas mudam só os ajustes do Windows e a cor.
        </p>
      )}
      {pending && (
        <ConfirmDialog
          title={`Aplicar "${pending.theme.name}"?`}
          confirmLabel="Aplicar tema"
          onConfirm={() => void confirm()}
          onCancel={() => setPending(null)}
        >
          {changes.length === 0 && <p>Nada a mudar: tudo já está assim.</p>}
          <ul>
            {changes.map((i) => (
              <li key={`${i.kind}-${i.label}`}>{i.detail ? `${i.label}: ${i.detail}` : i.label}</li>
            ))}
          </ul>
          {unchanged.length > 0 && (
            <p className="muted">{`${unchanged.length} item(ns) já estão assim ou não valem nesta máquina.`}</p>
          )}
          {pending.preview.warnings.map((w) => (
            <p key={w} className="clean-warning">{w}</p>
          ))}
          <p className="muted">Você pode desfazer o tema inteiro de uma vez.</p>
        </ConfirmDialog>
      )}
    </div>
  );
}
