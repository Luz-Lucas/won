import { useEffect, useState } from "react";
import { organizeLogin, organizeLogout, organizeRefresh, organizeStatus } from "../api";
import type { OrganizeStatus } from "../api";

const errorText = (e: unknown) => (typeof e === "string" ? e : String(e));

/** Conta do Organize (Supabase) usada pelos widgets de agenda, tarefas e calendário. */
export default function OrganizeAccount() {
  const [st, setSt] = useState<OrganizeStatus | null>(null);
  const [url, setUrl] = useState("");
  const [key, setKey] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    organizeStatus().then(setSt, (e) => setError(errorText(e)));
  }, []);

  const run = async (f: () => Promise<OrganizeStatus>) => {
    setBusy(true);
    setError(null);
    try {
      setSt(await f());
      setPassword("");
    } catch (e) {
      setError(errorText(e));
    } finally {
      setBusy(false);
    }
  };

  const canLogin = url.trim() !== "" && key.trim() !== "" && email.trim() !== "" && password !== "";

  return (
    <section className="card" aria-label="Conta do Organize">
      <h2>Organize</h2>
      <p className="muted">
        Liga os widgets de calendário, próximos eventos e tarefas da área de trabalho ao seu Organize.
        O WON entra com a sua conta (só leitura) e guarda apenas a sessão, nunca a senha. A sessão é
        separada da do app Organize.
      </p>
      {st?.connected ? (
        <>
          <p role="status">
            Conectado como <strong>{st.email || "—"}</strong>
            {st.fetched_at > 0
              ? `. ${st.events} eventos e ${st.tasks} tarefas, atualizado às ${new Date(st.fetched_at * 1000).toLocaleTimeString("pt-BR")}.`
              : ". Aguardando a primeira atualização."}
          </p>
          {st.error && (
            <p role="alert" className="form-error">
              Última atualização falhou: {st.error}
            </p>
          )}
          <div className="ram-actions">
            <button type="button" className="btn" disabled={busy} onClick={() => void run(organizeRefresh)}>
              Atualizar agora
            </button>
            <button type="button" className="btn" disabled={busy} onClick={() => void run(async () => organizeLogout())}>
              Desconectar
            </button>
          </div>
        </>
      ) : (
        <form
          className="org-form"
          onSubmit={(e) => {
            e.preventDefault();
            if (canLogin) void run(() => organizeLogin(url, key, email, password));
          }}
        >
          <label>
            Endereço do projeto (Supabase)
            <input type="url" value={url} placeholder="https://xxxx.supabase.co" onChange={(e) => setUrl(e.target.value)} />
          </label>
          <label>
            Chave pública (anon key)
            <input type="text" value={key} autoComplete="off" onChange={(e) => setKey(e.target.value)} />
          </label>
          <label>
            E-mail
            <input type="email" value={email} autoComplete="username" onChange={(e) => setEmail(e.target.value)} />
          </label>
          <label>
            Senha
            <input type="password" value={password} autoComplete="current-password" onChange={(e) => setPassword(e.target.value)} />
          </label>
          <button type="submit" className="btn btn-primary" disabled={busy || !canLogin}>
            {busy ? "Entrando…" : "Conectar"}
          </button>
        </form>
      )}
      {error && !st?.error && (
        <p role="alert" className="form-error">
          {error}
        </p>
      )}
    </section>
  );
}
