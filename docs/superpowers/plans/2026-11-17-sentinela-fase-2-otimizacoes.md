# Sentinela — Fase 2: motor de otimizações — Plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Um motor de otimizações do Windows que detecta, mostra a prévia, aplica e desfaz cada ajuste voltando ao valor original da máquina, com o catálogo "Comprovado" para jogos, as páginas Jogos e Histórico na janela e `sentinela tweak list/apply/undo` no CLI.

**Architecture:** `crates/core/src/tweaks/` guarda o modelo, o catálogo embutido (`catalog/tweaks.json` via `include_str!`), o motor (estado, prévia, aplicar, desfazer) e o diário em `%LOCALAPPDATA%\Sentinela\tweaks\journal.json`. Toda leitura e escrita no sistema passa por `trait SystemBackend`, com `WindowsBackend` (registro, tipo de inicialização de serviço, plano de energia) e `MemoryBackend` para testes. CLI e janela são camadas finas sobre o motor.

**Tech Stack:** Rust 2024, `windows-sys` 0.61, serde/serde_json, Tauri 2, React + TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-11-16-sentinela-gui-otimizacao-design.md` — §2 (anti-objetivos), §5.1 (privilégios e HKCU em outra conta), §6.1–6.3 (modelo, fluxo, catálogo), §10 (Jogos, Histórico, cartão "Otimização" do Início), §11 fase 2, §13 (testes de `tweaks` e validação do catálogo).

**Fora deste plano** (fases seguintes): aba "Avançado" e perfil "FPS competitivo" (fase 4), GPU por jogo e gerenciador de inicialização (fase 4), Atualizações do Windows §6.5 (fase 4), Modo Jogo §6.4 e Rede §6.6. `Action` só ganha as variantes que este catálogo usa.

## Global Constraints

- Só Windows; CI só `windows-latest`; builds locais com `CARGO_TARGET_DIR=C:/st`.
- Toda tarefa termina com `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace` e `npm test` (em `app/`) verdes.
- Nunca desligar Defender, Windows Update, firewall, UAC, mitigações de CPU nem integridade de memória. Serviços proibidos no catálogo: `WinDefend`, `WdNisSvc`, `SecurityHealthService`, `mpssvc`, `BFE`, `wuauserv`, `UsoSvc`, `WaaSMedicSvc`, `EventLog`, `CryptSvc`, `TrustedInstaller`.
- Sem scripts arbitrários: só ações tipadas.
- Desfazer devolve o valor que estava antes na máquina, inclusive "valor não existia" (apaga o valor).
- O valor anterior vai para o diário **antes** da escrita. Se o valor atual não é mais o que o Sentinela escreveu, desfazer avisa em vez de sobrescrever.
- Nada é aplicado sem prévia e confirmação.
- O CLI existente não muda; `tweak` é um subcomando novo.
- Textos da interface em pt-BR; `app/src-tauri` sem regra de negócio.
- Ajustes em `HKCU` ficam bloqueados quando a janela elevada roda em **outra** conta (§5.1).

## Review Focus

1. **Queda no meio de uma aplicação** (energia, processo morto entre o diário e a escrita): na próxima abertura, entradas pendentes são reconciliadas — se o valor atual é o alvo, conta como aplicado e desfazível; se é o anterior, a entrada some. → Tarefa 3.
2. **Ajuste aplicado, depois mudado pelo usuário ou pelo Windows:** estado vira "Parcial"/"Não aplicado", e desfazer não sobrescreve o valor novo; diz qual valor foi mantido. → Tarefa 3.
3. **Tipo de valor diferente no registro** (o alvo é DWORD e existe uma string com o mesmo nome): a leitura não quebra, o estado é "Não aplicado", e o desfazer restaura o tipo e o valor originais. → Tarefas 2 e 4.
4. **Plano de energia inexistente** (notebooks com Modern Standby não têm "Alto desempenho"): o ajuste aparece como "Indisponível nesta máquina", não falha nem é aplicado. → Tarefas 2 e 4.
5. **Aplicar em lote quando um item falha** (sem admin, acesso negado): os outros continuam, o que falhou é listado com o motivo, e o diário só guarda o que foi escrito. → Tarefa 3.

---

## Estrutura de arquivos

```
catalog/tweaks.json                    # catálogo embutido
crates/core/src/tweaks/
  mod.rs                               # pub use; Catalog::embedded()
  model.rs                             # Tweak, Action, RegValue, Snapshot, enums
  catalog.rs                           # parse + validate
  backend.rs                           # trait SystemBackend + MemoryBackend
  windows.rs                           # WindowsBackend, restore point, is_laptop
  state.rs                             # status() e preview()
  journal.rs                           # Journal, load/save, reconcile
  engine.rs                            # apply(), undo(), undo_all()
crates/cli/src/tweak.rs                # subcomando `tweak`
app/src-tauri/src/commands/tweaks.rs
app/src/pages/{Jogos,Historico}.tsx (+ testes)
```

---

### Task 1: Modelo e catálogo

**Files:**
- Create: `catalog/tweaks.json`, `crates/core/src/tweaks/{mod,model,catalog}.rs`
- Modify: `crates/core/src/lib.rs` (`pub mod tweaks;`)

**Interfaces:**
- Produces:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "snake_case")]
pub enum Group { Jogos, Energia, SegundoPlano, Privacidade, Visual }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "snake_case")]
pub enum Evidence { Comprovado, Situacional, Controverso }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "snake_case")]
pub enum Effect { Immediate, Relogon, Reboot }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)] #[serde(rename_all = "UPPERCASE")]
pub enum Hive { Hkcu, Hklm }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)] #[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum RegValue { Dword(u32), Qword(u64), String(String) }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "snake_case")]
pub enum StartType { Automatic, Manual, Disabled }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)] #[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    Registry { hive: Hive, path: String, name: String, value: Option<RegValue> }, // None = apagar o valor
    ListEntry { hive: Hive, path: String, name: String, key: String, value: String }, // item "k=v;" numa REG_SZ
    Service { name: String, start: StartType },
    PowerPlan { scheme: String },                 // GUID minúsculo, sem chaves
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tweak {
    pub id: String, pub title: String, pub description: String,
    pub group: Group, pub evidence: Evidence, pub effect: Effect,
    pub needs_admin: bool, pub laptop_warning: bool, pub recommended: bool,
    pub source: String,                           // URL da documentação que justifica o valor
    pub actions: Vec<Action>,
}
pub struct Catalog { pub tweaks: Vec<Tweak> }
impl Catalog {
    pub fn embedded() -> &'static Catalog;        // parse + validate uma vez (OnceLock); panic com a mensagem do validate
    pub fn parse(json: &str) -> Result<Catalog, String>;
    pub fn get(&self, id: &str) -> Option<&Tweak>;
}
pub fn validate(c: &Catalog) -> Result<(), String>;
pub const ALLOWED_ROOTS: &[(Hive, &str)];
pub const FORBIDDEN_SERVICES: &[&str];          // lista exata das Global Constraints
pub const HIGH_PERFORMANCE_SCHEME: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
```

`ALLOWED_ROOTS` (prefixos, sem diferenciar maiúsculas): `(Hkcu, "Software\\Microsoft")`, `(Hkcu, "System\\GameConfigStore")`, `(Hkcu, "Control Panel\\Mouse")`, `(Hkcu, "Control Panel\\Desktop")`, `(Hklm, "SOFTWARE\\Policies\\Microsoft\\Windows")`, `(Hklm, "SYSTEM\\CurrentControlSet\\Control\\GraphicsDrivers")`.

`validate` falha (mensagem com o id do ajuste) quando: id repetido ou fora de `^[a-z0-9-]+$`; `actions` vazio; caminho fora de `ALLOWED_ROOTS`; serviço em `FORBIDDEN_SERVICES` (sem diferenciar maiúsculas); ação `Hklm`, `Service` ou `GraphicsDrivers` com `needs_admin == false`; `source` vazio; GUID de `PowerPlan` mal formado.

**Catálogo** (todos `evidence: comprovado`, `recommended: true`, exceto onde dito). Antes de escrever cada valor, confira-o na fonte e grave a URL em `source`. Fontes aceitas: documentação da Microsoft (learn.microsoft.com), ou o `config/tweaks.json` do [winutil](https://github.com/ChrisTitusTech/winutil). Se uma fonte contradisser um valor abaixo, siga a fonte e registre a divergência no relatório.

| id | grupo / efeito / admin | ações |
|---|---|---|
| `game-mode` | jogos / immediate / não | `HKCU\Software\Microsoft\GameBar` `AutoGameModeEnabled` = DWORD 1 |
| `game-dvr-off` | jogos / immediate / não | `HKCU\System\GameConfigStore` `GameDVR_Enabled` = DWORD 0; `HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR` `AppCaptureEnabled` = DWORD 0 |
| `hags` | jogos / reboot / sim | `HKLM\SYSTEM\CurrentControlSet\Control\GraphicsDrivers` `HwSchMode` = DWORD 2 |
| `windowed-optimizations` | jogos / immediate / não | ListEntry `HKCU\Software\Microsoft\DirectX\UserGpuPreferences` `DirectXUserGlobalSettings` `SwapEffectUpgradeEnable`=`1`; e `VRROptimizeEnable`=`1` |
| `mouse-accel-off` | jogos / relogon / não | `HKCU\Control Panel\Mouse`: `MouseSpeed` = String "0", `MouseThreshold1` = String "0", `MouseThreshold2` = String "0" |
| `power-high-performance` | energia / immediate / não / `laptop_warning: true` | PowerPlan `HIGH_PERFORMANCE_SCHEME` |
| `transparency-off` | visual / immediate / não | `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize` `EnableTransparency` = DWORD 0 |
| `animations-off` | visual / relogon / não | `HKCU\Control Panel\Desktop\WindowMetrics` `MinAnimate` = String "0"; `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced` `TaskbarAnimations` = DWORD 0 |
| `telemetry-reduced` | privacidade / reboot / sim | Service `DiagTrack` Manual; Service `dmwappushservice` Manual; `HKLM\SOFTWARE\Policies\Microsoft\Windows\DataCollection` `AllowTelemetry` = DWORD 1 |
| `activity-history-off` | privacidade / relogon / sim | `HKLM\SOFTWARE\Policies\Microsoft\Windows\System`: `EnableActivityFeed` = DWORD 0, `PublishUserActivities` = DWORD 0, `UploadUserActivities` = DWORD 0 |
| `delivery-optimization-off` | segundo_plano / immediate / sim | `HKLM\SOFTWARE\Policies\Microsoft\Windows\DeliveryOptimization` `DODownloadMode` = DWORD 0 |

Títulos e descrições em pt-BR, curtos, dizendo o que muda e o custo (por exemplo, no HAGS: "Exige reinício. Em algumas placas antigas pode piorar; desfaça se notar travamentos").

- [ ] **Step 1: Testes que falham** (em `catalog.rs`)

```rust
#[test] fn embedded_catalog_is_valid() { assert!(Catalog::embedded().tweaks.len() >= 11); }
#[test] fn duplicate_id_is_rejected() { /* dois "game-mode" → Err contendo "game-mode" */ }
#[test] fn path_outside_allowed_roots_is_rejected() { /* HKLM\SOFTWARE\Microsoft\Windows Defender → Err */ }
#[test] fn forbidden_service_is_rejected_case_insensitive() { /* "windefend" → Err */ }
#[test] fn hklm_action_requires_admin() { /* Hklm com needs_admin false → Err */ }
#[test] fn every_embedded_tweak_has_a_source() { /* source começa com "https://" */ }
#[test] fn registry_value_json_shape() {
    let a: Action = serde_json::from_str(r#"{"kind":"registry","hive":"HKCU","path":"Software\\Microsoft\\GameBar","name":"AutoGameModeEnabled","value":{"type":"dword","data":1}}"#).unwrap();
    assert!(matches!(a, Action::Registry { value: Some(RegValue::Dword(1)), .. }));
}
```

- [ ] **Step 2:** `cargo test -p sentinela-core tweaks::catalog` — FAIL (módulo não existe).
- [ ] **Step 3:** Implementar modelo, `parse`, `validate`, `embedded` e o catálogo.
- [ ] **Step 4:** O mesmo comando — PASS.
- [ ] **Step 5: Commit** — `feat(core): tweak model and validated embedded catalog`

### Task 2: Backend, estado e prévia

**Files:**
- Create: `crates/core/src/tweaks/{backend,state}.rs`

**Interfaces:**
- Consumes: Task 1.
- Produces:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)] #[serde(tag = "kind", rename_all = "snake_case")]
pub enum Snapshot {
    Registry { value: Option<RegValue> },       // None = não existia
    ListEntry { value: Option<String> },        // valor do item "k=v"; None = item ausente (ou a REG_SZ inteira)
    Service { start: StartType },
    PowerPlan { scheme: String },
    Unavailable { reason: String },             // ex.: plano de energia inexistente
}
pub trait SystemBackend: Send + Sync {
    fn read(&self, action: &Action) -> Result<Snapshot, String>;
    fn write(&self, action: &Action, target: &Snapshot) -> Result<(), String>; // escreve exatamente `target`
    fn elevated(&self) -> bool;
    fn hkcu_allowed(&self) -> bool;             // false quando elevado em outra conta (§5.1)
}
pub fn target_of(action: &Action) -> Snapshot; // o estado que a ação quer
pub struct MemoryBackend { /* Mutex<HashMap<String, Snapshot>> + flags */ }
impl MemoryBackend {
    pub fn new() -> Self;                         // elevated = true, hkcu_allowed = true
    pub fn set(&self, action: &Action, s: Snapshot);
    pub fn get(&self, action: &Action) -> Snapshot;
    pub fn set_elevated(&self, v: bool); pub fn set_hkcu_allowed(&self, v: bool);
    pub fn fail_writes_for(&self, action: &Action, msg: &str); // simula acesso negado
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] #[serde(rename_all = "snake_case")]
pub enum TweakStatus { Applied, NotApplied, Partial, Unavailable }
#[derive(Debug, Clone, Serialize)]
pub struct Change { pub description: String, pub current: String, pub target: String }
#[derive(Debug, Clone, Serialize)]
pub struct TweakView {
    pub id: String, pub status: TweakStatus, pub blocked: Option<String>, // motivo: "requer administrador" / "Bloqueado: a janela elevada está em outra conta do Windows."
    pub changes: Vec<Change>,                     // prévia: só as ações cujo atual ≠ alvo
}
pub fn view(t: &Tweak, b: &dyn SystemBackend) -> TweakView;
```

Regras: `Applied` = todas as ações no alvo; `NotApplied` = nenhuma; `Partial` = algumas; `Unavailable` = alguma leitura devolveu `Snapshot::Unavailable` (as outras não importam). Uma leitura com erro conta como "não no alvo" e o erro entra em `Change.current` como `"erro: {msg}"`. `blocked`: `needs_admin && !elevated` → `"requer administrador"`; alguma ação HKCU e `!hkcu_allowed` → a mensagem da outra conta. Textos de `Change` em pt-BR legível: `description` como `HKCU\…\GameBar\AutoGameModeEnabled`, `Serviço DiagTrack (inicialização)`, `Plano de energia ativo`; valores como `DWORD 1`, `"0"`, `não existe`, `Manual`, `Alto desempenho` (o GUID conhecido) ou o GUID.

- [ ] **Step 1: Testes que falham** (em `state.rs`, com `MemoryBackend`)

```rust
#[test] fn all_actions_at_target_is_applied() {}
#[test] fn some_actions_at_target_is_partial() {}
#[test] fn none_is_not_applied_and_preview_lists_every_change() { /* changes.len() == actions.len() */ }
#[test] fn preview_omits_actions_already_at_target() {}
#[test] fn missing_power_plan_is_unavailable() { /* Snapshot::Unavailable → Unavailable, mesmo com outras ações no alvo */ }
#[test] fn wrong_value_type_is_not_at_target() { /* alvo DWORD 1, atual String "1" → NotApplied, current == "\"1\"" */ }
#[test] fn admin_tweak_unelevated_is_blocked() {}
#[test] fn hkcu_tweak_in_other_account_is_blocked() {}
#[test] fn read_error_shows_in_preview() {}
```

- [ ] **Step 2:** `cargo test -p sentinela-core tweaks::state` — FAIL.
- [ ] **Step 3:** Implementar `backend.rs` e `state.rs`. A chave do `MemoryBackend` é uma string estável da ação (ex.: `reg:HKCU\path\name`, `list:HKCU\path\name\key`, `svc:Name`, `power`).
- [ ] **Step 4:** PASS.
- [ ] **Step 5: Commit** — `feat(core): tweak backend trait, status and preview`

### Task 3: Diário, aplicar e desfazer

**Files:**
- Create: `crates/core/src/tweaks/{journal,engine}.rs`
- Modify: `crates/core/src/activity.rs` (`ActivityKind::Tweak`, serializa `"tweak"`)

**Interfaces:**
- Consumes: Tasks 1–2.
- Produces:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalAction { pub action: Action, pub before: Snapshot, pub written: Snapshot, pub done: bool }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry { pub tweak_id: String, pub title: String, pub applied_at: String, pub actions: Vec<JournalAction> }
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Journal { pub entries: Vec<JournalEntry> }
impl Journal {
    pub fn load(path: &Path) -> Journal;          // ausente/corrompido → vazio; corrompido é renomeado para journal.json.corrompido-{timestamp}
    pub fn save(&self, path: &Path) -> anyhow::Result<()>; // temp + rename, como activity
    pub fn reconcile(&mut self, b: &dyn SystemBackend);    // ver Review Focus 1
}
pub fn default_journal_path() -> PathBuf;       // %LOCALAPPDATA%\Sentinela\tweaks\journal.json

#[derive(Debug, Serialize)] pub struct ItemResult { pub tweak_id: String, pub ok: bool, pub message: String }
#[derive(Debug, Serialize)] pub struct ApplyReport { pub results: Vec<ItemResult>, pub effect: Effect /* o mais forte entre os aplicados */, pub restore_point: Option<String> }
#[derive(Debug, Serialize)] pub struct UndoReport { pub results: Vec<ItemResult>, pub conflicts: Vec<String> }

pub fn apply(ids: &[String], cat: &Catalog, b: &dyn SystemBackend, journal_path: &Path) -> ApplyReport;
pub fn undo(ids: &[String], b: &dyn SystemBackend, journal_path: &Path) -> UndoReport;
pub fn undo_all(b: &dyn SystemBackend, journal_path: &Path) -> UndoReport;
```

`apply`, por ajuste, em ordem: pular se `blocked`, `Unavailable` ou já `Applied` (com mensagem); para cada ação que não está no alvo: ler `before`, acrescentar `JournalAction { done: false }` à entrada e **salvar o diário**, escrever o alvo, marcar `done: true` e salvar de novo. Uma escrita que falha encerra aquele ajuste com `ok: false` e a mensagem; as ações já escritas daquele ajuste ficam no diário (desfazíveis), a que falhou sai do diário. Se o mesmo ajuste já tem entrada (aplicado antes, depois revertido por fora), a entrada nova substitui a antiga mas **mantém o `before` mais antigo** de cada ação. Ao terminar, grava `activity` `Tweak` com `Otimizações aplicadas: {n}` quando n > 0. `restore_point` é preenchido pela camada de cima (Task 4), não aqui; o engine só o propaga.

`undo`, por entrada: para cada ação `done`, ler o atual; se igual a `written`, escrever `before` (apagar quando era "não existia"); se diferente, não escrever e acrescentar a `conflicts`: `"{título}: {descrição da ação} foi alterado depois da otimização; valor atual mantido."`. A entrada sai do diário quando todas as ações foram restauradas ou estavam em conflito. Atividade `Tweak`: `Otimizações desfeitas: {n}`.

`reconcile`: para ações `done: false`, ler o atual; igual ao alvo → `done: true`; igual a `before` → remover a ação; outro valor → `done: true` (o desfazer depois detecta o conflito). Entradas sem ações são removidas.

- [ ] **Step 1: Testes que falham** (em `engine.rs` e `journal.rs`, com `MemoryBackend` e diário em tempdir)

```rust
#[test] fn journal_is_saved_before_the_write() {
    /* backend que, dentro de write, lê o arquivo do diário do disco: a ação já está lá com done == false */
}
#[test] fn undo_restores_original_including_absent_value() { /* before = Registry{None} → depois do undo, get() == Registry{None} */ }
#[test] fn undo_keeps_value_changed_externally_and_reports_conflict() {}
#[test] fn failing_item_does_not_stop_the_batch() { /* fail_writes_for numa ação de B: A e C ok, B ok == false, diário sem a ação que falhou */ }
#[test] fn reapply_keeps_the_oldest_before() {}
#[test] fn reconcile_pending_action_at_target_becomes_done() {}
#[test] fn reconcile_pending_action_at_before_is_dropped() {}
#[test] fn corrupt_journal_is_set_aside_not_lost() { /* arquivo inválido → load vazio e arquivo .corrompido-* existe */ }
#[test] fn whole_catalog_applies_and_undoes_without_residue() {
    /* MemoryBackend com estados iniciais variados (alguns ausentes, um String "1" onde o alvo é DWORD), PowerPlan disponível;
       apply(todos) → todos Applied; undo_all → cada ação volta exatamente ao estado inicial; diário vazio */
}
#[test] fn blocked_and_unavailable_are_skipped_with_message() {}
```

- [ ] **Step 2:** `cargo test -p sentinela-core tweaks` — FAIL.
- [ ] **Step 3:** Implementar `journal.rs`, `engine.rs` e o `ActivityKind::Tweak`.
- [ ] **Step 4:** PASS, mais `cargo test --workspace` (a nova variante de `ActivityKind` não pode quebrar a leitura de `activity.json` existente).
- [ ] **Step 5: Commit** — `feat(core): tweak journal, apply and undo`

### Task 4: Backend real do Windows

**Files:**
- Create: `crates/core/src/tweaks/windows.rs`
- Modify: `Cargo.toml` (features do `windows-sys`: `Win32_System_Registry`, `Win32_System_Power`, `Win32_System_Restore`, `Win32_Security`, `Win32_System_Threading`)

**Interfaces:**
- Consumes: `SystemBackend`, `Snapshot`, `Action`.
- Produces:

```rust
pub struct WindowsBackend { /* elevated, hkcu_allowed */ }
impl WindowsBackend { pub fn new(hkcu_allowed: bool) -> Self; } // elevated via clean::platform::is_elevated()
pub fn create_restore_point(description: &str) -> Result<(), String>; // SRSetRestorePointW, BEGIN_SYSTEM_CHANGE + END
pub fn is_laptop() -> bool;                     // GetSystemPowerStatus: tem bateria (BatteryFlag != 128 e != 255)
pub fn current_user_sid() -> Option<String>;    // SID do token do processo, formato "S-1-5-21-…"
```

Detalhes:
- **Registro:** `RegOpenKeyExW`/`RegQueryValueExW` com `KEY_WOW64_64KEY`; para ler e escrever, `RegCreateKeyExW` só na escrita; `RegSetValueExW`; `RegDeleteValueW`. Tipos `REG_DWORD`, `REG_QWORD`, `REG_SZ`. Ao ler um tipo que o modelo não conhece (binário, multi-sz), devolver `Err("tipo de valor não suportado")` — nunca escrever por cima disso. Chave inexistente na leitura = `Registry { value: None }`.
- **ListEntry:** lê a REG_SZ, separa por `;`, procura `key=`; escrever o alvo substitui ou acrescenta só aquele item e preserva os outros na ordem; escrever `ListEntry { value: None }` remove o item e, se a string ficar vazia e o valor não existia antes, apaga o valor (o `before` diz isso).
- **Serviço:** `OpenSCManagerW` + `OpenServiceW(SERVICE_QUERY_CONFIG | SERVICE_CHANGE_CONFIG)`; ler com `QueryServiceConfigW`; escrever só o tipo de inicialização com `ChangeServiceConfigW` (`SERVICE_NO_CHANGE` no resto). Nunca para nem inicia o serviço. Serviço inexistente → `Snapshot::Unavailable { reason: "serviço não existe" }`.
- **Plano de energia:** `PowerGetActiveScheme` (liberar com `LocalFree`) e `PowerSetActiveScheme`; disponibilidade com `PowerReadFriendlyName` no GUID alvo — falha → `Unavailable { reason: "plano de energia não existe nesta máquina" }`.
- **Ponto de restauração:** `SRSetRestorePointW` exige administrador; Proteção do Sistema desligada → `Err("Proteção do Sistema desligada")`.

- [ ] **Step 1: Testes que falham**
  - Puros: `list_entry_set_replaces_only_that_item` (`"A=1;B=2;"` + B=3 → `"A=1;B=3;"`), `list_entry_set_appends` (`""` + A=1 → `"A=1;"`), `list_entry_remove_keeps_others`, `sid_format_starts_with_s_1` (sobre a conversão).
  - `#[ignore]` (rodar à mão com `--ignored`, só em `HKCU\Software\Sentinela-test-{pid}`, apagando a chave no fim): `registry_roundtrip_dword_string_and_delete`, `wrong_type_reads_as_error_for_binary`, `list_entry_roundtrip_preserves_other_items`, `power_plan_reads_active_scheme` (só leitura).
- [ ] **Step 2:** `cargo test -p sentinela-core tweaks::windows` — FAIL.
- [ ] **Step 3:** Implementar. Cada chamada unsafe num helper pequeno; `RegCloseKey`/`CloseServiceHandle` em todos os caminhos (guard com `Drop`).
- [ ] **Step 4:** PASS; depois `cargo test -p sentinela-core tweaks::windows -- --ignored` uma vez, com a saída no relatório.
- [ ] **Step 5: Commit** — `feat(core): Windows backend for tweaks`

### Task 5: `sentinela tweak`

**Files:**
- Create: `crates/cli/src/tweak.rs`
- Modify: `crates/cli/src/cli.rs` (subcomando), `crates/cli/tests/cli.rs` (só testes novos), `README.md` (seção do comando)

**Interfaces:**
- Consumes: Tasks 1–4.
- Produces: subcomando com o mesmo padrão de `clean`:

```
sentinela tweak list [--json]
sentinela tweak apply <ID>... [--yes] [--json]
sentinela tweak undo (<ID>... | --all) [--yes] [--json]
```

- `list`: uma linha por ajuste: `id`, estado (`aplicado` / `não aplicado` / `parcial` / `indisponível`), título, e `(requer administrador)` quando bloqueado.
- `apply` e `undo` mostram a prévia (cada `Change` como `  {descrição}: {atual} → {alvo}`), pedem `[s/N]` no stderr, como o `clean`. `--yes` pula a pergunta. `apply` cria o ponto de restauração antes, só se elevado e só se algum ajuste vai escrever; falha vira aviso `aviso: ponto de restauração não criado: {motivo}` e segue. Avisa no fim conforme o `effect`: `Reinicie o computador para concluir.` / `Saia e entre de novo no Windows para concluir.`.
- Id desconhecido → erro global (código 2): `otimização desconhecida: {id}. Use \`sentinela tweak list\`.`
- Códigos: 0 sucesso (inclusive "nada a fazer"), 1 se algum item falhou ou houve conflito no undo, 2 erro global.
- O CLI usa `WindowsBackend::new(true)` (o terminal é sempre a conta de quem o abriu).

- [ ] **Step 1: Testes que falham** em `crates/cli/tests/cli.rs`, só os que não tocam no sistema: `tweak_list_shows_catalog_ids` (contém `game-mode` e `hags`), `tweak_apply_unknown_id_exits_2`, `tweak_undo_requires_ids_or_all` (sem argumentos → código 2). O comportamento de aplicar é coberto no core.
- [ ] **Step 2:** `cargo test -p sentinela --test cli tweak` — FAIL.
- [ ] **Step 3:** Implementar, mantendo a formatação e as mensagens dos outros comandos.
- [ ] **Step 4:** PASS e `cargo test --workspace`.
- [ ] **Step 5: Commit** — `feat(cli): tweak list/apply/undo`

### Task 6: Comandos da janela

**Files:**
- Create: `app/src-tauri/src/commands/tweaks.rs`
- Modify: `app/src-tauri/src/lib.rs`, `app/src-tauri/src/jobs.rs` (`JobKind::Tweaks`), `app/src-tauri/src/commands/system.rs` (relançamento passa `--for-sid=<SID>`), `app/src/jobs.ts` (tipo `JobKind`), `app/src/api.ts`

**Interfaces:**
- Consumes: Tasks 1–4; `spawn_job`, `JobRegistry`, `activity`.
- Produces (comandos):
  - `tweaks_list() -> Result<TweaksPage, String>` com `TweaksPage { tweaks: Vec<TweakItem>, laptop: bool, elevated: bool, hkcu_allowed: bool, journal_count: usize }` e `TweakItem { tweak: Tweak, view: TweakView, in_journal: bool }`. Roda `Journal::reconcile` antes (via `spawn_blocking`).
  - `tweaks_apply(ids: Vec<String>) -> Result<JobId, String>` (`JobKind::Tweaks`, resultado `ApplyReport`): ids vazios → `Err("Nenhuma otimização selecionada.")`; desconhecidos → `Err("otimização desconhecida: {id}")`; cria o ponto de restauração **uma vez por sessão do app** (flag em estado gerenciado), só se elevado e algum ajuste vai escrever; o resultado vai para `ApplyReport.restore_point` como `"criado"` ou `"não criado: {motivo}"`.
  - `tweaks_undo(ids: Vec<String>) -> Result<JobId, String>` e `tweaks_undo_all() -> Result<JobId, String>` (`JobKind::Tweaks`, resultado `UndoReport`).
  - `tweaks_journal() -> Result<Vec<JournalEntry>, String>`.
  - `system_status` ganha `tweaks_applied: usize` e `tweaks_total: usize` (para o cartão "Otimização" do Início).
- `hkcu_allowed`: `relaunch_elevated` acrescenta `--for-sid=<SID do processo atual>` aos argumentos; na inicialização, uma função pura `fn hkcu_allowed(arg_sid: Option<&str>, own_sid: Option<&str>) -> bool` decide: sem argumento → `true`; com argumento → `true` só se igual ao SID próprio.

- [ ] **Step 1: Testes que falham** (Rust, com `MemoryBackend` por trás de helpers puros como nos comandos da fase 1): `apply_rejects_empty_selection`, `apply_rejects_unknown_id`, `restore_point_only_once_per_session` (contador num fake), `hkcu_allowed_rules` (os três casos), `parse_for_sid_arg`.
- [ ] **Step 2:** `cargo test -p sentinela-gui tweaks` — FAIL.
- [ ] **Step 3:** Implementar; comandos `async` com o trabalho de disco e registro em `spawn_blocking`.
- [ ] **Step 4:** PASS, `npm run build`, `cargo clippy --workspace`.
- [ ] **Step 5: Commit** — `feat(gui): tweak commands, restore point per session, other-account guard`

### Task 7: Página Jogos

**Files:**
- Create: `app/src/pages/Jogos.tsx`, `app/src/pages/Jogos.test.tsx`
- Modify: `app/src/components/Sidebar.tsx` (`Page` ganha `"jogos"` e `"historico"`; ordem: Início, Jogos, Limpeza, Segurança, Disco, Histórico, Ajustes), `app/src/App.tsx`

**Interfaces:**
- Consumes: `tweaksList`, `tweaksApply`, `useJob("tweaks")`, `ConfirmDialog`.

Comportamento e textos:
- Topo: perfis **"Recomendado"** (marca os `recommended` não bloqueados nem indisponíveis; em notebook **não** marca `power-high-performance`) e **"Personalizado"** (o usuário marca à mão; vira o perfil ativo ao mexer em qualquer caixa).
- Lista por grupo (`Jogos`, `Energia`, `Segundo plano`, `Privacidade`, `Visual`). Cada linha: caixa de seleção, título, selo de evidência (`Comprovado`), estado (`Aplicado` / `Não aplicado` / `Parcial` / `Indisponível nesta máquina`), ícones/rótulos `Requer administrador`, `Exige reinício`, `Vale após sair e entrar`, e o motivo do bloqueio. Linhas bloqueadas ou indisponíveis têm a caixa desabilitada. Já aplicadas aparecem desmarcadas.
- Aviso de notebook no `power-high-performance` quando `laptop`: `Em notebook, gasta mais bateria e esquenta mais.`
- Painel de prévia ao lado: para os marcados, as `changes` de cada um (`{descrição}: {atual} → {alvo}`).
- **"Aplicar"** abre o `ConfirmDialog` com `Aplicar {n} otimização(ões)?` e, se algum exige admin e o app está elevado, `Um ponto de restauração do Windows será criado antes.` Depois do job: lista de resultados (falhas com motivo), o aviso de reinício/relogon conforme `effect`, o estado do ponto de restauração, e recarrega a lista.
- Sem elevação, um aviso no topo quando algum marcado/recomendado exige admin, com o botão "Reiniciar como administrador" (o mesmo do rodapé).
- Texto fixo no rodapé da página: `O Sentinela não desativa Defender, Windows Update, firewall nem outras proteções para ganhar FPS.`

- [ ] **Step 1: Testes de componente que falham**
  - `recommended_profile_checks_only_available_recommended`
  - `laptop_does_not_preselect_power_plan_and_shows_warning`
  - `blocked_row_is_disabled_with_reason`
  - `applies_only_after_confirmation` (`tweaksApply` não chamado antes de "Aplicar" no diálogo; chamado com os ids marcados)
  - `shows_reboot_notice_after_apply_with_reboot_effect`
  - `item_failure_is_listed_with_reason`
- [ ] **Step 2:** `npm test` — FAIL.
- [ ] **Step 3:** Implementar.
- [ ] **Step 4:** PASS e `npm run build`.
- [ ] **Step 5: Commit** — `feat(gui): games page with profiles, preview and apply`

### Task 8: Histórico e cartão do Início

**Files:**
- Create: `app/src/pages/Historico.tsx`, `app/src/pages/Historico.test.tsx`
- Modify: `app/src/pages/Inicio.tsx` (+ teste)
- Create: `docs/testes-manuais-fase-2.md`

**Interfaces:**
- Consumes: `tweaksJournal`, `tweaksUndo`, `tweaksUndoAll`, `useJob("tweaks")`, `ConfirmDialog`, `system_status.tweaks_applied/tweaks_total`.

Comportamento e textos:
- Histórico: uma linha por entrada do diário (título, data `humanDate(applied_at)`, quantas mudanças), botão **"Desfazer"** por linha e **"Desfazer tudo"** no topo; os dois pedem confirmação (`Desfazer "{título}"?` / `Desfazer todas as {n} otimizações?`). Depois do job: resultados, e cada conflito numa lista com o título `Mantido porque mudou depois da otimização:`. Vazio: `Nenhuma otimização aplicada pelo Sentinela.`
- Início: o terceiro cartão vira **"Otimização"** com `{aplicadas} de {total}` e o botão `Ir para Jogos`; a versão do app passa para o rodapé da página.
- `docs/testes-manuais-fase-2.md`: roteiro em VM — aplicar o perfil Recomendado sem admin (só HKCU), elevar e aplicar o resto (ponto de restauração criado), reiniciar e conferir estados, alterar um valor no `regedit` e desfazer (conflito mostrado), desfazer tudo e conferir no `regedit` que os valores que não existiam sumiram, `sentinela tweak list/apply/undo` no terminal.

- [ ] **Step 1: Testes que falham**: `undo_asks_confirmation`, `undo_all_asks_confirmation`, `conflicts_are_listed_after_undo`, `empty_journal_message`, e no Início `optimization_card_shows_counts`.
- [ ] **Step 2:** `npm test` — FAIL.
- [ ] **Step 3:** Implementar e escrever o roteiro.
- [ ] **Step 4:** PASS, `npm run build`, `cargo test --workspace`.
- [ ] **Step 5: Commit** — `feat(gui): history page with undo and optimization card`
