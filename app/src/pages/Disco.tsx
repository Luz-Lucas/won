import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { useEffect, useState } from "react";
import {
  diskFindDupes,
  diskFindLarge,
  type DupGroup,
  type FindResult,
  type LargeFile,
} from "../api";
import { humanBytes } from "../format";
import { useJob } from "../jobs";

type Tab = "large" | "dupes";

const number = new Intl.NumberFormat("pt-BR");
const INPUT_ERROR = "Informe apenas números inteiros não negativos.";
const TOP_ERROR = "Quantidade deve ser um inteiro entre 1 e 500.";

async function reveal(path: string) {
  try {
    await revealItemInDir(path);
  } catch {
    /* o arquivo pode ter sido movido: nada a fazer */
  }
}

/** Inteiro não negativo ou null. */
function parseCount(text: string): number | null {
  if (text.trim() === "") return null;
  const n = Number(text);
  return Number.isSafeInteger(n) && n >= 0 ? n : null;
}

const wasted = (groups: DupGroup[]) =>
  groups.reduce((sum, g) => sum + g.size * (g.paths.length - 1), 0);

function Unverified({ result }: { result: FindResult<unknown> }) {
  const [show, setShow] = useState(false);
  // O backend manda no máximo 200 erros e o total em `errors_total`.
  if (result.errors_total === 0) return null;
  return (
    <div className="scan-errors">
      <button
        type="button"
        className="link-btn"
        aria-expanded={show}
        onClick={() => setShow((v) => !v)}
      >
        {`Não verificados (${number.format(result.errors_total)})`}
      </button>
      {show && (
        <ul className="sample">
          {result.errors.map((e, i) => (
            <li key={`${e.path}-${i}`}>
              <span>{e.path}</span> <span className="muted">{e.reason}</span>
            </li>
          ))}
          {result.errors_total > result.errors.length && (
            <li className="muted">
              {`Mostrando ${number.format(result.errors.length)} de ${number.format(result.errors_total)}`}
            </li>
          )}
        </ul>
      )}
    </div>
  );
}

function PathRow({ path, size }: { path: string; size?: number }) {
  return (
    <li className="finding">
      <div className="finding-head">
        <span className="finding-path">{path}</span>
        {size !== undefined && (
          <span className="muted">{humanBytes(size)}</span>
        )}
        <button
          type="button"
          className="link-btn"
          onClick={() => void reveal(path)}
        >
          Abrir pasta
        </button>
      </div>
    </li>
  );
}

/** Lista os maiores arquivos e os duplicados de uma pasta. Nunca mexe nos arquivos. */
export default function Disco() {
  const largeJob = useJob<FindResult<LargeFile>>("find_large");
  const dupesJob = useJob<FindResult<DupGroup>>("find_dupes");
  const [tab, setTab] = useState<Tab>("large");
  const [folder, setFolder] = useState<string | null>(null);
  const [picking, setPicking] = useState(false);
  const [top, setTop] = useState("20");
  const [minMb, setMinMb] = useState("0");
  const [minKb, setMinKb] = useState("1");
  const [inputError, setInputError] = useState<string | null>(null);
  const [cancelling, setCancelling] = useState(false);

  const job = tab === "large" ? largeJob : dupesJob;
  const running = largeJob.state === "running" || dupesJob.state === "running";
  const busy = running || picking;

  useEffect(() => {
    if (!running) setCancelling(false);
  }, [running]);

  const pickFolder = async () => {
    setPicking(true);
    try {
      const dir = await open({ directory: true });
      if (typeof dir === "string" && dir) setFolder(dir);
    } catch {
      /* seletor fechado ou indisponível: mantém a pasta atual */
    } finally {
      setPicking(false);
    }
  };

  const search = () => {
    if (!folder) return;
    if (tab === "large") {
      const t = parseCount(top);
      const m = parseCount(minMb);
      if (t === null || t < 1 || t > 500) return setInputError(TOP_ERROR);
      if (m === null) return setInputError(INPUT_ERROR);
      setInputError(null);
      void largeJob.start(() => diskFindLarge(folder, t, m));
    } else {
      const k = parseCount(minKb);
      if (k === null) return setInputError(INPUT_ERROR);
      setInputError(null);
      void dupesJob.start(() => diskFindDupes(folder, k));
    }
  };

  const cancel = () => {
    setCancelling(true);
    const active = largeJob.state === "running" ? largeJob : dupesJob;
    active.cancel().catch(() => setCancelling(false));
  };

  const large = largeJob.state === "done" ? largeJob.result : null;
  const dupes = dupesJob.state === "done" ? dupesJob.result : null;
  const result = tab === "large" ? large : dupes;

  return (
    <section className="page">
      <h1>Disco</h1>
      <div className="disk-tabs" role="tablist">
        <button
          type="button"
          role="tab"
          className="btn"
          aria-selected={tab === "large"}
          disabled={busy}
          onClick={() => {
            setTab("large");
            setInputError(null);
          }}
        >
          Maiores arquivos
        </button>
        <button
          type="button"
          role="tab"
          className="btn"
          aria-selected={tab === "dupes"}
          disabled={busy}
          onClick={() => {
            setTab("dupes");
            setInputError(null);
          }}
        >
          Duplicados
        </button>
      </div>

      <div className="disk-form">
        <button
          type="button"
          className="btn"
          disabled={busy}
          onClick={() => void pickFolder()}
        >
          Escolher pasta…
        </button>
        {tab === "large" ? (
          <>
            <label>
              Quantidade
              <input
                type="number"
                min={1}
                max={500}
                value={top}
                disabled={busy}
                onChange={(e) => {
                  setTop(e.target.value);
                  setInputError(null);
                }}
              />
            </label>
            <label>
              Tamanho mínimo (MB)
              <input
                type="number"
                min={0}
                value={minMb}
                disabled={busy}
                onChange={(e) => {
                  setMinMb(e.target.value);
                  setInputError(null);
                }}
              />
            </label>
          </>
        ) : (
          <label>
            Tamanho mínimo (KB)
            <input
              type="number"
              min={0}
              value={minKb}
              disabled={busy}
              onChange={(e) => {
                setMinKb(e.target.value);
                setInputError(null);
              }}
            />
          </label>
        )}
        {running ? (
          <button
            type="button"
            className="btn"
            disabled={cancelling}
            onClick={cancel}
          >
            Cancelar
          </button>
        ) : (
          <button
            type="button"
            className="btn btn-primary"
            disabled={busy || !folder}
            onClick={search}
          >
            Procurar
          </button>
        )}
      </div>
      <p className="disk-folder muted">
        {folder ? (
          <span className="finding-path">{folder}</span>
        ) : (
          "Nenhuma pasta escolhida."
        )}
      </p>
      <p className="muted">Só lista os arquivos: nada é apagado ou movido.</p>

      {(inputError || job.error) && (
        <p role="alert" className="form-error">
          {inputError ?? job.error}
        </p>
      )}

      {running && (
        <p className="muted" role="status">
          Procurando…
        </p>
      )}

      {!running && result && (
        <div className="scan-result">
          {result.interrupted && (
            <p className="notice">Busca interrompida: resultado parcial.</p>
          )}
          {tab === "large" &&
            large &&
            (large.items.length === 0 ? (
              <p className="muted">Nenhum arquivo encontrado.</p>
            ) : (
              <ul className="finding-list">
                {large.items.map((f) => (
                  <PathRow key={f.path} path={f.path} size={f.size} />
                ))}
              </ul>
            ))}
          {tab === "dupes" &&
            dupes &&
            (dupes.items.length === 0 ? (
              <p className="muted">Nenhum duplicado encontrado.</p>
            ) : (
              <>
                <p>{`Espaço desperdiçado: ${humanBytes(wasted(dupes.items))}`}</p>
                {dupes.items.map((g) => (
                  <section key={g.sha256} className="finding-group">
                    <h3>{`${g.paths.length} cópias · ${humanBytes(g.size)} cada`}</h3>
                    <ul className="finding-list">
                      {g.paths.map((p) => (
                        <PathRow key={p} path={p} />
                      ))}
                    </ul>
                  </section>
                ))}
              </>
            ))}
          <Unverified result={result} />
        </div>
      )}
    </section>
  );
}
