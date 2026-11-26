import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useCallback, useEffect, useState } from "react";
import {
  modsEject,
  modsEjectAll,
  modsInject,
  modsRecoveryWrite,
  modsStatus,
} from "../api";
import type { ModsStatus } from "../api";
import ShellMods from "../components/ShellMods";
import ConfirmDialog from "../components/ConfirmDialog";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

export default function Mods() {
  const [status, setStatus] = useState<ModsStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [pending, setPending] = useState<string | null>(null);

  const load = useCallback(() => {
    modsStatus().then(setStatus, (e) => setError(errorText(e)));
  }, []);
  useEffect(load, [load]);

  const act = async (f: () => Promise<unknown>, done?: string) => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await f();
      if (done) setNotice(done);
      load();
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const reveal = async (path: string) => {
    try {
      await revealItemInDir(path);
    } catch {
      /* a pasta pode ter sido movida: nada a fazer */
    }
  };

  const recovery = () =>
    act(async () => {
      const path = await modsRecoveryWrite();
      await reveal(path);
    }, "Script de recuperação gravado.");

  const anyLoaded = status?.mods.some((m) => m.loaded.length > 0) ?? false;

  return (
    <section>
      <h1>Mods</h1>
      <p className="muted">
        Mods são DLLs suas, carregadas só nos programas que você listar. O WON nunca carrega
        nada no Explorer, em processos do sistema nem com um anticheat aberto, e tudo é desfeito
        pelo script de recuperação, mesmo sem o app abrir.
      </p>

      {status && !status.installed && (
        <p role="alert" className="form-error">
          O motor de mods (sentinela-mods.exe) não foi encontrado ao lado do WON.
        </p>
      )}
      {status?.anticheat_running && (
        <p role="alert" className="form-error">
          Há um anticheat aberto: nenhum mod será carregado enquanto ele estiver rodando.
        </p>
      )}
      {status?.registry_error && (
        <p role="alert" className="form-error">
          {`Não foi possível ler o mods.toml: ${status.registry_error}`}
        </p>
      )}

      <ShellMods />

      <div className="game-session">
        <h2>Recuperação</h2>
        <p className="muted">
          O script desfazer-mods.cmd descarrega tudo, tira os mods da inicialização e desativa o
          cadastro. Rode-o se um jogo ou anticheat reclamar, ou no modo seguro.
        </p>
        <div className="row">
          <button type="button" className="btn" disabled={busy} onClick={() => void recovery()}>
            Gravar e mostrar o script de recuperação
          </button>
          {status?.recovery_path && (
            <button type="button" className="btn" onClick={() => void reveal(status.recovery_path!)}>
              Abrir pasta de recuperação
            </button>
          )}
          <button
            type="button"
            className="btn"
            disabled={busy || !anyLoaded}
            onClick={() => void act(modsEjectAll, "Todos os mods foram descarregados.")}
          >
            Descarregar todos
          </button>
        </div>
      </div>

      <h2>Mods cadastrados</h2>
      {status && status.mods.length === 0 && (
        <p className="muted">
          Nenhum mod cadastrado. Crie o arquivo abaixo (veja docs/testes-manuais-fase-11.md).
        </p>
      )}
      {status && (
        <p className="muted">
          Cadastro:{" "}
          <button type="button" className="link-btn" onClick={() => void reveal(status.registry_path)}>
            {status.registry_path}
          </button>
        </p>
      )}
      <ul className="list">
        {status?.mods.map((m) => (
          <li key={m.name}>
            <strong>{m.name}</strong>
            <div className="muted">{m.dll}</div>
            <div className="muted">
              {m.targets.length > 0
                ? `Programas permitidos: ${m.targets.join(", ")}`
                : "Sem programas permitidos: nada será carregado."}
            </div>
            {!m.dll_exists && <div className="form-error">DLL não encontrada.</div>}
            {m.loaded.length > 0 && (
              <div>{`Carregado em: ${m.loaded.map((l) => `${l.process} (${l.pid})`).join(", ")}`}</div>
            )}
            <div className="row">
              <button
                type="button"
                className="btn"
                disabled={
                  busy || !status.installed || status.anticheat_running || !m.enabled || !m.dll_exists
                }
                onClick={() => setPending(m.name)}
              >
                {`Carregar ${m.name}`}
              </button>
              <button
                type="button"
                className="btn"
                disabled={busy || m.loaded.length === 0}
                onClick={() => void act(() => modsEject(m.name), `${m.name} foi descarregado.`)}
              >
                {`Descarregar ${m.name}`}
              </button>
            </div>
          </li>
        ))}
      </ul>

      {notice && <p role="status">{notice}</p>}
      {error && (
        <p role="alert" className="form-error">
          {error}
        </p>
      )}

      {pending && (
        <ConfirmDialog
          title={`Carregar ${pending}?`}
          confirmLabel="Carregar"
          onConfirm={() => {
            const name = pending;
            setPending(null);
            void act(async () => {
              const n = await modsInject(name);
              setNotice(n === 0 ? "Nenhum programa permitido está aberto." : `Carregado em ${n} programa(s).`);
            });
          }}
          onCancel={() => setPending(null)}
        >
          <p>
            A DLL será carregada nos programas permitidos que estiverem abertos. O script de
            recuperação é gravado antes. Só carregue DLLs em que você confia.
          </p>
        </ConfirmDialog>
      )}
    </section>
  );
}
