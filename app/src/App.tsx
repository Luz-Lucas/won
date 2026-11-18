import { useCallback, useEffect, useRef, useState } from "react";
import { systemStatus, type SystemStatus } from "./api";
import Sidebar, { type Page } from "./components/Sidebar";
import { JobsProvider, useJobSettled } from "./jobs";
import Ajustes from "./pages/Ajustes";
import Disco from "./pages/Disco";
import Inicio from "./pages/Inicio";
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
        {page === "limpeza" && <Limpeza />}
        {page === "seguranca" && (
          <Seguranca onNavigate={setPage} onRestored={refreshStatus} />
        )}
        {page === "disco" && <Disco />}
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
