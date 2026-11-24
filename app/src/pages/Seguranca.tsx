import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import {
  scanExceptionAdd,
  scanGuard,
  scanQuarantine,
  scanScheduleDismiss,
  scanScheduleGet,
  scanScheduleSet,
  scanStart,
  type QuarantineOutcome,
  type QuarantineRequest,
  type ScanItem,
  type ScanSummary,
  type ScanTarget,
  type ScheduleView,
} from "../api";
import ConfirmDialog from "../components/ConfirmDialog";
import FindingList from "../components/FindingList";
import ProgressBar from "../components/ProgressBar";
import type { Page } from "../components/Sidebar";
import { useJob } from "../jobs";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));
const number = new Intl.NumberFormat("pt-BR");

/** A regra que fica registrada na quarentena: a do hash, se houver. */
const ruleOf = (item: ScanItem) =>
  (item.findings.find((f) => f.detector === "hash") ?? item.findings[0])?.rule ?? "";

const request = (item: ScanItem): QuarantineRequest => ({
  path: item.path,
  sha256: item.sha256,
  rule: ruleOf(item),
});

interface InUse {
  path: string;
  pids: number[];
  names: string[];
}

/** Quarentena em andamento: as travas que ainda pedem resposta. */
interface Pending {
  items: ScanItem[];
  system: string[];
  inUse: InUse[];
  step: "system" | "in_use";
}

export default function Seguranca({
  onNavigate,
}: {
  onNavigate?: (page: Page) => void;
}) {
  const job = useJob<ScanSummary>("scan");
  const [picking, setPicking] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [showErrors, setShowErrors] = useState(false);
  const [items, setItems] = useState<ScanItem[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [ignored, setIgnored] = useState(0);
  const [pending, setPending] = useState<Pending | null>(null);
  const [moving, setMoving] = useState(false);
  const [outcome, setOutcome] = useState<QuarantineOutcome | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [ignoring, setIgnoring] = useState<ScanItem | null>(null);
  const [schedule, setSchedule] = useState<ScheduleView | null>(null);
  const [scheduleError, setScheduleError] = useState<string | null>(null);

  const running = job.state === "running";
  const busy = running || picking || moving;
  const result = job.state === "done" ? job.result : null;

  useEffect(() => {
    if (!running) setCancelling(false);
  }, [running]);

  useEffect(() => {
    scanScheduleGet().then(setSchedule, () => setSchedule(null));
  }, []);

  // Resultado novo: só malware confirmado por hash vem marcado.
  useEffect(() => {
    if (!result) return;
    setItems(result.items);
    setSelected(new Set(result.items.filter((i) => i.recommended).map((i) => i.path)));
    setIgnored(result.ignored);
    setOutcome(null);
    setActionError(null);
  }, [result]);

  const scan = (target: ScanTarget) => {
    setShowErrors(false);
    void job.start(() => scanStart(target));
  };

  const pickFolders = async () => {
    setPicking(true);
    try {
      const dirs = await open({ directory: true, multiple: true });
      const list = Array.isArray(dirs) ? dirs : typeof dirs === "string" ? [dirs] : [];
      if (list.length > 0) scan({ kind: "custom", paths: list });
    } catch {
      /* seletor fechado ou indisponível: nada a verificar */
    } finally {
      setPicking(false);
    }
  };

  const cancel = () => {
    setCancelling(true);
    job.cancel().catch(() => setCancelling(false));
  };

  const toggle = (path: string) =>
    setSelected((s) => {
      const n = new Set(s);
      if (!n.delete(path)) n.add(path);
      return n;
    });

  const move = async (chosen: ScanItem[], endPids: number[]) => {
    setPending(null);
    if (chosen.length === 0) return;
    setMoving(true);
    setActionError(null);
    try {
      const out = await scanQuarantine(chosen.map(request), endPids);
      setOutcome(out);
      const gone = new Set(
        out.done.filter((d) => d.status === "quarantined").map((d) => d.path),
      );
      setItems((list) => list.filter((i) => !gone.has(i.path)));
      setSelected((s) => new Set([...s].filter((p) => !gone.has(p))));
    } catch (e) {
      setActionError(errorText(e));
    } finally {
      setMoving(false);
    }
  };

  /** Confere as travas antes de mover: pasta do Windows e processo aberto. */
  const quarantineSelected = async () => {
    const chosen = items.filter((i) => selected.has(i.path));
    if (chosen.length === 0) return;
    setMoving(true);
    setActionError(null);
    setOutcome(null);
    try {
      const guards = await scanGuard(chosen.map((i) => i.path));
      const system = guards
        .filter((g) => g.guards.some((x) => x.kind === "system_folder"))
        .map((g) => g.path);
      const inUse: InUse[] = guards.flatMap((g) =>
        g.guards.flatMap((x) =>
          x.kind === "in_use" ? [{ path: g.path, pids: x.pids, names: x.names }] : [],
        ),
      );
      setMoving(false);
      if (system.length > 0) setPending({ items: chosen, system, inUse, step: "system" });
      else if (inUse.length > 0) setPending({ items: chosen, system, inUse, step: "in_use" });
      else await move(chosen, []);
    } catch (e) {
      setMoving(false);
      setActionError(errorText(e));
    }
  };

  const ignore = async (item: ScanItem) => {
    setIgnoring(null);
    setActionError(null);
    try {
      await scanExceptionAdd(item.sha256, item.path, ruleOf(item));
      setItems((list) => list.filter((i) => i.sha256 !== item.sha256));
      setSelected((s) => {
        const n = new Set(s);
        n.delete(item.path);
        return n;
      });
      setIgnored((n) => n + 1);
    } catch (e) {
      setActionError(errorText(e));
    }
  };

  const answerSchedule = async (on: boolean) => {
    setScheduleError(null);
    try {
      setSchedule(await (on ? scanScheduleSet(true) : scanScheduleDismiss()));
    } catch (e) {
      setScheduleError(errorText(e));
    }
  };

  // Resultado parcial ou sem arquivos lidos nunca diz que nada foi encontrado.
  const nothingFound =
    result !== null &&
    !result.interrupted &&
    result.scanned > 0 &&
    result.malicious === 0 &&
    result.suspicious === 0;
  const offerSchedule =
    result !== null && schedule !== null && !schedule.offered && !schedule.scheduled;
  const quarantined = outcome?.done.filter((d) => d.status === "quarantined").length ?? 0;

  return (
    <section className="page">
      <h1>Segurança</h1>
      <div className="scan-actions">
        <button
          type="button"
          className="btn btn-primary"
          disabled={busy}
          onClick={() => scan({ kind: "quick" })}
        >
          Rápida
        </button>
        <button
          type="button"
          className="btn"
          disabled={busy}
          onClick={() => scan({ kind: "full" })}
        >
          Completa
        </button>
        <button
          type="button"
          className="btn"
          disabled={busy}
          onClick={() => void pickFolders()}
        >
          Personalizada…
        </button>
        <button type="button" className="btn" onClick={() => onNavigate?.("quarentena")}>
          Ver quarentena
        </button>
      </div>
      <p className="muted">
        A rápida lê os programas abertos, o que inicia com o Windows e as pastas onde malware
        costuma cair, só nos tipos de arquivo de risco. A completa lê todos os discos fixos e pode
        demorar bastante. Sem administrador, algumas pastas do sistema ficam de fora.
      </p>
      <p className="muted">
        O Sentinela verifica sob demanda: não há proteção em tempo real, e malware novo pode passar.
        Mantenha o Microsoft Defender ligado.
      </p>

      {job.error && (
        <p role="alert" className="form-error">
          {job.error}
        </p>
      )}

      {running && (
        <div className="scan-running">
          <ProgressBar progress={job.progress} />
          <button type="button" className="btn" disabled={cancelling} onClick={cancel}>
            Cancelar
          </button>
        </div>
      )}

      {result && (
        <div className="scan-result">
          {!result.db_initialized && (
            <div className="notice">
              <p>
                Banco de assinaturas não inicializado: só a assinatura de teste EICAR e as
                heurísticas estão ativas.
              </p>
              <button type="button" className="btn" onClick={() => onNavigate?.("ajustes")}>
                Ir para Ajustes
              </button>
            </div>
          )}
          {result.interrupted && (
            <p className="notice">Verificação interrompida: resultado parcial.</p>
          )}
          <p>
            {`${number.format(result.scanned)} arquivos verificados · ${result.malicious} maliciosos · ${result.suspicious} suspeitos`}
          </p>
          {nothingFound && (
            <p className="notice">Nenhuma ameaça conhecida encontrada nos arquivos verificados.</p>
          )}
          {ignored > 0 && (
            <p className="muted">
              {`${ignored} ${ignored === 1 ? "achado ignorado" : "achados ignorados"} por "Ignorar sempre" (a lista fica em Ajustes).`}
            </p>
          )}
          {result.missing.map((p) => (
            <p key={p} className="notice">
              {`Não foi possível verificar: ${p}`}
            </p>
          ))}
          {result.warnings.map((w) => (
            <p key={w} className="muted">
              {w}
            </p>
          ))}
          {items.length > 0 && (
            <>
              <p className="muted">
                Só o malware confirmado por hash vem marcado. Os outros podem ser falso positivo:
                marque o que quiser pôr em quarentena. Nada é movido sem você pedir.
              </p>
              <div className="scan-actions">
                <button
                  type="button"
                  className="btn btn-primary"
                  disabled={busy || selected.size === 0}
                  onClick={() => void quarantineSelected()}
                >
                  {`Colocar em quarentena (${selected.size})`}
                </button>
              </div>
            </>
          )}
          {actionError && (
            <p role="alert" className="form-error">
              {actionError}
            </p>
          )}
          {outcome && (
            <div className="notice" role="status">
              {quarantined > 0 && (
                <p>
                  {`${quarantined} ${quarantined === 1 ? "arquivo colocado" : "arquivos colocados"} em quarentena.`}
                </p>
              )}
              {outcome.done
                .filter((d) => d.status !== "quarantined")
                .map((d) => (
                  <p key={d.path}>
                    {d.status === "in_use"
                      ? `Em uso, continua no lugar: ${d.path}`
                      : `Não foi possível mover ${d.path}: ${d.status === "failed" ? d.reason : ""}`}
                  </p>
                ))}
              {outcome.not_ended.length > 0 && (
                <p>{`Não foi possível encerrar: ${outcome.not_ended.join(", ")}`}</p>
              )}
            </div>
          )}
          <FindingList
            items={items}
            selected={selected}
            onToggle={toggle}
            onIgnore={(i) => setIgnoring(i)}
          />
          {result.items_total > result.items.length && (
            <p className="muted">
              {`Mostrando ${number.format(result.items.length)} de ${number.format(result.items_total)}`}
            </p>
          )}
          {result.errors_total > 0 && (
            <div className="scan-errors">
              <button
                type="button"
                className="link-btn"
                aria-expanded={showErrors}
                onClick={() => setShowErrors((v) => !v)}
              >
                {`Não verificados (${number.format(result.errors_total)})`}
              </button>
              {showErrors && (
                <ul className="sample">
                  {result.errors.map((e, i) => (
                    <li key={`${e.path}-${i}`}>
                      <span>{e.path}</span> <span className="muted">{e.reason}</span>
                    </li>
                  ))}
                  {result.errors_total > result.errors.length && (
                    <li className="muted">
                      {`… e mais ${number.format(result.errors_total - result.errors.length)}`}
                    </li>
                  )}
                </ul>
              )}
            </div>
          )}
          {offerSchedule && (
            <div className="notice" role="status">
              <p>
                Agendar uma verificação rápida toda semana? Ela roda aos domingos ao meio-dia (ou
                assim que o computador ligar) e avisa o resultado numa notificação.
              </p>
              <div className="scan-actions">
                <button type="button" className="btn" onClick={() => void answerSchedule(true)}>
                  Agendar
                </button>
                <button type="button" className="btn" onClick={() => void answerSchedule(false)}>
                  Agora não
                </button>
              </div>
            </div>
          )}
          {scheduleError && (
            <p role="alert" className="form-error">
              {scheduleError}
            </p>
          )}
        </div>
      )}

      {pending?.step === "system" && (
        <ConfirmDialog
          title="Arquivos da pasta do Windows"
          confirmLabel="Colocar em quarentena mesmo assim"
          onConfirm={() => {
            if (pending.inUse.length > 0) setPending({ ...pending, step: "in_use" });
            else void move(pending.items, []);
          }}
          onCancel={() => setPending(null)}
        >
          <p>
            Estes arquivos estão dentro da pasta do Windows. Se forem do próprio sistema, tirá-los
            do lugar pode impedir o Windows de funcionar. Só continue se tiver certeza.
          </p>
          <ul>
            {pending.system.map((p) => (
              <li key={p}>{p}</li>
            ))}
          </ul>
        </ConfirmDialog>
      )}
      {pending?.step === "in_use" && (
        <ConfirmDialog
          title="Programas abertos"
          confirmLabel="Encerrar e colocar em quarentena"
          onConfirm={() =>
            void move(
              pending.items,
              pending.inUse.flatMap((u) => u.pids),
            )
          }
          onCancel={() => setPending(null)}
        >
          <p>
            Estes arquivos são de programas abertos. O Sentinela encerra os programas antes de
            mover; o que não fechar continua no lugar e aparece como "em uso".
          </p>
          <ul>
            {pending.inUse.map((u) => (
              <li key={u.path}>{`${u.names.join(", ")} — ${u.path}`}</li>
            ))}
          </ul>
        </ConfirmDialog>
      )}
      {ignoring && (
        <ConfirmDialog
          title="Ignorar sempre?"
          confirmLabel="Ignorar sempre"
          onConfirm={() => void ignore(ignoring)}
          onCancel={() => setIgnoring(null)}
        >
          <p>
            {`As próximas verificações não vão mostrar arquivos com este conteúdo (SHA-256 ${ignoring.sha256.slice(0, 16)}…), em qualquer pasta. Dá para desfazer em Ajustes.`}
          </p>
          {ignoring.recommended && (
            <p>
              <strong>Este arquivo está na lista de malware conhecido.</strong>
            </p>
          )}
        </ConfirmDialog>
      )}
    </section>
  );
}
