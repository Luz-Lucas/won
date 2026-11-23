import { useCallback, useEffect, useState } from "react";
import {
  netBufferbloat,
  netDnsApply,
  netDnsBench,
  netDnsPreview,
  netMeasure,
  netPage,
  netTargetAdd,
  netTargetRemove,
  netTrace,
  relaunchElevated,
  tweaksApply,
  tweaksList,
  tweaksUndo,
} from "../api";
import type {
  ApplyReport,
  BloatGrade,
  BufferbloatResult,
  DnsPreview,
  DnsReport,
  Measurement,
  NetHistoryEntry,
  NetPage,
  NetResult,
  NetTarget,
  Trace,
  TweakItem,
  TweakItemResult,
  UndoReport,
} from "../api";
import ConfirmDialog from "../components/ConfirmDialog";
import { humanDate } from "../format";
import { useJob } from "../jobs";

const HONESTY =
  "A rota até o servidor do jogo é decidida pelo seu provedor e pelas redes no caminho. O Sentinela mede, ajusta o que é do seu computador e mostra o antes e depois.";

const DNS_NOTE =
  "O DNS só traduz nomes em endereços: acelera a abertura de sites e a conexão com servidores, não o ping dentro do jogo.";

const GRADE_LABEL: Record<BloatGrade, string> = {
  good: "Bom",
  moderate: "Moderado",
  high: "Alto",
};

const HISTORY_KIND: Record<NetHistoryEntry["kind"], string> = {
  latency: "Latência",
  trace: "Rota",
  bufferbloat: "Bufferbloat",
  dns: "DNS",
  route: "Rotas",
};

const ADMIN = "requer administrador";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

/** "23,4 ms" (ou "—" sem resposta). */
export const ms = (v: number | null) =>
  v === null ? "—" : `${v.toFixed(1).replace(".", ",")} ms`;

/** Igual a `Target::label` no backend. */
export const labelOf = (t: NetTarget) =>
  t.port === null ? t.host : t.host.includes(":") ? `[${t.host}]:${t.port}` : `${t.host}:${t.port}`;

const band = (ch: number) => (ch <= 14 ? "2,4 GHz" : "5 GHz ou 6 GHz");

const applied = (i: TweakItem) => i.view.status === "applied";
const usable = (i: TweakItem) => i.view.blocked === null && i.view.status !== "unavailable";

type Pending =
  | { kind: "dns"; resolver: string; name: string; doh: boolean; preview: DnsPreview[] }
  | { kind: "on"; item: TweakItem }
  | { kind: "off"; item: TweakItem };

export default function Rede() {
  const [page, setPage] = useState<NetPage | null>(null);
  const [tweaks, setTweaks] = useState<TweakItem[]>([]);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [spec, setSpec] = useState("");
  const [measured, setMeasured] = useState<Map<string, Measurement>>(new Map());
  const [trace, setTrace] = useState<Trace | null>(null);
  const [bloat, setBloat] = useState<BufferbloatResult | null>(null);
  const [dns, setDns] = useState<DnsReport | null>(null);
  const [dnsResults, setDnsResults] = useState<TweakItemResult[] | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);
  const [busy, setBusy] = useState(false);
  const [doh, setDoh] = useState(false);
  const net = useJob<NetResult>("net");
  const tweakJob = useJob<ApplyReport | UndoReport>("tweaks");
  const [tweakStarted, setTweakStarted] = useState(false);

  const loadTweaks = useCallback(() => {
    tweaksList().then(
      (p) => setTweaks(p.tweaks.filter((i) => i.tweak.group === "rede")),
      () => setTweaks([]),
    );
  }, []);

  const load = useCallback(() => {
    setLoadError(null);
    netPage().then(setPage, (e) => setLoadError(errorText(e)));
    loadTweaks();
  }, [loadTweaks]);

  useEffect(load, [load]);

  // Um job de rede terminou: guarda o resultado e relê o histórico.
  useEffect(() => {
    if (net.state !== "done" || !net.result) return;
    const r = net.result;
    if (r.op === "measure")
      setMeasured((old) => {
        const next = new Map(old);
        for (const m of r.results) next.set(labelOf(m.target), m);
        return next;
      });
    else if (r.op === "trace") setTrace(r.trace);
    else if (r.op === "bufferbloat") setBloat(r.result);
    else setDns(r.report);
    netPage().then(setPage, () => {});
  }, [net.state, net.result]);

  useEffect(() => {
    if (tweakJob.state === "done" && tweakJob.result) loadTweaks();
  }, [tweakJob.state, tweakJob.result, loadTweaks]);

  const running = net.state === "running";
  const elevated = page?.elevated ?? false;

  const startNet = (fn: () => Promise<number>) => {
    setActionError(null);
    void net.start(fn);
  };

  const add = async () => {
    const s = spec.trim();
    if (!s) return;
    setActionError(null);
    try {
      setPage(await netTargetAdd(s));
      setSpec("");
    } catch (e) {
      setActionError(errorText(e));
    }
  };

  const remove = async (label: string) => {
    setActionError(null);
    try {
      setPage(await netTargetRemove(label));
    } catch (e) {
      setActionError(errorText(e));
    }
  };

  const askDns = async (resolver: string) => {
    const row = dns?.rows.find((r) => r.resolver === resolver && !r.current);
    setActionError(null);
    try {
      const encrypt = doh && dohOffered(resolver);
      const preview = await netDnsPreview(resolver, encrypt);
      setPending({ kind: "dns", resolver, name: row?.name ?? resolver, doh: encrypt, preview });
    } catch (e) {
      setActionError(errorText(e));
    }
  };

  const confirm = async () => {
    const p = pending;
    setPending(null);
    if (!p) return;
    if (p.kind === "dns") {
      setBusy(true);
      setDnsResults(null);
      try {
        setDnsResults(await netDnsApply(p.resolver, p.doh));
        setPage(await netPage());
      } catch (e) {
        setActionError(errorText(e));
      } finally {
        setBusy(false);
      }
      return;
    }
    setTweakStarted(true);
    const id = p.item.tweak.id;
    void tweakJob.start(() => (p.kind === "on" ? tweaksApply([id]) : tweaksUndo([id])));
  };

  const relaunch = () => {
    relaunchElevated().catch((e) => setActionError(errorText(e)));
  };

  /** O Windows conhece o modelo de DNS sobre HTTPS deste resolvedor. */
  const dohOffered = (resolver: string) => {
    const addr = dns?.rows.find((r) => r.resolver === resolver)?.addr;
    return addr !== undefined && (page?.doh_known ?? []).includes(addr);
  };

  const progress = net.progress?.phase === "net" ? net.progress : null;
  const onWifi = page?.adapters.some((a) => a.wifi) ?? false;
  const tweakReport =
    tweakStarted && tweakJob.state === "done" && tweakJob.result ? tweakJob.result : null;

  const dialog = () => {
    if (!pending) return null;
    if (pending.kind === "dns")
      return (
        <ConfirmDialog
          title={`Usar o DNS ${pending.name}${pending.doh ? " criptografado" : ""}?`}
          confirmLabel="Aplicar"
          onConfirm={() => void confirm()}
          onCancel={() => setPending(null)}
        >
          {pending.preview.map((p) => (
            <div key={p.adapter}>
              <strong>{p.adapter}</strong>
              <ul>
                {p.changes.map((c, n) => (
                  <li key={`${c.description}-${n}`}>{`${c.description}: ${c.current} → ${c.target}`}</li>
                ))}
              </ul>
            </div>
          ))}
          {pending.doh && (
            <p>
              Com DNS sobre HTTPS, as consultas vão criptografadas; se o servidor não responder
              por HTTPS, os nomes deixam de abrir até desfazer.
            </p>
          )}
          <p>Para voltar ao DNS anterior, desfaça pelo Histórico.</p>
        </ConfirmDialog>
      );
    const t = pending.item.tweak;
    if (pending.kind === "off")
      return (
        <ConfirmDialog
          title={`Desfazer "${t.title}"?`}
          confirmLabel="Desfazer"
          onConfirm={() => void confirm()}
          onCancel={() => setPending(null)}
        >
          <p>Volta ao valor que estava antes do Sentinela. Vale depois de reiniciar.</p>
        </ConfirmDialog>
      );
    return (
      <ConfirmDialog
        title={`Aplicar "${t.title}"?`}
        confirmLabel="Aplicar"
        onConfirm={() => void confirm()}
        onCancel={() => setPending(null)}
      >
        <p>{t.description}</p>
        <ul>
          {pending.item.view.changes.map((c, n) => (
            <li key={`${c.description}-${n}`}>{`${c.description}: ${c.current} → ${c.target}`}</li>
          ))}
        </ul>
      </ConfirmDialog>
    );
  };

  return (
    <section className="page">
      <h1>Rede</h1>
      <p className="muted">{HONESTY}</p>
      {loadError && (
        <div>
          <p role="alert" className="form-error">
            Não foi possível carregar a rede: {loadError}
          </p>
          <button type="button" className="btn" onClick={load}>
            Tentar de novo
          </button>
        </div>
      )}
      {!page && !loadError && <p className="muted">Carregando…</p>}
      {page?.wifi_notice && <p className="notice">{page.wifi_notice}</p>}
      {actionError && (
        <p role="alert" className="form-error">
          {actionError}
        </p>
      )}
      {net.error && (
        <p role="alert" className="form-error">
          A medição falhou: {net.error}
        </p>
      )}
      {running && (
        <div className="notice">
          <p>
            {progress
              ? `Medindo ${progress.current} (${progress.index + 1} de ${progress.total})…`
              : "Medindo…"}
          </p>
          <button type="button" className="btn" onClick={() => void net.cancel()}>
            Cancelar
          </button>
        </div>
      )}

      {page && (
        <>
          <h2>Medir</h2>
          <table className="net-table">
            <thead>
              <tr>
                <th>Destino</th>
                <th>Média</th>
                <th>Jitter</th>
                <th>Perda</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {page.targets.map((t) => {
                const m = measured.get(t.label);
                return (
                  <tr key={t.label}>
                    <td>{t.name ? `${t.name} (${t.label})` : t.label}</td>
                    <td>{m ? (m.avg_ms === null ? "sem resposta" : ms(m.avg_ms)) : "—"}</td>
                    <td>{m ? ms(m.jitter_ms) : "—"}</td>
                    <td>{m ? `${Math.round(m.loss_pct)}%` : "—"}</td>
                    <td>
                      <button
                        type="button"
                        className="btn"
                        disabled={running}
                        aria-label={`Rastrear rota até ${t.label}`}
                        onClick={() => startNet(() => netTrace(t.label))}
                      >
                        Rastrear rota
                      </button>
                      {t.user && (
                        <button
                          type="button"
                          className="btn"
                          disabled={running}
                          aria-label={`Remover ${t.label}`}
                          onClick={() => void remove(t.label)}
                        >
                          Remover
                        </button>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          <div className="clean-footer">
            <button
              type="button"
              className="btn btn-primary"
              disabled={running}
              onClick={() => startNet(() => netMeasure(null))}
            >
              Medir
            </button>
            <label>
              Novo destino{" "}
              <input
                value={spec}
                placeholder="servidor ou servidor:porta"
                onChange={(e) => setSpec(e.target.value)}
              />
            </label>
            <button type="button" className="btn" onClick={() => void add()}>
              Adicionar destino
            </button>
          </div>
          <p className="muted">
            Destinos com porta são medidos pelo tempo de conexão (para servidores que não
            respondem a ping).
          </p>

          {trace && (
            <div aria-label="Rota">
              <h3>{`Rota até ${labelOf(trace.target)}`}</h3>
              {trace.error ? (
                <p className="form-error">{trace.error}</p>
              ) : (
                <>
                  <table className="net-table">
                    <thead>
                      <tr>
                        <th>Salto</th>
                        <th>Endereço</th>
                        <th>Tempo</th>
                        <th>Trecho</th>
                      </tr>
                    </thead>
                    <tbody>
                      {trace.hops.map((h) => {
                        const best = h.rtts_ms.filter((v): v is number => v !== null);
                        return (
                          <tr key={h.ttl}>
                            <td>{h.ttl}</td>
                            <td>{h.addr ?? "*"}</td>
                            <td>{best.length ? ms(Math.min(...best)) : "—"}</td>
                            <td>
                              {h.segment === "home" ? "Sua casa" : "Provedor e internet"}
                              {trace.delay_start === h.ttl && (
                                <strong> · o atraso começa aqui</strong>
                              )}
                            </td>
                          </tr>
                        );
                      })}
                    </tbody>
                  </table>
                  <p className="muted">
                    {trace.delay_start === null
                      ? "Nenhum salto acrescenta atraso de forma consistente."
                      : trace.hops.find((h) => h.ttl === trace.delay_start)?.segment === "home"
                        ? "O atraso começa dentro de casa: confira o roteador, o cabo ou o Wi‑Fi."
                        : "O atraso começa no provedor ou depois dele: isso não se ajusta no computador."}
                    {!trace.reached && " O destino não respondeu ao fim da rota."}
                  </p>
                </>
              )}
            </div>
          )}

          <h2>Bufferbloat</h2>
          <p className="muted">
            Mede o atraso até o roteador parado e de novo com um download ocupando a conexão
            (cerca de 15 segundos).
          </p>
          <button
            type="button"
            className="btn"
            disabled={running}
            onClick={() => startNet(netBufferbloat)}
          >
            Testar bufferbloat
          </button>
          {bloat && (
            <div className="notice" aria-label="Resultado do bufferbloat">
              {bloat.grade && <p>{`Resultado: ${GRADE_LABEL[bloat.grade]}`}</p>}
              <p>
                {`Parado: ${ms(bloat.idle.median_ms)} · Ocupado: ${ms(bloat.loaded.median_ms)}`}
                {bloat.download_mbps !== null &&
                  ` · Download: ${Math.round(bloat.download_mbps)} Mbps`}
              </p>
              <p>{bloat.diagnosis}</p>
            </div>
          )}

          <h2>DNS</h2>
          <p className="muted">{DNS_NOTE}</p>
          <button
            type="button"
            className="btn"
            disabled={running}
            onClick={() => startNet(netDnsBench)}
          >
            Medir DNS
          </button>
          {page.adapters.length > 0 && (
            <ul className="muted">
              {page.adapters.map((a) => (
                <li key={a.guid}>
                  {`${a.name}: ${a.dns.length ? a.dns.join(", ") : "automático"}`}
                </li>
              ))}
            </ul>
          )}
          {dns && (
            <div aria-label="Resultado do DNS">
              <table className="net-table">
                <thead>
                  <tr>
                    <th>DNS</th>
                    <th>Tempo</th>
                    <th>Respostas</th>
                  </tr>
                </thead>
                <tbody>
                  {dns.rows.map((r) => (
                    <tr key={`${r.name}-${r.addr}`}>
                      <td>{r.name}</td>
                      <td>{r.median_ms === null ? "sem resposta" : ms(r.median_ms)}</td>
                      <td>{`${r.answered} de ${r.sent}`}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <p>{dns.advice}</p>
              {!dns.keep_current && dns.best && (
                <>
                  {!elevated && (
                    <div className="notice">
                      <p>Trocar o DNS exige administrador.</p>
                      <button type="button" className="btn" onClick={relaunch}>
                        Reiniciar como administrador
                      </button>
                    </div>
                  )}
                  {dohOffered(dns.best) && (
                    <label className="clean-check">
                      <input
                        type="checkbox"
                        checked={doh}
                        onChange={(e) => setDoh(e.target.checked)}
                      />
                      <span>Criptografar (DNS sobre HTTPS)</span>
                    </label>
                  )}
                  <button
                    type="button"
                    className="btn btn-primary"
                    disabled={!elevated || busy || running}
                    onClick={() => void askDns(dns.best!)}
                  >
                    Usar o mais rápido
                  </button>
                </>
              )}
            </div>
          )}
          {dnsResults && (
            <ul className="tweak-results">
              {dnsResults.map((r) => (
                <li key={r.tweak_id} className={r.ok ? undefined : "form-error"}>
                  {r.ok ? "DNS aplicado. Desfaça pelo Histórico." : r.message}
                </li>
              ))}
            </ul>
          )}

          <h2>Adaptador</h2>
          {tweaks.length === 0 ? (
            <p className="muted">Nenhum adaptador físico com ajustes disponíveis.</p>
          ) : (
            <ul className="clean-list">
              {tweaks.map((i) => {
                const external = applied(i) && !i.in_journal;
                return (
                  <li key={i.tweak.id} className="clean-row">
                    <div className="clean-head">
                      <label className="clean-check">
                        <input
                          type="checkbox"
                          role="switch"
                          checked={applied(i)}
                          disabled={tweakJob.state === "running" || !usable(i) || external}
                          onChange={() =>
                            setPending({ kind: applied(i) ? "off" : "on", item: i })
                          }
                        />
                        <span>{i.tweak.title}</span>
                      </label>
                      <span className="tweak-badge">Situacional</span>
                      <span className="tweak-badge">Exige reinício</span>
                    </div>
                    {external && (
                      <p className="muted clean-warning">Configurado fora do Sentinela.</p>
                    )}
                    {i.view.blocked && i.view.blocked !== ADMIN && (
                      <p className="muted clean-warning">{i.view.blocked}</p>
                    )}
                    {i.view.blocked === ADMIN && (
                      <p className="muted clean-warning">Requer administrador.</p>
                    )}
                  </li>
                );
              })}
            </ul>
          )}
          {tweakReport && (
            <ul className="tweak-results">
              {tweakReport.results.map((r) => (
                <li key={r.tweak_id} className={r.ok ? undefined : "form-error"}>
                  {r.ok ? "Feito. Reinicie o computador para concluir." : r.message}
                </li>
              ))}
            </ul>
          )}

          <h2>Wi‑Fi</h2>
          {page.wifi ? (
            <p>
              {`Rede "${page.wifi.ssid}", sinal ${page.wifi.signal_pct}%`}
              {page.wifi.channel !== null &&
                `, canal ${page.wifi.channel} (${band(page.wifi.channel)})`}
            </p>
          ) : (
            <p className="muted">
              {onWifi ? "Não foi possível ler o Wi‑Fi." : "Sem conexão Wi‑Fi: você está no cabo."}
            </p>
          )}

          <h2>Histórico</h2>
          {page.history.length === 0 ? (
            <p className="muted">Nenhuma medição ainda.</p>
          ) : (
            <ul className="net-history">
              {page.history.slice(0, 20).map((h, n) => (
                <li key={`${h.at}-${n}`}>
                  {`${humanDate(h.at)} · ${HISTORY_KIND[h.kind]} · ${h.target}: ${h.summary}`}
                </li>
              ))}
            </ul>
          )}
        </>
      )}
      {dialog()}
    </section>
  );
}
