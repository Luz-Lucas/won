import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  netCompare,
  netGameRouteSet,
  netRouteSet,
  netRoutesPage,
  netTunnelImport,
  netTunnelRemove,
  netWarpRegister,
  netWarpTunnelCreate,
  netWifiHoldSet,
} from "../api";
import type { Comparison, NetResult, NetRoute, NetTargetItem, RoutesPage } from "../api";
import { useJob } from "../jobs";
import ConfirmDialog from "./ConfirmDialog";

const TUNNEL_WARNING =
  "Com o túnel ligado, o tráfego passa pelo servidor daquele serviço. Alguns jogos e anti-cheats bloqueiam VPN.";

const WARP_TERMS =
  "Isto cria uma conta gratuita do Cloudflare WARP neste computador e aceita os Termos de Serviço da Cloudflare em seu nome. Continue só se você concorda com eles.";
const WARP_WG_TERMS =
  "Isto registra um dispositivo gratuito no Cloudflare WARP (aceitando os Termos de Serviço da Cloudflare) e cria um túnel WireGuard chamado \"warp\". A chave privada fica só neste computador.";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

const ms = (v: number | null) => (v === null ? "—" : `${v.toFixed(1).replace(".", ",")} ms`);

const stateText = (r: NetRoute) =>
  r.on === null ? "Estado desconhecido" : r.on ? "Ligado" : "Desligado";

/** Rotas alternativas, comparador, rota por jogo e Wi‑Fi durante jogos. */
export default function Routes({ targets }: { targets: NetTargetItem[] }) {
  const [page, setPage] = useState<RoutesPage | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [pending, setPending] = useState<NetRoute | null>(null);
  const [warpAsk, setWarpAsk] = useState<"register" | "wireguard" | null>(null);
  const [tunnelName, setTunnelName] = useState("");
  const [target, setTarget] = useState("");
  const [chosen, setChosen] = useState<Set<string>>(new Set());
  const [comparison, setComparison] = useState<Comparison | null>(null);
  const net = useJob<NetResult>("net");

  const load = useCallback(() => {
    netRoutesPage().then(setPage, (e) => setError(errorText(e)));
  }, []);

  useEffect(load, [load]);

  useEffect(() => {
    if (net.state === "done" && net.result?.op === "compare") {
      setComparison(net.result.comparison);
      load();
    }
  }, [net.state, net.result, load]);

  const run = async (f: () => Promise<RoutesPage>) => {
    setBusy(true);
    setError(null);
    try {
      setPage(await f());
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const importConf = async () => {
    const name = tunnelName.trim();
    if (!name) {
      setError("Dê um nome ao túnel antes de importar.");
      return;
    }
    const path = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "WireGuard", extensions: ["conf"] }],
    });
    if (typeof path === "string") {
      await run(() => netTunnelImport(name, path));
      setTunnelName("");
    }
  };

  const toggle = (r: NetRoute) => {
    if (r.on) void run(() => netRouteSet(r.id, false));
    else setPending(r);
  };

  const controllable = page?.routes.filter((r) => r.controllable) ?? [];
  const running = net.state === "running";
  const compareLabel = target || targets[0]?.label || "";

  const startCompare = () => {
    setComparison(null);
    void net.start(() => netCompare(compareLabel, [...chosen]));
  };

  return (
    <section aria-labelledby="routes-title">
      <h2 id="routes-title">Rotas alternativas</h2>
      <p className="muted">
        A rota até o servidor do jogo é do provedor. O Cloudflare WARP e um túnel WireGuard seu
        mandam o tráfego por outro caminho, que às vezes é melhor e às vezes pior: o Sentinela
        compara e só recomenda quando a diferença passa da variação medida.
      </p>
      {error && (
        <p role="alert" className="form-error">
          {error}
        </p>
      )}
      {!page && !error && <p className="muted">Carregando…</p>}
      {page && (
        <>
          {page.routes.length === 0 ? (
            <p className="muted">
              Nenhuma alternativa instalada. Instale o Cloudflare WARP ou o WireGuard pela página
              Aplicativos, ou importe um túnel.
            </p>
          ) : (
            <ul className="clean-list">
              {page.routes.map((r) => (
                <li key={r.id} className="clean-row">
                  <div className="clean-head">
                    {r.controllable ? (
                      <label className="clean-check">
                        <input
                          type="checkbox"
                          role="switch"
                          aria-label={r.name}
                          checked={r.on === true}
                          disabled={busy || running}
                          onChange={() => toggle(r)}
                        />
                        <span>{r.name}</span>
                      </label>
                    ) : (
                      <span className="clean-check">{r.name}</span>
                    )}
                    <span className="muted">{stateText(r)}</span>
                    {r.kind === "warp" && r.on === false && !r.controllable && (
                      <button
                        type="button"
                        className="btn"
                        disabled={busy}
                        onClick={() => setWarpAsk("register")}
                      >
                        Ativar WARP
                      </button>
                    )}
                    {r.kind === "wireguard" && r.controllable && (
                      <button
                        type="button"
                        className="btn"
                        disabled={busy || r.on === true}
                        onClick={() => void run(() => netTunnelRemove(r.id.slice(3)))}
                      >
                        Remover
                      </button>
                    )}
                  </div>
                  {r.note && <p className="muted clean-warning">{r.note}</p>}
                </li>
              ))}
            </ul>
          )}
          <div className="clean-footer">
            <label>
              Nome do túnel{" "}
              <input
                value={tunnelName}
                placeholder="ex.: vps-sp"
                onChange={(e) => setTunnelName(e.target.value)}
              />
            </label>
            <button type="button" className="btn" disabled={busy} onClick={() => void importConf()}>
              Importar .conf…
            </button>
            {!page.tunnels.some((t) => t.name === "warp") && (
              <button
                type="button"
                className="btn"
                disabled={busy}
                onClick={() => setWarpAsk("wireguard")}
              >
                Criar túnel WireGuard do WARP
              </button>
            )}
          </div>
          <p className="muted">
            O arquivo fica só neste computador; a chave privada não aparece na janela.
          </p>

          <h3>Comparar</h3>
          {controllable.length === 0 ? (
            <p className="muted">Nenhuma rota que o Sentinela possa ligar para comparar.</p>
          ) : (
            <>
              <label>
                Destino{" "}
                <select value={compareLabel} onChange={(e) => setTarget(e.target.value)}>
                  {targets.map((t) => (
                    <option key={t.label} value={t.label}>
                      {t.name ? `${t.name} (${t.label})` : t.label}
                    </option>
                  ))}
                </select>
              </label>
              <div role="group" aria-label="Rotas para comparar">
                {controllable.map((r) => (
                  <label key={r.id} className="clean-check">
                    <input
                      type="checkbox"
                      checked={chosen.has(r.id)}
                      onChange={() =>
                        setChosen((s) => {
                          const n = new Set(s);
                          if (!n.delete(r.id)) n.add(r.id);
                          return n;
                        })
                      }
                    />
                    <span>{r.name}</span>
                  </label>
                ))}
              </div>
              <p className="muted">{TUNNEL_WARNING} Cada rota volta ao estado em que estava.</p>
              <button
                type="button"
                className="btn btn-primary"
                disabled={running || chosen.size === 0 || !compareLabel}
                onClick={startCompare}
              >
                Comparar rotas
              </button>
            </>
          )}
          {comparison && (
            <div aria-label="Comparação de rotas">
              <table className="net-table">
                <thead>
                  <tr>
                    <th>Rota</th>
                    <th>Mediana</th>
                    <th>Jitter</th>
                    <th>Perda</th>
                  </tr>
                </thead>
                <tbody>
                  {comparison.rows.map((r) => (
                    <tr key={r.id}>
                      <td>
                        {r.name}
                        {comparison.best === r.id && <strong> · recomendada</strong>}
                      </td>
                      <td>{ms(r.measurement.median_ms)}</td>
                      <td>{ms(r.measurement.jitter_ms)}</td>
                      <td>{`${Math.round(r.measurement.loss_pct)}%`}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <p>{comparison.advice}</p>
              {comparison.interrupted && <p className="muted">Comparação cancelada.</p>}
              {comparison.errors.map((e) => (
                <p key={e} className="form-error">
                  {e}
                </p>
              ))}
            </div>
          )}

          <h3>Rota por jogo</h3>
          <p className="muted">
            Ao abrir o jogo, o Sentinela liga a rota escolhida; ao fechar, desliga (se foi ele que
            ligou). Túnel WireGuard exige o Sentinela aberto como administrador.
          </p>
          {page.games.length === 0 ? (
            <p className="muted">Nenhum jogo encontrado.</p>
          ) : (
            <ul className="clean-list">
              {page.games.map((g) => (
                <li key={g.key} className="clean-row">
                  <label className="clean-head">
                    <span className="clean-check">{g.name}</span>
                    <select
                      aria-label={`Rota de ${g.name}`}
                      value={g.route ?? ""}
                      disabled={busy}
                      onChange={(e) =>
                        void run(() => netGameRouteSet(g.key, e.target.value || null))
                      }
                    >
                      <option value="">Conexão normal</option>
                      {page.routes
                        .filter((r) => r.kind !== "paid")
                        .map((r) => (
                          <option key={r.id} value={r.id}>
                            {r.name}
                          </option>
                        ))}
                    </select>
                  </label>
                </li>
              ))}
            </ul>
          )}

          <h3>Wi‑Fi durante jogos</h3>
          <label className="clean-check">
            <input
              type="checkbox"
              checked={page.wifi_hold}
              disabled={busy}
              onChange={(e) => void run(() => netWifiHoldSet(e.target.checked))}
            />
            <span>Segurar a varredura do Wi‑Fi enquanto um jogo estiver aberto</span>
          </label>
          <p className="muted">
            Situacional: evita picos de atraso quando o Windows procura outras redes. O Windows
            volta ao normal sozinho quando o jogo ou o Sentinela fecham.
          </p>
        </>
      )}
      {pending && (
        <ConfirmDialog
          title={`Ligar ${pending.name}?`}
          confirmLabel="Ligar"
          onConfirm={() => {
            const r = pending;
            setPending(null);
            void run(() => netRouteSet(r.id, true));
          }}
          onCancel={() => setPending(null)}
        >
          <p>{TUNNEL_WARNING}</p>
        </ConfirmDialog>
      )}
      {warpAsk && (
        <ConfirmDialog
          title={warpAsk === "register" ? "Ativar o Cloudflare WARP?" : "Criar o túnel do WARP?"}
          confirmLabel={warpAsk === "register" ? "Ativar" : "Criar túnel"}
          onConfirm={() => {
            const ask = warpAsk;
            setWarpAsk(null);
            void run(ask === "register" ? netWarpRegister : () => netWarpTunnelCreate());
          }}
          onCancel={() => setWarpAsk(null)}
        >
          <p>{warpAsk === "register" ? WARP_TERMS : WARP_WG_TERMS}</p>
        </ConfirmDialog>
      )}
    </section>
  );
}
