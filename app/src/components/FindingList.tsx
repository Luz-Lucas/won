import { revealItemInDir } from "@tauri-apps/plugin-opener";
import type { ScanItem, Severity } from "../api";

const LEVELS: { level: Severity; label: string }[] = [
  { level: "malicious", label: "Malicioso" },
  { level: "suspicious", label: "Suspeito" },
];

async function reveal(path: string) {
  try {
    await revealItemInDir(path);
  } catch {
    /* o arquivo pode ter sido movido: nada a fazer */
  }
}

/** Achados agrupados por nível (Malicioso antes de Suspeito). Nunca mexe nos arquivos. */
export default function FindingList({ items }: { items: ScanItem[] }) {
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
                    <span className="finding-path">{item.path}</span>
                    <button
                      type="button"
                      className="link-btn"
                      onClick={() => void reveal(item.path)}
                    >
                      Abrir pasta
                    </button>
                  </div>
                  <ul className="finding-rules">
                    {item.findings.map((f, i) => (
                      <li
                        key={`${f.detector}-${f.rule}-${i}`}
                        className="muted"
                      >
                        {`${f.detector}: ${f.rule}`}
                        {f.detail ? ` — ${f.detail}` : ""}
                      </li>
                    ))}
                  </ul>
                </li>
              ))}
            </ul>
          </section>
        );
      })}
    </div>
  );
}
