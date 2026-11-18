import { useCallback, useEffect, useRef, useState } from "react";
import { systemStatus, windowReady, type SystemStatus } from "./api";
import Sidebar, { type Page } from "./components/Sidebar";
import { JobsProvider, useJobSettled } from "./jobs";
import Ajustes from "./pages/Ajustes";
import Disco from "./pages/Disco";
import Historico from "./pages/Historico";
import Inicio from "./pages/Inicio";
import Jogos from "./pages/Jogos";
import Limpeza from "./pages/Limpeza";
import Seguranca from "./pages/Seguranca";
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

  // O Início sempre mostra dados atuais: ao abrir e depois de cada operação.
  useEffect(() => {
    if (page === "inicio") refreshStatus();
  }, [page, refreshStatus]);
  useJobSettled(refreshStatus);

  return (
    <div className="shell">
      <Sidebar current={page} onNavigate={setPage} elevated={status?.elevated} />
      <main className="content">
        {page === "inicio" && (
          <Inicio status={status} error={statusError} onNavigate={setPage} />
        )}
        {page === "jogos" && <Jogos />}
        {page === "limpeza" && <Limpeza />}
        {page === "seguranca" && (
          <Seguranca onNavigate={setPage} onRestored={refreshStatus} />
        )}
        {page === "disco" && <Disco />}
        {page === "historico" && <Historico />}
        {page === "ajustes" && <Ajustes />}
      </main>
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
