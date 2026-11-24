import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
import { useState } from "react";
import type { ScanItem, Severity } from "../api";

const LEVELS: { level: Severity; label: string }[] = [
  { level: "malicious", label: "Malicioso" },
  { level: "suspicious", label: "Suspeito" },
];

export const virusTotalUrl = (sha256: string) => `https://www.virustotal.com/gui/file/${sha256}`;

async function reveal(path: string) {
  try {
    await revealItemInDir(path);
  } catch {
    /* o arquivo pode ter sido movido: nada a fazer */
  }
}

async function virusTotal(sha256: string) {
  try {
    await openUrl(virusTotalUrl(sha256));
  } catch {
    /* sem navegador: nada a fazer */
  }
}

interface Props {
  items: ScanItem[];
  /** Caminhos marcados para quarentena. */
  selected: Set<string>;
  onToggle: (path: string) => void;
  onIgnore: (item: ScanItem) => void;
}

/**
 * Achados agrupados por nível (Malicioso antes de Suspeito), com o motivo em
 * linguagem simples e as ações por achado. Nada aqui move arquivos: marcar só
 * escolhe o que vai para a quarentena quando o usuário pedir.
 */
export default function FindingList({ items, selected, onToggle, onIgnore }: Props) {
  const [open, setOpen] = useState<Set<string>>(new Set());
  const toggleOpen = (path: string) =>
    setOpen((s) => {
      const n = new Set(s);
      if (!n.delete(path)) n.add(path);
      return n;
    });

  return (
    <div className="findings">
      {LEVELS.map(({ level, label }) => {
        const group = items.filter((i) => i.verdict === level);
        if (group.length === 0) return null;
        return (
          <section key={level} className={`finding-group finding-${level}`}>
            <h3>{`${label} (${group.length})`}</h3>
            <ul className="finding-list">
              {group.map((item) => (
                <li key={item.path} className="finding">
                  <div className="finding-head">
                    <label className="finding-check">
                      <input
                        type="checkbox"
                        aria-label={`Selecionar ${item.path}`}
                        checked={selected.has(item.path)}
                        onChange={() => onToggle(item.path)}
                      />
                      <span className="finding-path">{item.path}</span>
                    </label>
                    {item.recommended && <span className="badge">Quarentena recomendada</span>}
                  </div>
                  <ul className="finding-rules">
                    {item.why.map((w, i) => (
                      <li key={`${i}-${w}`}>{w}</li>
                    ))}
                  </ul>
                  <div className="finding-actions">
                    <button
                      type="button"
                      className="link-btn"
                      aria-expanded={open.has(item.path)}
                      onClick={() => toggleOpen(item.path)}
                    >
                      Detalhes
                    </button>
                    <button
                      type="button"
                      className="link-btn"
                      onClick={() => void reveal(item.path)}
                    >
                      Abrir pasta
                    </button>
                    <button
                      type="button"
                      className="link-btn"
                      onClick={() => void virusTotal(item.sha256)}
                    >
                      Consultar no VirusTotal
                    </button>
                    <button type="button" className="link-btn" onClick={() => onIgnore(item)}>
                      Ignorar sempre
                    </button>
                  </div>
                  {open.has(item.path) && (
                    <div className="finding-details">
                      <p className="muted">{`SHA-256: ${item.sha256}`}</p>
                      <ul className="finding-rules">
                        {item.findings.map((f, i) => (
                          <li key={`${f.detector}-${f.rule}-${i}`} className="muted">
                            {`${f.detector}: ${f.rule}`}
                            {f.detail ? ` — ${f.detail}` : ""}
                          </li>
                        ))}
                      </ul>
                      <p className="muted">
                        O VirusTotal abre no navegador só com o hash; nenhum arquivo é enviado.
                      </p>
                    </div>
                  )}
                </li>
              ))}
            </ul>
          </section>
        );
      })}
    </div>
  );
}
