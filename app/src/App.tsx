import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  quitApp,
  scanRequestTake,
  systemStatus,
  windowReady,
  type SystemStatus,
} from "./api";
import ConfirmDialog from "./components/ConfirmDialog";
import Sidebar, { type Page } from "./components/Sidebar";
import { JobsProvider, useJobSettled } from "./jobs";
import Aplicativos from "./pages/Aplicativos";
import Ajustes from "./pages/Ajustes";
import Disco from "./pages/Disco";
import Historico from "./pages/Historico";
import Inicio from "./pages/Inicio";
import Jogos from "./pages/Jogos";
import Limpeza from "./pages/Limpeza";
import Mods from "./pages/Mods";
import Personalizar from "./pages/Personalizar";
import Rede from "./pages/Rede";
import Quarentena from "./pages/Quarentena";
import Seguranca, { type ExplorerRequest } from "./pages/Seguranca";
import "./styles/tokens.css";
import "./styles/app.css";

function Shell() {
  const [page, setPage] = useState<Page>("inicio");
  const [status, setStatus] = useState<SystemStatus | null>(null);
  const [statusError, setStatusError] = useState<string | null>(null);
  // Só a resposta do pedido mais recente vale (e nenhuma depois de desmontar).
  const request = useRef(0);

  const refreshStatus = useCallback(() => {
    const n = ++request.current;
    systemStatus().then(
      (s) => {
        if (n !== request.current) return;
        setStatus(s);
        setStatusError(null);
      },
      (e) => {
        if (n === request.current)
          setStatusError(typeof e === "string" ? e : String(e));
      },
    );
  }, []);

  // A janela nasce escondida; avisa o backend assim que a interface montou.
  // Efeito (não requestAnimationFrame): o WebView2 pausa rAF em página oculta.
  useEffect(() => {
    windowReady().catch(() => {
      /* sem backend (navegador): nada a fazer */
    });
  }, []);

  useEffect(
    () => () => {
      request.current++;
    },
    [],
  );

  // "Sair" com uma fila de aplicativos rodando: o backend pede confirmação.
  const [askQuit, setAskQuit] = useState(false);
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void listen("app:quit-confirm", () => setAskQuit(true)).then((un) => {
      if (disposed) un();
      else unlisten = un;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  // "Verificar com o WON" no Explorador: abre a Segurança e verifica o item.
  const [explorerRequest, setExplorerRequest] = useState<ExplorerRequest | null>(null);
  const requestId = useRef(0);
  useEffect(() => {
    const take = () => {
      scanRequestTake().then(
        (paths) => {
          if (!Array.isArray(paths) || paths.length === 0) return;
          setExplorerRequest({ id: ++requestId.current, paths });
          setPage("seguranca");
        },
        () => {
          /* sem backend (navegador): nada a fazer */
        },
      );
    };
    take();
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void listen("scan:request", take).then((un) => {
      if (disposed) un();
      else unlisten = un;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  // O Início sempre mostra dados atuais: ao abrir e depois de cada operação.
  useEffect(() => {
    if (page === "inicio") refreshStatus();
  }, [page, refreshStatus]);
  useJobSettled(refreshStatus);

  return (
    <div className="shell">
      <Sidebar current={page} onNavigate={setPage} elevated={status?.elevated} />
      <main className="content" key={page}>
        {page === "inicio" && (
          <Inicio
            status={status}
            error={statusError}
            onNavigate={setPage}
            onChanged={refreshStatus}
          />
        )}
        {page === "jogos" && <Jogos />}
        {page === "limpeza" && <Limpeza />}
        {page === "mods" && <Mods />}
        {page === "aplicativos" && <Aplicativos />}
        {page === "personalizar" && <Personalizar onNavigate={setPage} />}
        {page === "rede" && <Rede />}
        {page === "seguranca" && (
          <Seguranca
            onNavigate={setPage}
            explorerRequest={explorerRequest}
            elevated={status?.elevated}
          />
        )}
        {page === "quarentena" && <Quarentena onRestored={refreshStatus} />}
        {page === "disco" && <Disco />}
        {page === "historico" && <Historico />}
        {page === "ajustes" && <Ajustes />}
      </main>
      {askQuit && (
        <ConfirmDialog
          title="Sair do WON?"
          confirmLabel="Sair"
          cancelLabel="Continuar"
          onConfirm={() => {
            setAskQuit(false);
            quitApp().catch(() => {
              /* sem backend: nada a fazer */
            });
          }}
          onCancel={() => setAskQuit(false)}
        >
          <p>
            Um aplicativo está sendo instalado. Ele vai terminar em segundo plano e os próximos da
            fila não serão instalados. Sair mesmo assim?
          </p>
        </ConfirmDialog>
      )}
    </div>
  );
}

/** As operações longas vivem acima das páginas: trocar de página não as perde. */
export default function App() {
  return (
    <JobsProvider>
      <Shell />
    </JobsProvider>
  );
}
