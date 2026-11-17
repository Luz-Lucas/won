import type { SystemStatus } from "../api";
import type { Page } from "../components/Sidebar";
import { humanDate } from "../format";

interface InicioProps {
  status: SystemStatus | null;
  error: string | null;
  onNavigate: (page: Page) => void;
}

function newestFetch(status: SystemStatus): string | null {
  const dates = status.db.sources.map((s) => s.fetched_at);
  if (!status.db.initialized || dates.length === 0) return null;
  return dates.reduce((a, b) => (new Date(a) >= new Date(b) ? a : b));
}

export default function Inicio({ status, error, onNavigate }: InicioProps) {
  const newest = status ? newestFetch(status) : null;
  return (
    <section className="page">
      <h1>Início</h1>
      <p className="warning">
        O Sentinela não substitui um antivírus completo. Mantenha o Microsoft
        Defender ativo.
      </p>
      {error && (
        <p role="alert" className="form-error">
          Não foi possível carregar o estado do sistema: {error}
        </p>
      )}
      {!status && !error && <p className="muted">Carregando…</p>}
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
              <h2>Sistema</h2>
              <p>{`Versão ${status.version}`}</p>
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
        </>
      )}
    </section>
  );
}
