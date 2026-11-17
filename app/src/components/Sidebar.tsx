export type Page = "inicio" | "limpeza" | "seguranca" | "disco" | "ajustes";

const ITEMS: { page: Page; label: string }[] = [
  { page: "inicio", label: "Início" },
  { page: "limpeza", label: "Limpeza" },
  { page: "seguranca", label: "Segurança" },
  { page: "disco", label: "Disco" },
  { page: "ajustes", label: "Ajustes" },
];

interface SidebarProps {
  current: Page;
  onNavigate: (page: Page) => void;
  elevated: boolean;
}

export default function Sidebar({ current, onNavigate, elevated }: SidebarProps) {
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
        {elevated ? "Modo administrador" : "Modo padrão"}
      </footer>
    </nav>
  );
}
