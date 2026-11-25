import { useState } from "react";
import { updatesUpdateNow } from "../api";
import type { SystemStatus } from "../api";
import ConfirmDialog from "../components/ConfirmDialog";
import type { Page } from "../components/Sidebar";
import { MODE_LABEL } from "../components/WindowsUpdates";
import { humanDate } from "../format";

interface InicioProps {
  status: SystemStatus | null;
  error: string | null;
  onNavigate: (page: Page) => void;
  /** Depois de "Atualizar agora": recarrega o estado. */
  onChanged?: () => void;
}

/** "bloqueadas há 3 dias", "pausadas desde hoje". */
export function updatesNotice(mode: "blocked" | "paused", days: number | null): string {
  const word = mode === "blocked" ? "bloqueadas" : "pausadas";
  if (days === null) return `Atualizações do Windows ${word}.`;
  if (days === 0) return `Atualizações do Windows ${word} desde hoje.`;
  return `Atualizações do Windows ${word} há ${days} ${days === 1 ? "dia" : "dias"}.`;
}

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

function newestFetch(status: SystemStatus): string | null {
  const dates = status.db.sources.map((s) => s.fetched_at);
  if (!status.db.initialized || dates.length === 0) return null;
  return dates.reduce((a, b) => (new Date(a) >= new Date(b) ? a : b));
}

export default function Inicio({ status, error, onNavigate, onChanged }: InicioProps) {
  const newest = status ? newestFetch(status) : null;
  const [askUpdate, setAskUpdate] = useState(false);
  const [updateError, setUpdateError] = useState<string | null>(null);
  const [updating, setUpdating] = useState(false);
  const held = status?.updates_mode === "blocked" || status?.updates_mode === "paused";

  const updateNow = async () => {
    setAskUpdate(false);
    setUpdating(true);
    setUpdateError(null);
    try {
      await updatesUpdateNow();
      onChanged?.();
    } catch (e) {
      setUpdateError(errorText(e));
    } finally {
      setUpdating(false);
    }
  };
  return (
    <section className="page">
      <h1>Início</h1>
      <p className="warning">
        O WON não substitui um antivírus completo. Mantenha o Microsoft
        Defender ativo.
      </p>
      {error && (
        <p role="alert" className="form-error">
          Não foi possível carregar o estado do sistema: {error}
        </p>
      )}
      {!status && !error && <p className="muted">Carregando…</p>}
      {status && held && (
        <div className="notice" role="status">
          <p>
            {updatesNotice(status.updates_mode as "blocked" | "paused", status.updates_days)}{" "}
            O Windows não recebe correções de segurança nesse tempo.
          </p>
          <button
            type="button"
            className="btn"
            disabled={updating}
            onClick={() => setAskUpdate(true)}
          >
            Atualizar agora
          </button>
        </div>
      )}
      {status && !held && status.updates_suspended && (
        <div className="notice" role="status">
          <p>
            {`Atualizações liberadas temporariamente. Quando o Windows terminar de instalar, volte ao modo ${MODE_LABEL[status.updates_suspended]} em Ajustes.`}
          </p>
          <button type="button" className="btn" onClick={() => onNavigate("ajustes")}>
            Ir para Ajustes
          </button>
        </div>
      )}
      {updateError && (
        <p role="alert" className="form-error">
          {updateError}
        </p>
      )}
      {askUpdate && status && (
        <ConfirmDialog
          title="Liberar as atualizações agora?"
          confirmLabel="Atualizar agora"
          onConfirm={() => void updateNow()}
          onCancel={() => setAskUpdate(false)}
        >
          <p>
            {`O WON volta as atualizações ao padrão do Windows e abre o Windows Update. Quando a instalação terminar, volte ao modo ${MODE_LABEL[status.updates_mode]} em Ajustes.`}
          </p>
        </ConfirmDialog>
      )}
      {status && (
        <>
          <div className="cards">
            <article className="card">
              <h2>Proteção</h2>
              <p>
                {newest
                  ? `Banco atualizado em ${humanDate(newest)}`
                  : "Banco não inicializado"}
              </p>
              <p>{`${status.quarantine_count} na quarentena`}</p>
              <button
                type="button"
                className="btn"
                onClick={() => onNavigate("seguranca")}
              >
                Ir para Segurança
              </button>
            </article>
            <article className="card">
              <h2>Espaço</h2>
              <p>Libere espaço removendo arquivos temporários e caches.</p>
              <button
                type="button"
                className="btn"
                onClick={() => onNavigate("limpeza")}
              >
                Ir para Limpeza
              </button>
            </article>
            <article className="card">
              <h2>Otimização</h2>
              <p>{`${status.tweaks_applied} de ${status.tweaks_total}`}</p>
              <button
                type="button"
                className="btn"
                onClick={() => onNavigate("jogos")}
              >
                Ir para Otimização
              </button>
            </article>
          </div>
          <h2 className="section-title">Atividade recente</h2>
          {status.activity.length === 0 ? (
            <p className="muted">Nenhuma atividade ainda.</p>
          ) : (
            <ul className="activity">
              {status.activity.map((a, i) => (
                <li key={`${a.at}-${i}`}>
                  <span className="muted">{humanDate(a.at)}</span>{" "}
                  <span>{a.summary}</span>
                </li>
              ))}
            </ul>
          )}
          <p className="muted page-footer">{`WON ${status.version}`}</p>
        </>
      )}
    </section>
  );
}
