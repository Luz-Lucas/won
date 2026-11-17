import { useState } from "react";
import Sidebar, { type Page } from "./components/Sidebar";
import Ajustes from "./pages/Ajustes";
import Disco from "./pages/Disco";
import Inicio from "./pages/Inicio";
import Limpeza from "./pages/Limpeza";
import Seguranca from "./pages/Seguranca";
import "./styles/tokens.css";
import "./styles/app.css";

const PAGES: Record<Page, () => React.JSX.Element> = {
  inicio: Inicio,
  limpeza: Limpeza,
  seguranca: Seguranca,
  disco: Disco,
  ajustes: Ajustes,
};

export default function App() {
  const [page, setPage] = useState<Page>("inicio");
  const Current = PAGES[page];
  return (
    <div className="shell">
      <Sidebar current={page} onNavigate={setPage} elevated={false} />
      <main className="content">
        <Current />
      </main>
    </div>
  );
}
