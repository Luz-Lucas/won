import { useCallback, useEffect, useState } from "react";
import { gameSessionEnd, gameSessionStart, gameSessionStatus } from "../api";
import type { GameSession } from "../api";
import ConfirmDialog from "./ConfirmDialog";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

/** Sessão de jogo: plano de energia de alto desempenho, desfeito ao encerrar. */
export default function GameSessionCard() {
  const [session, setSession] = useState<GameSession | null | undefined>(undefined);
  const [confirm, setConfirm] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(() => {
    gameSessionStatus().then(setSession, () => setSession(null));
  }, []);
  useEffect(load, [load]);

  const act = async (f: () => Promise<unknown>) => {
    setBusy(true);
    setError(null);
    try {
      await f();
      load();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  if (session === undefined) return null;

  return (
    <div className="game-session">
      <h2>Sessão de jogo</h2>
      {session ? (
        <>
          <p>
            Em andamento: o plano de energia de alto desempenho está ativo. Ao encerrar, o plano
            anterior volta.
          </p>
          <button type="button" className="btn" disabled={busy} onClick={() => void act(gameSessionEnd)}>
            Encerrar sessão de jogo
          </button>
        </>
      ) : (
        <>
          <p className="muted">
            Troca o plano de energia para o de alto desempenho e volta ao atual quando você
            encerrar. Nada fica alterado depois.
          </p>
          <button type="button" className="btn" disabled={busy} onClick={() => setConfirm(true)}>
            Iniciar sessão de jogo
          </button>
        </>
      )}
      {error && (
        <p role="alert" className="form-error">
          {error}
        </p>
      )}
      {confirm && (
        <ConfirmDialog
          title="Iniciar a sessão de jogo?"
          confirmLabel="Iniciar"
          onConfirm={() => {
            setConfirm(false);
            void act(gameSessionStart);
          }}
          onCancel={() => setConfirm(false)}
        >
          <p>O plano de energia do Windows passa para alto desempenho até você encerrar a sessão.</p>
        </ConfirmDialog>
      )}
    </div>
  );
}
