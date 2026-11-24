import { useCallback, useEffect, useRef, useState } from "react";
import { relaunchElevated, tweaksApply, tweaksList } from "../api";
import type {
  ApplyReport,
  TweakEffect,
  TweakGroup,
  TweakItem,
  TweakStatus,
  TweaksPage,
} from "../api";
import GameSessionCard from "../components/GameSession";
import ConfirmDialog from "../components/ConfirmDialog";
import GpuPerGame from "../components/GpuPerGame";
import StartupList from "../components/StartupList";
import { useJob } from "../jobs";

type Profile = "recomendado" | "fps" | "personalizado";

type Tab = "principais" | "avancado";

const EVIDENCE_LABEL: Record<TweakItem["tweak"]["evidence"], string> = {
  comprovado: "Comprovado",
  situacional: "Situacional",
  controverso: "Controverso",
};

const GROUPS: { id: TweakGroup; label: string }[] = [
  { id: "jogos", label: "Jogos" },
  { id: "energia", label: "Energia" },
  { id: "segundo_plano", label: "Segundo plano" },
  { id: "privacidade", label: "Privacidade" },
  { id: "visual", label: "Visual" },
];

const STATUS: Record<TweakStatus, string> = {
  applied: "Aplicado",
  not_applied: "Não aplicado",
  partial: "Parcial",
  unavailable: "Indisponível nesta máquina",
};

const EFFECT_LABEL: Partial<Record<TweakEffect, string>> = {
  reboot: "Exige reinício",
  relogon: "Vale após sair e entrar",
};

const EFFECT_NOTICE: Partial<Record<TweakEffect, string>> = {
  reboot: "Reinicie o computador para concluir.",
  relogon: "Saia e entre de novo no Windows para concluir.",
};

const POWER_PLAN = "power-high-performance";

const OTHER_ACCOUNT =
  "Esta janela está em outra conta do Windows. O histórico e os ajustes do usuário mostrados aqui são dessa conta.";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

const selectable = (i: TweakItem) =>
  i.view.blocked === null && i.view.status !== "unavailable";

/** Vai escrever algo no sistema (mesma regra do backend). */
const willWrite = (i: TweakItem) =>
  i.view.blocked === null &&
  (i.view.status === "not_applied" || i.view.status === "partial");

/** Perfil "Recomendado": recomendados disponíveis e ainda não aplicados. */
const recommendedIds = (p: TweaksPage) =>
  new Set(
    p.tweaks
      .filter(
        (i) =>
          i.tweak.recommended &&
          selectable(i) &&
          i.view.status !== "applied" &&
          !(p.laptop && i.tweak.id === POWER_PLAN),
      )
      .map((i) => i.tweak.id),
  );

/** "FPS competitivo": Recomendado + os situacionais da aba Avançado (nunca os controversos). */
const fpsIds = (p: TweaksPage) => {
  const ids = recommendedIds(p);
  for (const i of p.tweaks)
    if (
      i.tweak.group === "avancado" &&
      i.tweak.evidence === "situacional" &&
      selectable(i) &&
      i.view.status !== "applied"
    )
      ids.add(i.tweak.id);
  return ids;
};

const profileIds = (profile: Profile, p: TweaksPage) =>
  profile === "fps" ? fpsIds(p) : recommendedIds(p);

export default function Jogos() {
  const [data, setData] = useState<TweaksPage | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [profile, setProfile] = useState<Profile>("recomendado");
  const profileRef = useRef<Profile>("recomendado");
  const [tab, setTab] = useState<Tab>("principais");
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const [confirming, setConfirming] = useState(false);
  const [relaunchError, setRelaunchError] = useState<string | null>(null);
  const job = useJob<ApplyReport>("tweaks");
  // O job "tweaks" é compartilhado com o Histórico: só mostra o relatório de
  // uma aplicação iniciada nesta página.
  const [started, setStarted] = useState(false);

  const load = useCallback(() => {
    setLoadError(null);
    tweaksList().then(
      (p) => {
        setData(p);
        // Reaplica o perfil ativo aos dados novos; no personalizado, só
        // descarta o que deixou de ser selecionável.
        setChecked((old) =>
          profileRef.current !== "personalizado"
            ? profileIds(profileRef.current, p)
            : new Set(
                p.tweaks
                  .filter(
                    (i) =>
                      selectable(i) &&
                      i.view.status !== "applied" &&
                      old.has(i.tweak.id),
                  )
                  .map((i) => i.tweak.id),
              ),
        );
      },
      (e) => setLoadError(errorText(e)),
    );
  }, []);

  useEffect(load, [load]);

  // Terminado o job: recarrega a lista e reaplica o perfil ativo.
  useEffect(() => {
    if (job.state === "done" && job.result) load();
  }, [job.state, job.result, load]);

  const running = job.state === "running";
  const items = data?.tweaks ?? [];
  const chosen = items.filter((i) => checked.has(i.tweak.id));
  const elevated = data?.elevated ?? false;
  const needsAdmin = (i: TweakItem) => i.tweak.needs_admin;
  // Igual ao backend: elevado, algo vai escrever e nenhum ponto nesta sessão.
  const restorePointNote =
    elevated && !(data?.restore_point_done ?? false) && chosen.some(willWrite);
  const bannerNeeded =
    data !== null &&
    !elevated &&
    items.some(
      (i) =>
        needsAdmin(i) &&
        (checked.has(i.tweak.id) ||
          (i.tweak.recommended && i.view.status !== "applied")),
    );
  const titleOf = (id: string) =>
    items.find((i) => i.tweak.id === id)?.tweak.title ?? id;

  const chooseProfile = (p: Profile) => {
    profileRef.current = p;
    setProfile(p);
    if (p !== "personalizado" && data) setChecked(profileIds(p, data));
    // Mostra o que o perfil acrescentou na aba Avançado.
    if (p === "fps") setTab("avancado");
  };

  const toggle = (id: string) => {
    profileRef.current = "personalizado";
    setProfile("personalizado");
    setChecked((s) => {
      const next = new Set(s);
      if (!next.delete(id)) next.add(id);
      return next;
    });
  };

  const confirm = () => {
    setConfirming(false);
    const ids = chosen.map((i) => i.tweak.id);
    if (ids.length === 0) return; // nunca envia seleção vazia
    setStarted(true);
    void job.start(() => tweaksApply(ids));
  };

  const relaunch = async () => {
    setRelaunchError(null);
    try {
      await relaunchElevated();
    } catch (e) {
      setRelaunchError(errorText(e));
    }
  };

  const report =
    started && job.state === "done" && job.result && "effect" in job.result
      ? job.result
      : null;

  /** Uma linha da lista; na aba Avançado mostra também a explicação inteira. */
  const renderRow = (i: TweakItem, details: boolean) => {
    const t = i.tweak;
    return (
      <li key={t.id} className="clean-row">
        <div className="clean-head">
          <label className="clean-check">
            <input
              type="checkbox"
              checked={checked.has(t.id)}
              disabled={!selectable(i) || running}
              onChange={() => toggle(t.id)}
            />
            <span>{t.title}</span>
          </label>
          <span className="tweak-badge">{EVIDENCE_LABEL[t.evidence]}</span>
          <span className="muted">{STATUS[i.view.status]}</span>
          {t.needs_admin && <span className="tweak-badge">Requer administrador</span>}
          {EFFECT_LABEL[t.effect] && (
            <span className="tweak-badge">{EFFECT_LABEL[t.effect]}</span>
          )}
        </div>
        {details && <p className="muted clean-warning">{t.description}</p>}
        {i.view.blocked &&
          !(t.needs_admin && i.view.blocked === "requer administrador") && (
            <p className="muted clean-warning">{i.view.blocked}</p>
          )}
        {data?.laptop && t.laptop_warning && (
          <p className="muted clean-warning">
            Em notebook, gasta mais bateria e esquenta mais.
          </p>
        )}
      </li>
    );
  };

  return (
    <section className="page">
      <h1>Jogos</h1>
      <GameSessionCard />
      {data && !data.hkcu_allowed && <p className="notice">{OTHER_ACCOUNT}</p>}
      {bannerNeeded && (
        <div className="notice">
          <p>Algumas otimizações exigem administrador.</p>
          <button type="button" className="btn" onClick={() => void relaunch()}>
            Reiniciar como administrador
          </button>
        </div>
      )}
      {relaunchError && (
        <p role="alert" className="form-error">
          {relaunchError}
        </p>
      )}
      {loadError && (
        <div>
          <p role="alert" className="form-error">
            Não foi possível carregar as otimizações: {loadError}
          </p>
          <button type="button" className="btn" onClick={load}>
            Tentar de novo
          </button>
        </div>
      )}
      {job.error && (
        <p role="alert" className="form-error">
          Não foi possível aplicar: {job.error}
        </p>
      )}
      {!data && !loadError && <p className="muted">Carregando…</p>}
      {running && <p className="muted">Aplicando…</p>}
      {report && (
        <div className="notice">
          {report.notice && <p>{report.notice}</p>}
          <ul className="tweak-results">
            {report.results.map((r) => (
              <li key={r.tweak_id} className={r.ok ? undefined : "form-error"}>
                {r.ok
                  ? `${titleOf(r.tweak_id)}: aplicado`
                  : `${titleOf(r.tweak_id)}: ${r.message}`}
              </li>
            ))}
          </ul>
          {EFFECT_NOTICE[report.effect] && <p>{EFFECT_NOTICE[report.effect]}</p>}
          {report.restore_point !== null && (
            <p>{`Ponto de restauração: ${report.restore_point}`}</p>
          )}
        </div>
      )}
      {data && (
        <div className="tweak-layout">
          <div>
            <div className="tweak-profiles" role="group" aria-label="Perfil">
              {(
                [
                  ["recomendado", "Recomendado"],
                  ["fps", "FPS competitivo"],
                  ["personalizado", "Personalizado"],
                ] as const
              ).map(([id, label]) => (
                <button
                  key={id}
                  type="button"
                  className="btn"
                  aria-pressed={profile === id}
                  disabled={running}
                  onClick={() => chooseProfile(id)}
                >
                  {label}
                </button>
              ))}
            </div>
            <div className="tweak-profiles" role="tablist" aria-label="Lista">
              {(
                [
                  ["principais", "Principais"],
                  ["avancado", "Avançado"],
                ] as const
              ).map(([id, label]) => (
                <button
                  key={id}
                  type="button"
                  role="tab"
                  className="btn"
                  aria-selected={tab === id}
                  onClick={() => setTab(id)}
                >
                  {label}
                </button>
              ))}
            </div>
            {tab === "principais" &&
              GROUPS.map(({ id, label }) => {
                const rows = items.filter((i) => i.tweak.group === id);
                if (rows.length === 0) return null;
                return (
                  <div key={id}>
                    <h2>{label}</h2>
                    <ul className="clean-list">{rows.map((i) => renderRow(i, false))}</ul>
                  </div>
                );
              })}
            {tab === "avancado" && (
              <div role="tabpanel" aria-label="Avançado">
                <p className="muted">
                  Ajustes com ganho pequeno ou que dependem do jogo, e alguns que pioram outras
                  coisas. Leia a explicação de cada um. Os controversos nunca entram em perfil.
                </p>
                <ul className="clean-list">
                  {items
                    .filter((i) => i.tweak.group === "avancado")
                    .map((i) => renderRow(i, true))}
                </ul>
              </div>
            )}
            <div className="clean-footer">
              <span>{`${chosen.length} selecionada(s)`}</span>
              <button
                type="button"
                className="btn btn-primary"
                disabled={running || chosen.length === 0}
                onClick={() => setConfirming(true)}
              >
                Aplicar
              </button>
            </div>
          </div>
          <aside className="tweak-preview" aria-label="Prévia">
            <h2>Prévia</h2>
            {chosen.length === 0 && <p className="muted">Nada marcado.</p>}
            {chosen.map((i) => (
              <div key={i.tweak.id}>
                <strong>{i.tweak.title}</strong>
                <ul>
                  {i.view.changes.map((c, n) => (
                    <li key={`${c.description}-${n}`}>
                      {`${c.description}: ${c.current} → ${c.target}`}
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </aside>
        </div>
      )}
      <p className="muted tweak-footnote">
        O Sentinela não desativa Defender, Windows Update, firewall nem outras
        proteções para ganhar FPS.
      </p>
      <GpuPerGame />
      <StartupList />
      {confirming && (
        <ConfirmDialog
          title={`Aplicar ${chosen.length} otimização(ões)?`}
          confirmLabel="Aplicar"
          onConfirm={confirm}
          onCancel={() => setConfirming(false)}
        >
          {restorePointNote && (
            <p>Um ponto de restauração do Windows será solicitado antes.</p>
          )}
        </ConfirmDialog>
      )}
    </section>
  );
}
