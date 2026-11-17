# Sentinela — Fases 0 e 1: workspace e janela — Plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Dividir o crate em workspace (`core` + `cli`) sem mudar o CLI, e entregar um aplicativo de janela (Tauri 2) com as páginas Início, Limpeza, Segurança, Disco e Ajustes sobre as funções que já existem, distribuído por instalador.

**Architecture:** O código de negócio vai para `crates/core` (lib `sentinela_core`); `crates/cli` fica com `cli.rs`, a barra de progresso do terminal e os relatórios em texto. A janela é `app/src-tauri` (bin `sentinela-gui`): comandos Tauri finos que chamam o `core`, rodam trabalhos longos em thread e emitem eventos `job:progress` / `job:done` / `job:error`. O frontend é React + TypeScript + Vite em `app/src`.

**Tech Stack:** Rust estável (edição 2024), Tauri 2, React, TypeScript, Vite, Vitest + Testing Library, GitHub Actions (`windows-latest`).

**Spec:** `docs/superpowers/specs/2026-11-16-sentinela-gui-otimizacao-design.md` (§3, §5, §5.1, §7, §10, §11 fases 0 e 1). Este plano cobre **só as fases 0 e 1**. As fases 2, 3, 4, S1 e S2 ganham planos próprios depois, porque os tipos exatos delas dependem do que este plano deixar pronto.

## Global Constraints

- Só Windows. CI roda só em `windows-latest`. Não escrever código de compatibilidade com outros sistemas.
- Toda tarefa termina com `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings` e `cargo test --workspace` verdes.
- O CLI não muda: mesmos comandos, mesma saída, mesmos códigos; `crates/cli/tests/cli.rs` passa sem editar asserções. O binário continua se chamando `sentinela`.
- `app/src-tauri` não contém regra de negócio: valida entrada, chama o `core`, emite eventos.
- Nada é apagado, movido ou alterado sem o usuário ver antes o que vai mudar e confirmar.
- A verificação pela janela **nunca** move arquivos (não usar `run_full`).
- O app abre sem elevação; itens que exigem administrador aparecem bloqueados com "Reiniciar como administrador".
- Todo texto da interface em pt-BR.
- Distribuição só por instalador NSIS; sem versão portátil.
- Estado do app em `%LOCALAPPDATA%\Sentinela\`.
- Nenhum malware real em testes: EICAR e arquivos gerados.
- A chave da abuse.ch nunca é impressa, registrada em log nem devolvida ao frontend.

## Review Focus

Entradas que o design não descreve e que vão acontecer com um usuário real. Cada uma tem o teste na tarefa indicada.

1. **Caminho com Unicode inválido** (nome de arquivo com surrogate solto, comum em lixo de Temp): o relatório inteiro não pode falhar ao serializar; o caminho aparece com `�`. → Tarefa 2.
2. **Plano de limpeza com centenas de milhares de arquivos:** a janela não pode receber a lista inteira; recebe totais e uma amostra de 200 por categoria. → Tarefa 7.
3. **Clique duplo em "Verificar" / "Limpar":** a segunda operação do mesmo tipo é recusada com mensagem, não roda em paralelo. → Tarefa 5.
4. **Fechar a janela com uma operação em andamento:** todas as operações são canceladas antes de o processo sair; a varredura para no próximo arquivo. → Tarefas 5 e 6.
5. **Pasta-alvo inexistente** (Downloads redirecionado para um OneDrive desconectado, pendrive removido): a verificação rápida pula a pasta e diz qual; se não sobrar nenhuma, mostra erro em vez de "nada encontrado". → Tarefa 8.

---

## Estrutura de arquivos ao final

```
Cargo.toml                         # [workspace]
crates/core/                       # pacote sentinela-core, lib sentinela_core
  src/lib.rs
  src/{walker,platform_fs,finder,full,quarantine,signatures,updater}.rs   # movidos de src/
  src/scan/ src/clean/             # movidos de src/
  src/serde_path.rs                # novo: serialização tolerante de caminhos
  src/status.rs                    # novo: estado do banco de assinaturas
  src/activity.rs                  # novo: registro de atividade recente
  src/secrets.rs                   # novo: Gerenciador de Credenciais
  src/clean/summary.rs             # novo: resumo da limpeza para a janela
  src/scan/targets.rs              # novo: alvos de rápida/completa/personalizada
crates/cli/                        # pacote sentinela, bin sentinela
  src/{main,cli,progress,report}.rs  # movidos de src/
  tests/cli.rs                     # movido de tests/
app/
  package.json  vite.config.ts  index.html
  src/                             # React
    main.tsx  App.tsx  api.ts  jobs.ts  format.ts
    components/{Sidebar,ConfirmDialog,ProgressBar,FindingList}.tsx
    pages/{Inicio,Limpeza,Seguranca,Disco,Ajustes}.tsx
    styles/{tokens,app}.css
  src-tauri/                       # pacote sentinela-gui
    Cargo.toml  tauri.conf.json  capabilities/default.json
    src/{main,lib,jobs}.rs
    src/commands/{system,clean,scan,signatures,disk}.rs
```

---

## Fase 0

### Task 1: Workspace `core` + `cli`

**Files:**
- Modify: `Cargo.toml` (vira manifesto de workspace), `.github/workflows/ci.yml`, `.github/workflows/release.yml`
- Create: `crates/core/Cargo.toml`, `crates/core/src/lib.rs`, `crates/cli/Cargo.toml`
- Move (`git mv`): `src/{walker,platform_fs,finder,full,quarantine,signatures,updater}.rs`, `src/scan/`, `src/clean/` → `crates/core/src/`; `src/{main,cli,progress,report}.rs` → `crates/cli/src/`; `tests/cli.rs` → `crates/cli/tests/cli.rs`

**Interfaces:**
- Produces: crate `sentinela_core` com os módulos públicos `clean, finder, full, platform_fs, quarantine, scan, signatures, updater, walker` (mesmos itens de hoje). Pacote `sentinela` com o bin `sentinela`.

- [ ] **Step 1: Registrar a linha de base**

Run: `cargo test 2>&1 | Select-String "test result"`
Anotar o total de testes aprovados; o Step 5 precisa do mesmo total.

- [ ] **Step 2: Mover os arquivos com `git mv`** conforme a lista acima. Nada é reescrito neste passo.

- [ ] **Step 3: Escrever os manifestos**

  - Raiz: `[workspace] members = ["crates/core", "crates/cli"]`, `resolver = "3"`, e `[workspace.dependencies]` com as versões exatas do `Cargo.toml` atual. `[workspace.package]` com `version = "0.1.0"`, `edition = "2024"`.
  - `crates/core`: `name = "sentinela-core"`. Dependências: só as que os arquivos movidos importam (todas as atuais menos `clap` e `ctrlc`), mais o bloco `windows-sys` atual. Dev: `tempfile`.
  - `crates/cli`: `name = "sentinela"`, `[[bin]] name = "sentinela" path = "src/main.rs"`. Dependências: `sentinela-core` (path), `anyhow`, `clap`, `ctrlc`, `globset`, `serde_json`. Dev: `assert_cmd`, `predicates`, `tempfile`, `sha2`, `serde_json`.
  - `crates/core/src/lib.rs`: os `pub mod` de hoje menos `cli`, `progress` e `report`.
  - `crates/cli/src/main.rs`: declara `mod cli; mod progress; mod report;` e chama `cli::run()`.

- [ ] **Step 4: Corrigir os imports em `crates/cli/src/`**

Em `cli.rs`, `progress.rs` e `report.rs` (inclusive nos `mod tests` do `report.rs`): `crate::{clean,finder,full,platform_fs,quarantine,scan,signatures,updater,walker}` → `sentinela_core::…`. `crate::progress` e `crate::report` continuam `crate::`. `updater::now_rfc3339` é `pub(crate)` e não é usado fora do core; não mexer.

- [ ] **Step 5: Verificar**

Run: `cargo fmt --check; cargo clippy --workspace --all-targets -- -D warnings; cargo test --workspace`
Expected: tudo verde, mesmo total de testes do Step 1.
Run: `cargo run -p sentinela -- info`
Expected: imprime `sentinela 0.1.0` e o estado do banco, como antes.

- [ ] **Step 6: Ajustar os workflows**

  - `ci.yml`: `matrix.os` só com `windows-latest`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace`.
  - `release.yml`: `cargo build --release -p sentinela` (o caminho `target/release/sentinela.exe` não muda).

- [ ] **Step 7: Commit** — `refactor: split into core and cli workspace crates`

### Task 2: API do core pronta para a janela

**Files:**
- Create: `crates/core/src/serde_path.rs`, `crates/core/src/status.rs`
- Modify: `crates/core/src/lib.rs`, `walker.rs`, `scan/mod.rs`, `finder.rs`, `full.rs`, `clean/category.rs`, `updater.rs`; `crates/cli/src/cli.rs`, `crates/cli/src/report.rs`

**Interfaces:**
- Produces:
  - `serde_path::lossy(&PathBuf, S) -> Result<S::Ok, S::Error>`, `serde_path::lossy_vec(&Vec<PathBuf>, S)`, `serde_path::lossy_pairs(&Vec<(PathBuf, String)>, S)` — para `#[serde(serialize_with = "…")]`.
  - `finder::FindResult<T> { pub items: Vec<T>, pub errors: Vec<WalkError>, pub interrupted: bool }` (`Serialize`).
  - `finder::find_large(roots: &[PathBuf], top: usize, min_size: u64, cancel: &AtomicBool) -> FindResult<LargeFile>`
  - `finder::find_dupes(roots: &[PathBuf], min_size: u64, cancel: &AtomicBool) -> FindResult<DupGroup>`
  - `status::DbStatus { pub dir: PathBuf, pub initialized: bool, pub sources: Vec<SourceInfo>, pub warnings: Vec<String> }` (`Serialize`), `status::db_status(dir: &Path) -> DbStatus`
  - `updater::UpdateSummary` passa a derivar `Serialize` e `Clone`.

- [ ] **Step 1: Teste que falha — caminho inválido não derruba a serialização** (em `serde_path.rs`)

```rust
#[test]
fn invalid_unicode_path_serializes_lossily() {
    use std::os::windows::ffi::OsStringExt;
    let p = PathBuf::from(std::ffi::OsString::from_wide(&[0x61, 0xD800, 0x62]));
    let e = crate::walker::WalkError { path: p, reason: "x".into() };
    let s = serde_json::to_string(&e).expect("must not fail");
    assert!(s.contains("a\u{FFFD}b"));
}
```

- [ ] **Step 2:** Run `cargo test -p sentinela-core invalid_unicode` — Expected: FAIL (`serde_json` devolve erro de "path contains invalid UTF-8").

- [ ] **Step 3: Implementar `serde_path` e aplicar** com `serialize_with` em: `WalkError.path`, `ScanItem.path`, `LargeFile.path`, `DupGroup.paths`, `Candidate.path`, `ApplyReport.skipped`, `QuarantineFailure.path`, `DbStatus.dir`. **Não** aplicar em `QuarantineEntry` (ele é lido de volta do `index.json`; fora do escopo).

- [ ] **Step 4: Testes que falham — cancelamento no `finder`** (em `finder.rs`)

```rust
#[test]
fn find_large_stops_when_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.bin"), vec![0u8; 10]).unwrap();
    let cancel = AtomicBool::new(true);
    let r = find_large(&[dir.path().to_path_buf()], 20, 0, &cancel);
    assert!(r.interrupted);
    assert!(r.items.is_empty());
}
#[test]
fn find_dupes_reports_not_interrupted_on_full_run() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a"), b"same").unwrap();
    std::fs::write(dir.path().join("b"), b"same").unwrap();
    let r = find_dupes(&[dir.path().to_path_buf()], 1, &AtomicBool::new(false));
    assert!(!r.interrupted);
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].paths.len(), 2);
}
```

- [ ] **Step 5: Mudar as assinaturas do `finder`** conforme Interfaces. `cancel` é lido a cada item do percurso e antes de cada hash; ao cancelar, devolve o que já tinha com `interrupted = true` (em `find_large`, lista vazia se o percurso não terminou, porque um "top N" parcial engana). Adaptar os testes existentes do `finder` e as duas chamadas em `cli.rs` (passam `&AtomicBool::new(false)` e desmontam `FindResult`); `report::write_finder_*` não muda.

- [ ] **Step 6: Teste e implementação de `status::db_status`**

```rust
#[test]
fn empty_dir_is_not_initialized() {
    let dir = tempfile::tempdir().unwrap();
    let s = db_status(dir.path());
    assert!(!s.initialized);
    assert!(s.sources.is_empty());
}
```

`initialized` é `true` quando `signatures::load(dir).manifest` existe e tem fontes. `cmd_info` em `cli.rs` passa a usar `db_status`, com a mesma saída.

- [ ] **Step 7:** Run `cargo test --workspace` — Expected: PASS, inclusive `crates/cli/tests/cli.rs` sem alterações de asserção.

- [ ] **Step 8: Commit** — `feat(core): lossy path serialization, finder cancellation, db status`

### Task 3: Registro de atividade

**Files:**
- Create: `crates/core/src/activity.rs`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Produces:

```rust
pub const MAX_ENTRIES: usize = 200;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivityKind { Clean, Scan, Quarantine, Restore, Update }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEntry { pub at: String /* RFC 3339 UTC */, pub kind: ActivityKind, pub summary: String }
pub fn default_activity_path() -> PathBuf;   // %LOCALAPPDATA%\Sentinela\activity.json
pub fn append(path: &Path, kind: ActivityKind, summary: &str) -> anyhow::Result<()>;
pub fn recent(path: &Path, n: usize) -> Vec<ActivityEntry>;   // mais novo primeiro
```

- [ ] **Step 1: Testes que falham**

```rust
#[test]
fn recent_returns_newest_first() {
    let d = tempfile::tempdir().unwrap(); let p = d.path().join("a.json");
    append(&p, ActivityKind::Clean, "um").unwrap();
    append(&p, ActivityKind::Scan, "dois").unwrap();
    append(&p, ActivityKind::Update, "tres").unwrap();
    let r = recent(&p, 2);
    assert_eq!(r.iter().map(|e| e.summary.as_str()).collect::<Vec<_>>(), ["tres", "dois"]);
}
#[test]
fn file_is_capped_at_max_entries() {
    let d = tempfile::tempdir().unwrap(); let p = d.path().join("a.json");
    for i in 0..MAX_ENTRIES + 5 { append(&p, ActivityKind::Clean, &i.to_string()).unwrap(); }
    assert_eq!(recent(&p, usize::MAX).len(), MAX_ENTRIES);
    assert_eq!(recent(&p, 1)[0].summary, (MAX_ENTRIES + 4).to_string());
}
#[test]
fn corrupt_file_reads_as_empty_and_is_replaced_on_append() {
    let d = tempfile::tempdir().unwrap(); let p = d.path().join("a.json");
    std::fs::write(&p, b"{not json").unwrap();
    assert!(recent(&p, 10).is_empty());
    append(&p, ActivityKind::Scan, "ok").unwrap();
    assert_eq!(recent(&p, 10).len(), 1);
}
```

- [ ] **Step 2:** Run `cargo test -p sentinela-core activity` — Expected: FAIL (módulo não existe).
- [ ] **Step 3: Implementar.** Arquivo é um array JSON; `append` cria a pasta, grava em arquivo temporário ao lado e renomeia por cima. Usa `updater::now_rfc3339()` para `at`.
- [ ] **Step 4:** Run `cargo test -p sentinela-core activity` — Expected: PASS.
- [ ] **Step 5: Commit** — `feat(core): recent activity log`

---

## Fase 1

### Task 4: Esqueleto da janela

**Files:**
- Create: `app/` inteiro (ver estrutura), com `App.tsx`, `components/Sidebar.tsx`, `styles/tokens.css`, `styles/app.css`, cinco páginas com só o título, `app/src/App.test.tsx`
- Modify: `Cargo.toml` (members), `.github/workflows/ci.yml`, `.gitignore` (`node_modules`, `app/dist`)

**Interfaces:**
- Produces: pacote `sentinela-gui` em `app/src-tauri` no workspace; `App` com navegação por estado local (`type Page = "inicio" | "limpeza" | "seguranca" | "disco" | "ajustes"`), sem biblioteca de rotas; `Sidebar({ current, onNavigate, elevated })`.

- [ ] **Step 1: Conferir o ambiente**

Run: `node --version; npm --version`
Expected: Node 20 ou mais novo. Se faltar, parar e pedir ao usuário para instalar o Node LTS; não instalar por conta própria.

- [ ] **Step 2: Gerar o projeto** com o gerador oficial do Tauri 2, template React + TypeScript, gerenciador npm, na pasta `app/`. Conferir na documentação atual do Tauri as flags não interativas do `create-tauri-app` antes de rodar. Depois: nome do pacote Rust `sentinela-gui`; `productName` `Sentinela`; `identifier` `io.github.luz-lucas.sentinela`; janela 1100×720, mínimo 900×600, título `Sentinela`; apagar o comando `greet` e o conteúdo de exemplo; adicionar `app/src-tauri` aos `members`; dependência `sentinela-core` por path.

- [ ] **Step 3: Plugins e permissões.** Adicionar `tauri-plugin-dialog`, `tauri-plugin-opener` e `tauri-plugin-single-instance` (segunda instância só traz a primeira para frente). `capabilities/default.json` com apenas `core:default`, `dialog:allow-open` e `opener:allow-reveal-item-in-dir`. Sem acesso a conteúdo remoto.

- [ ] **Step 4: Teste que falha** (`app/src/App.test.tsx`, Vitest + Testing Library + jsdom; `@tauri-apps/api/core` simulado com `vi.mock`)

```tsx
it("mostra as cinco páginas e troca de página", async () => {
  render(<App />);
  for (const n of ["Início", "Limpeza", "Segurança", "Disco", "Ajustes"])
    expect(screen.getByRole("button", { name: n })).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Limpeza" }));
  expect(screen.getByRole("heading", { level: 1, name: "Limpeza" })).toBeInTheDocument();
});
```

- [ ] **Step 5:** Run `npm test` em `app/` — Expected: FAIL.
- [ ] **Step 6: Implementar `App`, `Sidebar` e as páginas vazias.** A barra lateral só lista páginas que existem nesta fase. Direção visual: tema escuro, denso, com cor de destaque única; variáveis em `tokens.css`. Usar a skill `frontend-design` para a direção visual antes de escrever o CSS.
- [ ] **Step 7:** Run `npm test` — Expected: PASS. Run `npm run tauri dev` — Expected: a janela abre com a barra lateral e troca de página.
- [ ] **Step 8: CI.** Antes dos passos de cargo: `actions/setup-node@v4` (Node 22), e em `app/`: `npm ci`, `npm test`, `npm run build` (o `dist` precisa existir para o `sentinela-gui` compilar).
- [ ] **Step 9: Commit** — `feat(gui): tauri window skeleton with sidebar`

### Task 5: Infraestrutura de operações longas

**Files:**
- Create: `app/src-tauri/src/jobs.rs`, `app/src/jobs.ts`
- Modify: `app/src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `sentinela_core::scan::Progress`.
- Produces (Rust):

```rust
pub type JobId = u64;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind { Scan, Clean, FindLarge, FindDupes, Update }
pub trait Sink: Send + Sync { fn emit(&self, event: &str, payload: serde_json::Value); }
pub struct JobRegistry;  // estado gerenciado pelo Tauri, dentro de Arc
impl JobRegistry {
    pub fn new() -> Self;
    pub fn start(&self, kind: JobKind) -> Result<(JobId, Arc<AtomicBool>), String>;
    pub fn finish(&self, id: JobId);
    pub fn cancel(&self, id: JobId) -> bool;
    pub fn cancel_all(&self);
}
pub fn spawn_job<T, F>(reg: Arc<JobRegistry>, sink: Arc<dyn Sink>, kind: JobKind, work: F) -> Result<JobId, String>
where T: Serialize + Send + 'static,
      F: FnOnce(JobId, &AtomicBool, Arc<dyn Sink>) -> Result<T, String> + Send + 'static;
pub struct EventProgress;  // impl sentinela_core::scan::Progress
impl EventProgress { pub fn new(job: JobId, sink: Arc<dyn Sink>, interval: Duration) -> Self; }
```

- Eventos (nomes e formato fixos):
  - `job:progress` → `{ job, phase: "counting", files_seen }` ou `{ job, phase: "scanning", files_done, total_files, bytes_done, total_bytes, current }`
  - `job:done` → `{ job, kind, result }`
  - `job:error` → `{ job, kind, message }`
- Mensagem exata quando já há operação do mesmo tipo: `Já existe uma operação deste tipo em andamento.`
- Produces (TS): `useJob<T>()` → `{ state: "idle" | "running" | "done" | "error", progress, result, error, start(invokeFn), cancel() }`, que escuta os três eventos filtrando por `job`.

- [ ] **Step 1: Testes que falham** (em `jobs.rs`, com um `Sink` que grava os eventos num `Mutex<Vec<(String, Value)>>`)

```rust
#[test] fn second_job_of_same_kind_is_refused() {
    let r = JobRegistry::new();
    let (_id, _c) = r.start(JobKind::Scan).unwrap();
    assert_eq!(r.start(JobKind::Scan).unwrap_err(), "Já existe uma operação deste tipo em andamento.");
    assert!(r.start(JobKind::Clean).is_ok());
}
#[test] fn finish_frees_the_kind() {
    let r = JobRegistry::new();
    let (id, _) = r.start(JobKind::Scan).unwrap(); r.finish(id);
    assert!(r.start(JobKind::Scan).is_ok());
}
#[test] fn cancel_all_sets_every_flag() {
    let r = JobRegistry::new();
    let (_, a) = r.start(JobKind::Scan).unwrap(); let (_, b) = r.start(JobKind::Clean).unwrap();
    r.cancel_all();
    assert!(a.load(Ordering::Relaxed) && b.load(Ordering::Relaxed));
}
#[test] fn spawn_job_emits_done_and_frees_registry() { /* work devolve Ok(7): um evento job:done com result == 7; start(kind) volta a funcionar */ }
#[test] fn spawn_job_emits_error_on_err_and_on_panic() { /* Err("x") e panic!() geram job:error e liberam o registro */ }
#[test] fn progress_is_throttled_and_never_goes_backwards() {
    /* interval = 1 h: advance(5,..), advance(3,..), advance(9,..), finish() → no máximo 1 evento "scanning" antes do finish,
       e o último evento "scanning" tem files_done == 9 */
}
```

- [ ] **Step 2:** Run `cargo test -p sentinela-gui jobs` — Expected: FAIL.
- [ ] **Step 3: Implementar.** `spawn_job` roda `work` em `std::thread::spawn` dentro de `catch_unwind`; sempre chama `finish`. `EventProgress` guarda o máximo visto, emite no máximo um evento por `interval` e um evento final no `finish`. Em `lib.rs`: `struct TauriSink(AppHandle)` implementando `Sink` com `emit`; `JobRegistry` em `Arc` como estado gerenciado.
- [ ] **Step 4: `useJob` no frontend** com teste: ao receber `job:done` de outro `job`, o estado não muda; do próprio, vira `done` com `result`.
- [ ] **Step 5:** Run `cargo test -p sentinela-gui; npm test` — Expected: PASS.
- [ ] **Step 6: Commit** — `feat(gui): job registry, progress events and useJob hook`

### Task 6: Sistema e página Início

**Files:**
- Create: `app/src-tauri/src/commands/system.rs`, `app/src/api.ts`, `app/src/format.ts`, `app/src/format.test.ts`
- Modify: `app/src-tauri/src/lib.rs`, `app/src/pages/Inicio.tsx`, `app/src/components/Sidebar.tsx`, `app/src-tauri/Cargo.toml` (`windows-sys` com `Win32_UI_Shell`, `Win32_Foundation`)

**Interfaces:**
- Consumes: `status::db_status`, `activity::recent`, `quarantine::list`, `clean::platform::is_elevated`, `JobRegistry`.
- Produces (comandos):
  - `system_status() -> SystemStatus { version: String, elevated: bool, db: DbStatus, quarantine_count: usize, activity: Vec<ActivityEntry> /* 10 mais recentes */ }`
  - `relaunch_elevated() -> Result<(), String>` — `ShellExecuteW` com verbo `runas` no próprio executável; UAC recusado → `Err("Elevação cancelada.")` e o app continua aberto; aceito → o processo atual sai.
  - `job_cancel(id: JobId) -> bool`
- Produces (TS): `api.ts` com um wrapper tipado por comando e os tipos espelhando o serde; `format.ts` com `humanBytes(n: number): string` (mesmas regras do `report::human_bytes`: `0 B`, `1.0 KB`, unidades binárias até TB) e `humanDate(rfc3339: string): string` em pt-BR.

- [ ] **Step 1: Teste que falha** (`format.test.ts`)

```ts
it.each([[0, "0 B"], [1023, "1023 B"], [1024, "1.0 KB"], [1536, "1.5 KB"], [1048576, "1.0 MB"]])(
  "humanBytes(%d) = %s", (n, s) => expect(humanBytes(n)).toBe(s));
```

- [ ] **Step 2:** Run `npm test` — Expected: FAIL. Implementar `format.ts`. Run — Expected: PASS.
- [ ] **Step 3: Implementar os comandos** e registrá-los. No evento de fechamento da janela principal, chamar `JobRegistry::cancel_all()` antes de sair.
- [ ] **Step 4: Página Início.** Três cartões: **Proteção** (idade do banco ou "Banco não inicializado", itens em quarentena, botão que leva a Segurança), **Espaço** (botão que leva a Limpeza), **Sistema** (versão). Lista "Atividade recente". Rodapé da barra lateral: "Administrador: sim/não" e, quando não, o botão "Reiniciar como administrador". Aviso fixo na página: "O Sentinela não substitui um antivírus completo. Mantenha o Microsoft Defender ativo."
- [ ] **Step 5: Teste de componente:** com `system_status` simulado devolvendo `elevated: false`, o botão "Reiniciar como administrador" aparece; com `true`, não aparece.
- [ ] **Step 6: Verificação manual.** `npm run tauri dev`: Início mostra versão e estado do banco; recusar o UAC mantém o app aberto e mostra "Elevação cancelada."
- [ ] **Step 7: Commit** — `feat(gui): system status, elevation and home page`

### Task 7: Limpeza

**Files:**
- Create: `crates/core/src/clean/summary.rs`, `app/src-tauri/src/commands/clean.rs`, `app/src/components/ConfirmDialog.tsx`, `app/src/pages/Limpeza.test.tsx`
- Modify: `crates/core/src/clean/mod.rs`, `app/src/pages/Limpeza.tsx`, `app/src/api.ts`

**Interfaces:**
- Consumes: `clean::run_clean`, `clean::all_categories`, `Env::from_system`, `spawn_job`, `activity::append`.
- Produces (core):

```rust
pub const SAMPLE_LIMIT: usize = 200;
#[derive(Debug, Clone, Serialize)]
pub struct CategorySummary {
    pub id: String, pub needs_admin: bool,
    pub files: u64, pub bytes: u64,
    pub warnings: Vec<String>, pub skipped_reason: Option<String>,
    pub sample: Vec<Candidate>,            // no máximo SAMPLE_LIMIT
    pub deleted: Option<u64>, pub freed: Option<u64>, pub skipped_files: u64,
}
#[derive(Debug, Clone, Serialize)]
pub struct CleanSummary {
    pub categories: Vec<CategorySummary>,
    pub total_files: u64, pub total_bytes: u64,
    pub deleted: u64, pub freed: u64, pub elevated: bool,
}
pub fn summarize(report: &CleanReport, cats: &[Box<dyn Category>], elevated: bool) -> CleanSummary;
pub fn validate_selection(selected: &[String], cats: &[Box<dyn Category>]) -> Result<(), String>;
// Err: "categoria desconhecida: {s}. Categorias válidas: {lista separada por vírgula}"
```

- Produces (comandos): `clean_plan(selected: Vec<String>) -> Result<CleanSummary, String>` (simulação, síncrono em thread de bloqueio); `clean_apply(selected: Vec<String>) -> Result<JobId, String>` (`JobKind::Clean`, resultado `CleanSummary`). Os totais contam só categorias não puladas.

- [ ] **Step 1: Testes que falham** (`summary.rs`)

```rust
#[test] fn sample_is_capped_but_totals_are_not() {
    /* CleanReport com uma categoria de 1.000 Candidates de 1 byte:
       sample.len() == SAMPLE_LIMIT; files == 1000; total_files == 1000; total_bytes == 1000 */
}
#[test] fn skipped_category_does_not_count_towards_totals() { /* skipped_reason = Some(..) → total_files == 0 */ }
#[test] fn unknown_category_is_rejected() {
    let cats = crate::clean::all_categories();
    let e = validate_selection(&["nao-existe".into()], &cats).unwrap_err();
    assert!(e.starts_with("categoria desconhecida: nao-existe."));
}
```

- [ ] **Step 2:** Run `cargo test -p sentinela-core summary` — Expected: FAIL. Implementar. Run — Expected: PASS. Trocar a validação duplicada em `cli.rs::cmd_clean` por `validate_selection` (mesma mensagem).
- [ ] **Step 3: Comandos.** `clean_apply` chama `run_clean` com `apply = true` e `confirm` devolvendo `true` (a confirmação é da janela), e ao terminar faz `activity::append(Clean, "Limpeza: {freed legível} liberados")`.
- [ ] **Step 4: Teste de componente que falha** (`Limpeza.test.tsx`, `api` simulada)

```tsx
it("só apaga depois da confirmação e avisa da Lixeira", async () => {
  mockPlan({ categories: [cat("user-temp", 3, 300), cat("recycle-bin", 2, 200)], total_files: 5, total_bytes: 500 });
  render(<Limpeza />);
  await userEvent.click(await screen.findByRole("button", { name: "Limpar" }));
  expect(api.cleanApply).not.toHaveBeenCalled();
  expect(screen.getByText("A Lixeira será esvaziada permanentemente.")).toBeInTheDocument();
  expect(screen.getByText(/5 arquivo\(s\), 500 B/)).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Apagar" }));
  expect(api.cleanApply).toHaveBeenCalledWith(["user-temp", "recycle-bin"]);
});
it("categoria que exige administrador aparece bloqueada", async () => { /* skipped_reason preenchido → checkbox desabilitado e o motivo visível */ });
```

- [ ] **Step 5: Página Limpeza.** Ao abrir, chama `clean_plan([])`. Uma linha por categoria: checkbox (marcado por padrão quando não pulada), nome em pt-BR, arquivos, tamanho, avisos, e "ver amostra" que expande os `sample`. Nomes: `user-temp` "Temporários do usuário", `windows-temp` "Temporários do Windows", `crash-dumps` "Despejos de erro", `browser-cache` "Cache de navegadores", `recycle-bin` "Lixeira", `windows-update` "Cache do Windows Update". Botão "Limpar" abre o `ConfirmDialog`; depois de aplicar, mostra apagados/liberados/pulados e refaz a simulação. "Limpar" fica desabilitado enquanto há operação em andamento.
- [ ] **Step 6:** Run `npm test; cargo test --workspace` — Expected: PASS.
- [ ] **Step 7: Verificação manual** em pasta de teste: criar arquivos antigos em `%TEMP%`, conferir que a simulação os lista e que nada some antes de confirmar.
- [ ] **Step 8: Commit** — `feat(gui): cleaning page with preview and confirmation`

### Task 8: Segurança

**Files:**
- Create: `crates/core/src/scan/targets.rs`, `app/src-tauri/src/commands/scan.rs`, `app/src/components/FindingList.tsx`, `app/src/components/ProgressBar.tsx`, `app/src/pages/Seguranca.test.tsx`
- Modify: `crates/core/src/scan/mod.rs`, `app/src/pages/Seguranca.tsx`, `app/src/api.ts`

**Interfaces:**
- Consumes: `Engine::from_database`, `run_scan_with_progress`, `signatures::{load, default_db_dir}`, `platform_fs::fixed_drive_roots`, `quarantine::{list, restore, default_quarantine_dir}`, `EventProgress`, `spawn_job`.
- Produces (core):

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ScanTarget { Quick, Full, Custom { paths: Vec<PathBuf> } }
pub struct Resolved { pub roots: Vec<PathBuf>, pub missing: Vec<PathBuf> }
/// `quick` e `drives` vêm de fora para o teste não depender da máquina.
pub fn resolve(target: &ScanTarget, quick: &[PathBuf], drives: &[PathBuf]) -> Result<Resolved, String>;
// Err quando não sobra nenhuma raiz: "Nenhuma pasta disponível para verificar."
pub fn quick_folders() -> Vec<PathBuf>;   // dirs::download_dir() e dirs::desktop_dir(), os que existirem como Some
pub const ITEMS_LIMIT: usize = 1000;
pub const ERRORS_LIMIT: usize = 200;
#[derive(Serialize)]
pub struct ScanSummary {
    pub scanned: usize, pub interrupted: bool,
    pub malicious: usize, pub suspicious: usize,
    pub items: Vec<ScanItem>, pub items_total: usize,       // items ordenados: Malicious primeiro
    pub errors: Vec<WalkError>, pub errors_total: usize,
    pub missing: Vec<String>, pub db_initialized: bool, pub warnings: Vec<String>,
}
pub fn summarize(report: ScanReport, missing: &[PathBuf], db_initialized: bool, warnings: Vec<String>) -> ScanSummary;
```

- Produces (comandos): `scan_start(target: ScanTarget) -> Result<JobId, String>` (`JobKind::Scan`, resultado `ScanSummary`, `max_size` 256 MB, **não move nada**); `quarantine_list() -> Vec<QuarantineEntry>`; `quarantine_restore(id: String) -> Result<String, String>` (devolve o caminho restaurado).

- [ ] **Step 1: Testes que falham** (`targets.rs`)

```rust
#[test] fn quick_skips_missing_folders_and_reports_them() {
    let d = tempfile::tempdir().unwrap();
    let gone = d.path().join("sumiu");
    let r = resolve(&ScanTarget::Quick, &[d.path().to_path_buf(), gone.clone()], &[]).unwrap();
    assert_eq!(r.roots, [d.path().to_path_buf()]);
    assert_eq!(r.missing, [gone]);
}
#[test] fn nothing_left_is_an_error_not_a_clean_result() {
    let e = resolve(&ScanTarget::Quick, &[PathBuf::from("Z:\\nao\\existe")], &[]).unwrap_err();
    assert_eq!(e, "Nenhuma pasta disponível para verificar.");
}
#[test] fn full_uses_the_drives() { /* Full + drives = [tempdir] → roots == [tempdir] */ }
#[test] fn custom_with_no_paths_is_an_error() { /* mesma mensagem */ }
#[test] fn summary_caps_items_and_puts_malicious_first() {
    /* 1.500 itens Suspicious + 1 Malicious no fim → items.len() == ITEMS_LIMIT, items[0].verdict == Malicious,
       items_total == 1501, malicious == 1, suspicious == 1500 */
}
```

- [ ] **Step 2:** Run `cargo test -p sentinela-core targets` — Expected: FAIL. Implementar. Run — Expected: PASS.
- [ ] **Step 3: Teste de ponta a ponta no core com EICAR:** escrever `signatures::EICAR` num tempdir, `resolve(Custom)` → `run_scan_with_progress` com banco vazio → `summarize`: `malicious == 1`, e o arquivo **continua no disco**.
- [ ] **Step 4: Comandos.** `scan_start` resolve os alvos (erro volta direto, sem criar operação), monta o `Engine`, usa `EventProgress` com intervalo de 100 ms, e ao terminar faz `activity::append(Scan, "Verificação: {scanned} arquivos, {malicious} maliciosos, {suspicious} suspeitos")`. `quarantine_restore` registra `Restore`.
- [ ] **Step 5: Teste de componente que falha** (`Seguranca.test.tsx`)

```tsx
it("mostra achados por nível, o motivo, e as pastas puladas", async () => {
  /* job:done com 1 malicious + 1 suspicious e missing: ["D:\\Downloads"] →
     "Malicioso" aparece antes de "Suspeito"; a regra de cada achado é visível;
     aparece "Não foi possível verificar: D:\Downloads" */
});
it("resultado interrompido diz que é parcial", async () => { /* interrupted: true → "Verificação interrompida: resultado parcial." */ });
it("sem achados não promete segurança", async () => {
  /* malicious 0, suspicious 0 → texto exato: "Nenhuma ameaça conhecida encontrada nos arquivos verificados." */
});
```

- [ ] **Step 6: Página Segurança.** Três botões: **Rápida** (Downloads e Área de Trabalho), **Completa** (todos os discos fixos; aviso de que demora e de que sem administrador a cobertura é parcial), **Pasta…** (seletor do `plugin-dialog`). Durante a operação: `ProgressBar` com porcentagem por bytes, arquivos, arquivo atual, e "Cancelar". Resultado: `FindingList` agrupada por nível com caminho, detector e regra, botão "Abrir pasta" (`revealItemInDir`), seção recolhível "Não verificados" com motivo, e, se `items_total > items.length`, "Mostrando 1.000 de N". Se `db_initialized` é falso: aviso com atalho para Ajustes. Seção **Quarentena**: lista (data, tamanho, caminho original, regra) com "Restaurar"; erro do restore aparece como está. Nesta fase a janela não coloca nada em quarentena (isso é a fase S1).
- [ ] **Step 7:** Run `npm test; cargo test --workspace` — Expected: PASS.
- [ ] **Step 8: Verificação manual:** criar um arquivo EICAR numa pasta de teste (com o Defender podendo removê-lo; se remover, usar exclusão temporária da pasta), verificar a pasta e conferir o achado e que o arquivo não foi tocado.
- [ ] **Step 9: Commit** — `feat(gui): on-demand scan page and quarantine list`

### Task 9: Assinaturas e Ajustes

**Files:**
- Create: `crates/core/src/secrets.rs`, `app/src-tauri/src/commands/signatures.rs`
- Modify: `crates/core/src/lib.rs`, `crates/core/Cargo.toml` (`windows-sys` + `Win32_Security_Credentials`), `crates/cli/src/cli.rs`, `app/src/pages/Ajustes.tsx`, `app/src/api.ts`

**Interfaces:**
- Consumes: `updater::{update, HttpFetcher}`, `spawn_job`, `activity::append`.
- Produces (core):

```rust
pub const ABUSE_CH_KEY: &str = "abuse-ch-auth-key";
pub trait SecretStore: Send + Sync {
    fn get(&self, name: &str) -> Option<String>;
    fn set(&self, name: &str, value: &str) -> anyhow::Result<()>;
    fn delete(&self, name: &str) -> anyhow::Result<()>;
}
pub struct WindowsCredentialStore;   // credencial genérica, alvo "Sentinela/{name}"
pub struct MemoryStore;              // para testes; Default
/// Guardada na janela tem precedência sobre a variável de ambiente. Vazio ou só espaços = ausente.
pub fn resolve_abuse_key(store: &dyn SecretStore, env_value: Option<String>) -> Option<String>;
```

- Produces (comandos): `signatures_update() -> Result<JobId, String>` (`JobKind::Update`, resultado `UpdateSummary`); `abuse_key_set(key: String) -> Result<(), String>`; `abuse_key_clear() -> Result<(), String>`; `abuse_key_present() -> bool`. **Não existe** comando que devolva a chave.

- [ ] **Step 1: Testes que falham** (`secrets.rs`)

```rust
#[test] fn stored_key_wins_over_env() {
    let s = MemoryStore::default(); s.set(ABUSE_CH_KEY, " guardada ").unwrap();
    assert_eq!(resolve_abuse_key(&s, Some("env".into())).as_deref(), Some("guardada"));
}
#[test] fn falls_back_to_env_and_ignores_blank() {
    let s = MemoryStore::default();
    assert_eq!(resolve_abuse_key(&s, Some(" env ".into())).as_deref(), Some("env"));
    assert_eq!(resolve_abuse_key(&s, Some("   ".into())), None);
    s.set(ABUSE_CH_KEY, "").unwrap();
    assert_eq!(resolve_abuse_key(&s, None), None);
}
#[test] #[ignore = "toca no Gerenciador de Credenciais real"] fn windows_store_roundtrip() {
    /* set/get/delete com um nome de teste único; rodar à mão com --ignored */
}
```

- [ ] **Step 2:** Run `cargo test -p sentinela-core secrets` — Expected: FAIL. Implementar (`CredWriteW` / `CredReadW` / `CredDeleteW` / `CredFree`, persistência `CRED_PERSIST_LOCAL_MACHINE`). Run — Expected: PASS; depois `cargo test -p sentinela-core secrets -- --ignored` uma vez à mão.
- [ ] **Step 3:** `cmd_update` no CLI passa a usar `resolve_abuse_key(&WindowsCredentialStore, env)`; sem chave guardada o comportamento é idêntico ao atual.
- [ ] **Step 4: Comandos.** `signatures_update` registra `Update` com `"Assinaturas: {hashes} hashes, {rules_count} regras"`. Erro de autenticação (`updater::AuthRequired`) vira a mensagem "A chave da abuse.ch foi recusada. Confira a chave em Ajustes."
- [ ] **Step 5: Página Ajustes.** Banco de assinaturas: pasta, fontes com data e contagem, botão "Atualizar agora" com resultado e avisos. Chave abuse.ch: campo do tipo senha, "Salvar", "Remover", estado "Chave salva" / "Sem chave (só as regras YARA são atualizadas)", e o link `https://auth.abuse.ch/` para criar a conta gratuita. Teste de componente: depois de "Salvar", o campo é esvaziado e o valor digitado não aparece em nenhum lugar da página.
- [ ] **Step 6:** Run `npm test; cargo test --workspace` — Expected: PASS.
- [ ] **Step 7: Commit** — `feat: signature updates from the window, key in Credential Manager`

### Task 10: Disco

**Files:**
- Create: `app/src-tauri/src/commands/disk.rs`, `app/src/pages/Disco.test.tsx`
- Modify: `app/src/pages/Disco.tsx`, `app/src/api.ts`

**Interfaces:**
- Consumes: `finder::{find_large, find_dupes, FindResult, LargeFile, DupGroup}`, `spawn_job`.
- Produces (comandos):
  - `disk_find_large(path: PathBuf, top: usize, min_mb: u64) -> Result<JobId, String>` (`JobKind::FindLarge`, resultado `FindResult<LargeFile>`)
  - `disk_find_dupes(path: PathBuf, min_kb: u64) -> Result<JobId, String>` (`JobKind::FindDupes`, resultado `FindResult<DupGroup>`)
  - Validação em função pura `validate(path: &Path, top: usize, min: u64, unit: u64) -> Result<(usize, u64), String>`: caminho inexistente → `o caminho não existe: {caminho}`; `top` limitado a 1..=500; estouro na multiplicação → `tamanho mínimo grande demais`.

- [ ] **Step 1: Testes que falham** para `validate`: caminho inexistente; `top = 0` vira 1 e `top = 9999` vira 500; `min = u64::MAX` com `unit = 1024` dá erro.
- [ ] **Step 2:** Run `cargo test -p sentinela-gui disk` — Expected: FAIL. Implementar comandos e `validate`. Run — Expected: PASS.
- [ ] **Step 3: Teste de componente que falha** (`Disco.test.tsx`): com um grupo de 3 caminhos de 100 bytes, a página mostra "Espaço desperdiçado: 200 B"; e **não existe** nenhum botão com nome contendo "Apagar" ou "Excluir".
- [ ] **Step 4: Página Disco.** Escolher pasta ou disco; duas abas: "Maiores arquivos" (quantidade, padrão 20; tamanho mínimo em MB, padrão 0) e "Duplicados" (mínimo em KB, padrão 1). Resultados com "Abrir pasta"; "Não verificados" recolhível; "Cancelar" durante a busca; resultado interrompido marcado como parcial. A página só lista.
- [ ] **Step 5:** Run `npm test; cargo test --workspace` — Expected: PASS.
- [ ] **Step 6: Commit** — `feat(gui): disk page for large and duplicate files`

### Task 11: Instalador e release

**Files:**
- Modify: `app/src-tauri/tauri.conf.json`, `.github/workflows/release.yml`, `README.md`
- Create: `docs/testes-manuais-fase-1.md`

**Interfaces:**
- Consumes: tudo acima.
- Produces: instalador NSIS `Sentinela_<versão>_x64-setup.exe` contendo `sentinela-gui.exe` e `sentinela.exe`.

- [ ] **Step 1: Configurar o pacote.** `bundle.targets = ["nsis"]`; instalação por máquina (pasta gravável só por administrador — pré-requisito do motor de mods do design §9.2); `beforeBuildCommand` compila o frontend e roda `cargo build --release -p sentinela`; `sentinela.exe` entra como recurso do pacote, ao lado do executável da janela. Conferir na documentação atual do Tauri 2 os nomes exatos das chaves de `bundle` e `nsis`.
- [ ] **Step 2: Build local.** Run em `app/`: `npm run tauri build` — Expected: gera o instalador em `target/release/bundle/nsis/`. Instalar numa VM Windows 11 e conferir que os dois executáveis estão na pasta de instalação e que `sentinela.exe info` roda de lá.
- [ ] **Step 3: `release.yml`.** Node 22, `npm ci` e `npm run tauri build` em `app/`; a release anexa o instalador e, como hoje, o `target/release/sentinela.exe` solto.
- [ ] **Step 4: `docs/testes-manuais-fase-1.md`** — roteiro em VM com snapshot: instalar; abrir sem administrador; cada página; recusar e aceitar o UAC; limpeza real de `%TEMP%`; verificação de pasta com EICAR; cancelar uma verificação completa; fechar a janela durante uma verificação e conferir no Gerenciador de Tarefas que o processo saiu; restaurar um item da quarentena criado pelo CLI; atualizar assinaturas com e sem chave; abrir o app duas vezes (só uma janela); desinstalar.
- [ ] **Step 5: README.** Seção "Interface gráfica" no topo de "Download" (instalador, o que cada página faz), trocar "Distribuído como um único `sentinela.exe`" pelo texto novo, tirar a menção a outros sistemas em "Como compilar" e acrescentar os comandos do `app/`.
- [ ] **Step 6:** Rodar o roteiro manual inteiro uma vez e anotar o resultado no próprio arquivo.
- [ ] **Step 7: Commit** — `build: NSIS installer bundling the CLI; docs for the window`
