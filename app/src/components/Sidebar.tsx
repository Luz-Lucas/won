import { useState } from "react";
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
  | "ajustes";

const ITEMS: { page: Page; label: string }[] = [
  { page: "inicio", label: "Início" },
  { page: "jogos", label: "Jogos" },
  { page: "limpeza", label: "Limpeza" },
  { page: "aplicativos", label: "Aplicativos" },
  { page: "personalizar", label: "Personalizar" },
  { page: "rede", label: "Rede" },
  { page: "seguranca", label: "Segurança" },
  { page: "quarentena", label: "Quarentena" },
  { page: "disco", label: "Disco" },
  { page: "historico", label: "Histórico" },
  { page: "ajustes", label: "Ajustes" },
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
        <span className="brand-mark" aria-hidden="true" />
        <span className="brand-name">Sentinela</span>
      </div>
      <ul className="nav">
        {ITEMS.map(({ page, label }) => (
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
