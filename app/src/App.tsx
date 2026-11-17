import { useEffect, useState } from "react";
import { systemStatus, type SystemStatus } from "./api";
import Sidebar, { type Page } from "./components/Sidebar";
import Ajustes from "./pages/Ajustes";
import Disco from "./pages/Disco";
import Inicio from "./pages/Inicio";
import Limpeza from "./pages/Limpeza";
import Seguranca from "./pages/Seguranca";
import "./styles/tokens.css";
import "./styles/app.css";

export default function App() {
  const [page, setPage] = useState<Page>("inicio");
  const [status, setStatus] = useState<SystemStatus | null>(null);
  const [statusError, setStatusError] = useState<string | null>(null);

  useEffect(() => {
    let disposed = false;
    systemStatus().then(
      (s) => {
        if (!disposed) setStatus(s);
      },
      (e) => {
        if (!disposed) setStatusError(typeof e === "string" ? e : String(e));
      },
    );
    return () => {
      disposed = true;
    };
  }, []);

  return (
    <div className="shell">
      <Sidebar current={page} onNavigate={setPage} elevated={status?.elevated} />
      <main className="content">
        {page === "inicio" && (
          <Inicio status={status} error={statusError} onNavigate={setPage} />
        )}
        {page === "limpeza" && <Limpeza />}
        {page === "seguranca" && <Seguranca />}
        {page === "disco" && <Disco />}
        {page === "ajustes" && <Ajustes />}
      </main>
    </div>
  );
}
