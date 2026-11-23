import { humanBytes } from "../format";
import type { JobProgress } from "../jobs";

/** Progresso de uma verificação: porcentagem por bytes, arquivos e arquivo atual. */
export default function ProgressBar({
  progress,
}: {
  progress: JobProgress | null;
}) {
  if (!progress || progress.phase === "counting") {
    return (
      <p className="muted" role="status">
        {progress ? `Contando arquivos… ${progress.files_seen}` : "Iniciando…"}
      </p>
    );
  }
  if (progress.phase === "apps") {
    const pct = progress.total > 0 ? Math.floor((progress.index / progress.total) * 100) : 0;
    return (
      <div className="progress">
        <div
          className="progress-track"
          role="progressbar"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={pct}
        >
          <div className="progress-fill" style={{ width: `${pct}%` }} />
        </div>
        <p className="muted progress-current">{`${progress.index + 1} de ${progress.total} · ${progress.current}`}</p>
      </div>
    );
  }
  const ratio =
    progress.total_bytes > 0
      ? progress.bytes_done / progress.total_bytes
      : progress.total_files > 0
        ? progress.files_done / progress.total_files
        : 0;
  const pct = Math.min(100, Math.floor(ratio * 100));
  return (
    <div className="progress">
      <div
        className="progress-track"
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={pct}
      >
        <div className="progress-fill" style={{ width: `${pct}%` }} />
      </div>
      <p className="muted">
        {`${pct}% · ${progress.files_done} de ${progress.total_files} arquivos · ${humanBytes(progress.bytes_done)} de ${humanBytes(progress.total_bytes)}`}
      </p>
      <p className="muted progress-current">{progress.current}</p>
    </div>
  );
}
