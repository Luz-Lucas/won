import { useState } from "react";
import { WonMark } from "./WonMark";
import { relaunchElevated } from "../api";

export type Page =
  | "inicio"
  | "jogos"
  | "limpeza"
  | "aplicativos"
  | "personalizar"
  | "rede"
  | "seguranca"
  | "quarentena"
  | "disco"
  | "historico"
  | "ajustes"
  | "desempenho"
  | "mods";

/** Grupos da barra lateral: o que o usuário quer fazer, não o que o programa tem. */
const GROUPS: { title: string | null; items: { page: Page; label: string }[] }[] = [
  { title: null, items: [{ page: "inicio", label: "Início" }] },
  {
    title: "Desempenho",
    items: [
      { page: "jogos", label: "Otimização" },
      { page: "limpeza", label: "Limpeza" },
      { page: "disco", label: "Disco" },
      { page: "aplicativos", label: "Aplicativos" },
      { page: "desempenho", label: "Monitor" },
    ],
  },
  { title: "Conexão", items: [{ page: "rede", label: "Rede" }] },
  {
    title: "Proteção",
    items: [
      { page: "seguranca", label: "Segurança" },
      { page: "quarentena", label: "Quarentena" },
    ],
  },
  {
    title: "Aparência",
    items: [
      { page: "personalizar", label: "Estilização" },
      { page: "mods", label: "Mods" },
    ],
  },
  {
    title: "Sistema",
    items: [
      { page: "historico", label: "Histórico" },
      { page: "ajustes", label: "Ajustes" },
    ],
  },
];

interface SidebarProps {
  current: Page;
  onNavigate: (page: Page) => void;
  /** `undefined` enquanto o estado do sistema não foi carregado. */
  elevated: boolean | undefined;
}

export default function Sidebar({ current, onNavigate, elevated }: SidebarProps) {
  const [relaunchError, setRelaunchError] = useState<string | null>(null);

  const relaunch = async () => {
    setRelaunchError(null);
    try {
      await relaunchElevated();
    } catch (e) {
      setRelaunchError(typeof e === "string" ? e : String(e));
    }
  };

  return (
    <nav className="sidebar" aria-label="Principal">
      <div className="brand">
        <WonMark size={20} />
        <span className="brand-name">WON</span>
      </div>
      <div className="nav-groups">
        {GROUPS.map((g) => (
          <div key={g.title ?? "geral"} className="nav-group">
            {g.title && (
              <div className="nav-group-title" aria-hidden="true">
                {g.title}
              </div>
            )}
            <ul className="nav">
              {g.items.map(({ page, label }) => (
                <li key={page}>
                  <button
                    type="button"
                    className="nav-item"
                    aria-current={page === current ? "page" : undefined}
                    onClick={() => onNavigate(page)}
                  >
                    {label}
                  </button>
                </li>
              ))}
            </ul>
          </div>
        ))}
      </div>
      <footer className="sidebar-footer" data-elevated={elevated}>
        <div>
          Administrador:{" "}
          {elevated === undefined ? "—" : elevated ? "sim" : "não"}
        </div>
        {elevated === false && (
          <button type="button" className="btn" onClick={() => void relaunch()}>
            Reiniciar como administrador
          </button>
        )}
        {relaunchError && (
          <p role="alert" className="form-error">
            {relaunchError}
          </p>
        )}
      </footer>
    </nav>
  );
}
