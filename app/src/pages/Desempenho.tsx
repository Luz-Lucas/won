import { useCallback, useEffect, useState } from "react";
import { perfGpu, perfReset, perfSample } from "../api";
import type { PerfGroup, PerfSnapshot } from "../api";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

type Ref = Record<string, { cpu: number; ram: number }>;

const sign = (n: number, unit: string) => `${n > 0 ? "+" : ""}${Math.round(n * 10) / 10} ${unit}`;

/** Dicas simples a partir da média medida; só aparecem quando há algo a melhorar. */
function tips(groups: PerfGroup[], samples: number): string[] {
  if (samples < 5) return [];
  const out: string[] = [];
  const g = (id: string) => groups.find((x) => x.id === id);
  const desk = g("desktop");
  if (desk && desk.cpu_avg > 3)
    out.push(
      `A barra, os widgets e o tiling usam em média ${desk.cpu_avg}% da CPU. Reduza widgets com relógio/mídia, desligue o tiling ou a barra com ilhas para baixar isso.`,
    );
  const ex = g("explorer");
  if (ex && ex.cpu_avg > 5)
    out.push(
      `O Explorer está em ${ex.cpu_avg}% da CPU em média. Com ilhas, dock ou Iniciar à esquerda ligados o estilo anima ~30 vezes por segundo; desligue um deles e compare.`,
    );
  const st = g("start");
  if (st && st.cpu_avg > 2) out.push(`O Iniciar usa ${st.cpu_avg}% em média. Desligue "esconder a lista A-Z" ou o tema do Iniciar e compare.`);
  const gui = g("gui");
  if (gui && gui.ram_avg_mb > 400) out.push(`A janela do WON usa ${gui.ram_avg_mb} MB; feche-a na bandeja quando não precisar dela.`);
  return out;
}

export default function Desempenho() {
  const [snap, setSnap] = useState<PerfSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [gpuOn, setGpuOn] = useState(false);
  const [gpu, setGpu] = useState<Record<string, number>>({});
  const [ref, setRef] = useState<Ref | null>(null);

  useEffect(() => {
    let alive = true;
    const tick = () =>
      perfSample().then(
        (s) => alive && (setSnap(s), setError(null)),
        (e) => alive && setError(errorText(e)),
      );
    void tick();
    const id = setInterval(() => void tick(), 2000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, []);

  useEffect(() => {
    if (!gpuOn) return;
    let alive = true;
    const tick = () =>
      perfGpu().then(
        (r) => alive && setGpu(Object.fromEntries(r.map((x) => [x.id, x.gpu]))),
        () => undefined,
      );
    void tick();
    const id = setInterval(() => void tick(), 5000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [gpuOn]);

  const restart = useCallback(() => {
    void perfReset().then(() => setSnap(null));
  }, []);

  const saveRef = () => {
    if (!snap) return;
    setRef(Object.fromEntries(snap.groups.map((g) => [g.id, { cpu: g.cpu_avg, ram: g.ram_avg_mb }])));
  };

  return (
    <section className="page">
      <h1>Monitor de desempenho</h1>
      <p className="muted">
        Quanto cada parte do WON e cada processo do Windows que recebe o estilo está usando agora,
        com média e pico desde o início da medição. A CPU é a fatia da máquina inteira.
      </p>
      {error && (
        <p role="alert" className="form-error">
          {error}
        </p>
      )}
      {snap && (
        <p>
          Máquina: CPU <strong>{snap.total_cpu}%</strong> · RAM{" "}
          <strong>
            {snap.ram_used_mb} / {snap.ram_total_mb} MB
          </strong>{" "}
          · {snap.samples} amostra(s)
        </p>
      )}
      <div className="ram-actions">
        <button type="button" className="btn" onClick={restart}>
          Zerar média e pico
        </button>
        <button type="button" className="btn" disabled={!snap || snap.samples < 5} onClick={saveRef}>
          Salvar como referência
        </button>
        {ref && (
          <button type="button" className="btn" onClick={() => setRef(null)}>
            Limpar referência
          </button>
        )}
      </div>
      <label className="check">
        <input type="checkbox" checked={gpuOn} onChange={(e) => setGpuOn(e.target.checked)} />
        Medir também a GPU (consulta o contador do Windows a cada 5 s)
      </label>
      {snap && (
        <table className="table" aria-label="Uso por parte">
          <thead>
            <tr>
              <th>Parte</th>
              <th>Proc.</th>
              <th>CPU agora</th>
              <th>CPU média</th>
              <th>CPU pico</th>
              <th>RAM agora</th>
              <th>RAM média</th>
              {gpuOn && <th>GPU</th>}
              {ref && <th>Δ da referência</th>}
            </tr>
          </thead>
          <tbody>
            {snap.groups.map((g) => (
              <tr key={g.id} title={g.note}>
                <td>{g.label}</td>
                <td>{g.procs}</td>
                <td>{g.cpu}%</td>
                <td>{g.cpu_avg}%</td>
                <td>{g.cpu_peak}%</td>
                <td>{g.ram_mb} MB</td>
                <td>{g.ram_avg_mb} MB</td>
                {gpuOn && <td>{gpu[g.id] ?? 0}%</td>}
                {ref && (
                  <td>
                    {ref[g.id]
                      ? `${sign(g.cpu_avg - ref[g.id].cpu, "% CPU")} · ${sign(g.ram_avg_mb - ref[g.id].ram, "MB")}`
                      : "—"}
                  </td>
                )}
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <p className="muted">
        Para saber o custo do estilo em si: desligue o estilo, deixe medir por um minuto e use
        "Salvar como referência"; ligue o estilo e compare a coluna Δ. Os processos do Windows
        (Explorer, Iniciar, Pesquisa) incluem o custo normal deles.
      </p>
      {snap &&
        tips(snap.groups, snap.samples).map((t) => (
          <p key={t} className="notice">
            {t}
          </p>
        ))}
    </section>
  );
}
