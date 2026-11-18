# Sentinela — Fase 4a: personalização nativa, limpeza nova e inicialização — Plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Uma página Personalizar com as personalizações por registro do §9.1 (interruptores e listas de opções, com prévia, histórico e desfazer), as cinco categorias novas de limpeza do §7, e um gerenciador de inicialização que liga e desliga programas sem nunca apagá-los.

**Architecture:** As personalizações são ajustes do motor da fase 2 (`crates/core/src/tweaks/`) no grupo `personalizacao`, com um campo novo `choice` que agrupa opções mutuamente exclusivas. As categorias de limpeza seguem o `trait Category` existente. O gerenciador de inicialização é um módulo novo `crates/core/src/startup/` que lê as entradas do Windows e grava só o estado "aprovado/desativado" que o próprio Gerenciador de Tarefas usa.

**Tech Stack:** Rust 2024, windows-sys 0.61, Tauri 2, React + TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-11-16-sentinela-gui-otimizacao-design.md` — §6.1–6.2 (motor), §6.3 (linha "Apps em segundo plano e inicialização"), §7 (categorias novas), §9.1, §10, §11 fase 4.

**Fora deste plano (fase 4b):** aba "Avançado" e perfil "FPS competitivo", Atualizações do Windows (§6.5), GPU de alto desempenho por jogo. Cor de destaque fica para os Temas (fase 5), porque exige valor escolhido pelo usuário.

## Global Constraints

- Só Windows; CI `windows-latest`; builds locais com `CARGO_TARGET_DIR=C:/st`.
- Toda tarefa termina com `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `npm test` e `npm run build` verdes.
- Nada muda sem prévia e confirmação; tudo do motor de ajustes é desfazível pelo Histórico, voltando ao valor original da máquina.
- Limpeza: as quatro garantias do `clean` valem para cada categoria nova — sem confirmação nada é apagado; nada fora das pastas da categoria, inclusive por links; arquivos com menos de 24 h preservados onde a categoria usa idade; dados de login, saves e jogos nunca tocados.
- Inicialização: nunca apagar entradas nem arquivos; só alternar o estado aprovado/desativado.
- O Sentinela nunca reinicia o Explorer sozinho: oferece o botão "Reiniciar Explorer".
- Textos em pt-BR; `app/src-tauri` sem regra de negócio; nenhum teste altera o registro real fora de `HKCU\Software\Sentinela-test-*`.

## Review Focus

1. **Trocar de opção numa lista** (barra à esquerda → centro → esquerda): o desfazer final devolve o valor original da máquina, não o da opção anterior. → Tarefa 1.
2. **Valor protegido pelo Windows** (o Windows 11 recente bloqueia a escrita de `TaskbarDa` dos Widgets): a falha aparece com motivo legível e nada fica pela metade no histórico. → Tarefas 1 e 2.
3. **Launcher aberto durante a limpeza** (Steam, Discord, Battle.net): a categoria é pulada com aviso, como o cache de navegador. → Tarefa 3.
4. **Entrada de inicialização apontando para um arquivo que não existe mais:** aparece marcada como "arquivo não encontrado", pode ser desativada, e nunca é apagada. → Tarefa 5.
5. **Entrada de inicialização de outro usuário ou do sistema sem administrador:** aparece com cadeado e o motivo; alternar não é oferecido. → Tarefa 5.

---

### Task 1: Motor — opções exclusivas, valor padrão e efeito "reiniciar Explorer"

**Files:** `crates/core/src/tweaks/{model,catalog,engine,state}.rs`, `catalog/tweaks.json` (só esquema; itens na Tarefa 2), testes no próprio módulo.

**Interfaces (produz):**
- `Tweak` ganha `#[serde(default)] pub choice: Option<String>` (id do grupo de opções; ajustes com o mesmo `choice` são mutuamente exclusivos) e `#[serde(default)] pub choice_label: Option<String>`.
- `Effect` ganha `Explorer` (ordem: `Immediate < Explorer < Relogon < Reboot`).
- `Group` ganha `Personalizacao` (serde `"personalizacao"`) e um campo `#[serde(default)] pub area: Option<String>` em `Tweak` com um de: `barra`, `iniciar`, `explorador`, `aparencia`, `sistema`.
- `Action::Registry` aceita `name: ""` = valor padrão da chave (o "(Padrão)" do regedit). O backend do Windows já passa o nome como string; confirme que `""` vira um ponteiro para `L""` e funciona em leitura, escrita e exclusão (teste `#[ignore]` na chave de teste).
- `ALLOWED_ROOTS` ganha: `(Hkcu, "Software\\Classes\\CLSID")`, `(Hkcu, "Software\\Policies\\Microsoft\\Windows")`, `(Hkcu, "Control Panel\\Accessibility")`, `(Hklm, "SOFTWARE\\Policies\\Microsoft\\Dsh")`.
- `pub fn apply_choice(id: &str, cat: &Catalog, b: &dyn SystemBackend, journal_path: &Path) -> ApplyReport`: se outra opção do mesmo `choice` está no diário, desfaz essa primeiro (mesma trava do diário) e então aplica a escolhida; se o desfazer der conflito, não aplica e explica.
- `validate`: todos os ajustes de um mesmo `choice` têm o mesmo `area` e as mesmas chaves (mesmo conjunto de `(hive, path, name)`), senão erro; `choice_label` obrigatório quando há `choice`; `name: ""` permitido só em `Registry`.

- [ ] **Step 1: Testes que falham:** `switching_choice_and_undo_restores_original` (A→B→undo B→undo A volta ao original; A→B via `apply_choice` e depois `undo_all` volta ao original), `choice_conflict_does_not_apply`, `choice_members_must_share_keys`, `default_value_name_allowed_only_for_registry`, `explorer_effect_orders_between_immediate_and_relogon`, `classes_clsid_root_allowed`.
- [ ] **Step 2:** `cargo test -p sentinela-core tweaks` — FAIL.
- [ ] **Step 3:** Implementar.
- [ ] **Step 4:** PASS; `#[ignore]` de valor padrão rodado à mão uma vez.
- [ ] **Step 5: Commit** — `feat(core): exclusive tweak choices, default registry value, explorer effect`

### Task 2: Catálogo de personalização

**Files:** `catalog/tweaks.json`, teste em `catalog.rs`.

Todos com `group: personalizacao`, `evidence: comprovado`, `recommended: false`, `source` conferida na documentação da Microsoft ou no `config/tweaks.json` do winutil (mesma regra da fase 2; divergências no relatório). `effect: explorer` onde a mudança só aparece após reiniciar o Explorer.

| área | id(s) | ação |
|---|---|---|
| barra | `taskbar-align-left` / `taskbar-align-center` (choice `taskbar-align`, labels "Esquerda"/"Centro") | `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced` `TaskbarAl` = 0 / 1 |
| barra | `taskbar-search-hidden` / `-icon` / `-box` (choice `taskbar-search`: "Oculta"/"Ícone"/"Caixa") | `HKCU\Software\Microsoft\Windows\CurrentVersion\Search` `SearchboxTaskbarMode` = 0 / 1 / 2 |
| barra | `taskbar-taskview-off` | `…\Explorer\Advanced` `ShowTaskViewButton` = 0 |
| barra | `taskbar-widgets-off` (admin) | `HKLM\SOFTWARE\Policies\Microsoft\Dsh` `AllowNewsAndInterests` = 0 |
| barra | `taskbar-end-task` | `…\Explorer\Advanced\TaskbarDeveloperSettings` `TaskbarEndTask` = 1 |
| barra | `taskbar-clock-seconds` | `…\Explorer\Advanced` `ShowSecondsInSystemClock` = 1 |
| barra | `taskbar-combine-always` / `-when-full` / `-never` (choice `taskbar-combine`) | `…\Explorer\Advanced` `TaskbarGlomLevel` = 0 / 1 / 2 |
| iniciar | `start-recommendations-off` | `…\Explorer\Advanced` `Start_IrisRecommendations` = 0 |
| iniciar | `start-bing-off` | `HKCU\Software\Policies\Microsoft\Windows\Explorer` `DisableSearchBoxSuggestions` = 1 |
| iniciar | `start-layout-default` / `-more-pins` / `-more-recommendations` (choice `start-layout`) | `…\Explorer\Advanced` `Start_Layout` = 0 / 1 / 2 |
| explorador | `explorer-classic-menu` | `HKCU\Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\InprocServer32` `""` = String "" |
| explorador | `explorer-show-extensions` | `…\Explorer\Advanced` `HideFileExt` = 0 |
| explorador | `explorer-show-hidden` | `…\Explorer\Advanced` `Hidden` = 1 |
| explorador | `explorer-open-this-pc` | `…\Explorer\Advanced` `LaunchTo` = 1 |
| explorador | `explorer-hide-home` | `HKCU\Software\Classes\CLSID\{f874310e-b6b7-47dc-bc84-b9e6b38f5903}` `System.IsPinnedToNameSpaceTree` = 0 |
| explorador | `explorer-hide-gallery` | `HKCU\Software\Classes\CLSID\{e88865ea-0e1c-4e20-9aa6-edcd0212c87c}` `System.IsPinnedToNameSpaceTree` = 0 |
| explorador | `explorer-compact` | `…\Explorer\Advanced` `UseCompactMode` = 1 |
| aparencia | `theme-apps-dark` / `theme-apps-light` (choice `theme-apps`) | `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize` `AppsUseLightTheme` = 0 / 1 |
| aparencia | `theme-system-dark` / `theme-system-light` (choice `theme-system`) | mesmo caminho, `SystemUsesLightTheme` = 0 / 1 |
| sistema | `lockscreen-off` (admin) | `HKLM\SOFTWARE\Policies\Microsoft\Windows\Personalization` `NoLockScreen` = 1 |
| sistema | `logon-blur-off` (admin) | `HKLM\SOFTWARE\Policies\Microsoft\Windows\System` `DisableAcrylicBackgroundOnLogon` = 1 |
| sistema | `sticky-keys-shortcut-off` | `HKCU\Control Panel\Accessibility\StickyKeys` `Flags` = String "506" |
| sistema | `snap-off` | `HKCU\Control Panel\Desktop` `WindowArrangementActive` = String "0" |

Transparência e animações já existem no grupo Visual; a página Personalizar mostra os dois também (filtra por id). NumLock na inicialização fica fora (exige `HKU\.DEFAULT`, outra raiz).

- [ ] **Step 1: Teste que falha:** `personalization_catalog_is_valid` (todos os ids acima presentes, cada `choice` com ≥ 2 opções e mesmas chaves, nenhum `recommended`).
- [ ] **Step 2–4:** FAIL → implementar → PASS (e `cargo test --workspace`).
- [ ] **Step 5: Commit** — `feat(core): native personalization catalog`

### Task 3: Categorias novas de limpeza

**Files:** `crates/core/src/clean/{gaming,system_extra}.rs` (novas), `crates/core/src/clean/mod.rs` (`all_categories` na ordem: as seis atuais, depois `shader-cache`, `launcher-cache`, `thumbnail-cache`, `delivery-optimization`, `windows-logs`), testes.

| id | admin | raízes (resolvidas pelo `Env`) | regra |
|---|---|---|---|
| `shader-cache` | não | `%LOCALAPPDATA%\D3DSCache`, `%LOCALAPPDATA%\NVIDIA\DXCache`, `%LOCALAPPDATA%\NVIDIA\GLCache`, `%LOCALAPPDATA%\AMD\DxCache`, `%LOCALAPPDATA%\AMD\DxcCache`, `%USERPROFILE%\AppData\LocalLow\Intel\ShaderCache` | arquivos com mais de 24 h |
| `launcher-cache` | não | `%LOCALAPPDATA%\Steam\htmlcache`; `%LOCALAPPDATA%\EpicGamesLauncher\Saved\webcache*` (só pastas cujo nome começa com `webcache`); `%LOCALAPPDATA%\Battle.net\BrowserCaches`; `%APPDATA%\discord\{Cache,Code Cache,GPUCache}` | pula o launcher cujo processo está aberto (`steam.exe`/`steamwebhelper.exe`, `EpicGamesLauncher.exe`, `Battle.net.exe`, `Discord.exe`), com aviso por launcher, como o cache de navegador (reusar a detecção de processo dele) |
| `thumbnail-cache` | não | `%LOCALAPPDATA%\Microsoft\Windows\Explorer` — só arquivos `thumbcache_*.db` e `iconcache_*.db` | sem idade; arquivos em uso são pulados e contados |
| `delivery-optimization` | sim | `%SystemRoot%\ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache` | sem idade |
| `windows-logs` | sim | `%SystemRoot%\Logs\CBS` (`*.log`, `*.cab`) e `%SystemRoot%\Temp\*.log` | mais de 24 h; nunca o `CBS.log` ativo (o mais recente) |

Nomes pt-BR na página Limpeza: "Cache de shaders", "Cache de launchers", "Cache de miniaturas", "Cache da Otimização de Entrega", "Logs do Windows".

- [ ] **Step 1: Testes que falham** (raiz falsa em tempdir, como os existentes), para cada categoria: os quatro obrigatórios (sem apply nada some; nada fora das raízes inclusive por link/junction; recentes preservados onde há idade; arquivos de fora do padrão — p.ex. `Steam\userdata`, `discord\Local Storage`, `Explorer\outra.db` — preservados) e `launcher_open_is_skipped_with_warning` (detector de processo falso), `active_cbs_log_preserved`.
- [ ] **Step 2–4:** FAIL → implementar → PASS.
- [ ] **Step 5:** Página Limpeza: nomes novos e o teste `new_categories_render_with_names`.
- [ ] **Step 6: Commit** — `feat(core,gui): shader, launcher, thumbnail, delivery-optimization and log cleaning`

### Task 4: Comandos e página Personalizar

**Files:** `app/src-tauri/src/commands/tweaks.rs` (`tweaks_apply_choice(id)` job `Tweaks`; `restart_explorer()`), `app/src/pages/Personalizar.tsx` (+ teste), `Sidebar.tsx` (`"personalizar"` depois de Aplicativos), `App.tsx`, `api.ts`.

- `restart_explorer`: encerra e reabre o `explorer.exe` do usuário (`taskkill /F /IM explorer.exe` por caminho completo de `System32`, depois `explorer.exe` de `%SystemRoot%`), só por clique do usuário; devolve erro legível se falhar.
- Página: abas por área (`Barra de tarefas`, `Menu Iniciar`, `Explorador`, `Aparência`, `Sistema`); cada ajuste simples é um interruptor (ligado = Aplicado); cada `choice` é um grupo de botões de opção com a opção aplicada marcada (nenhuma marcada = valor do Windows). Mudar um interruptor ou opção abre a confirmação com a prévia (`{descrição}: {atual} → {alvo}`) e, ao confirmar, aplica (`tweaksApply` para interruptor ligado; `tweaksUndo` para desligar algo aplicado pelo Sentinela; `tweaksApplyChoice` para opções). Desligar algo que o Sentinela não aplicou: o interruptor fica desabilitado com o texto `Configurado fora do Sentinela.`
- Depois de um job com efeito `explorer`: aviso `Reinicie o Explorer para ver a mudança.` + botão **"Reiniciar Explorer"** (confirmação: `Reiniciar o Explorer? As janelas de pastas abertas serão fechadas.`).
- Bloqueios iguais à página Jogos (admin, outra conta), mesmo banner de administrador.

- [ ] **Step 1: Testes que falham** (Rust: `apply_choice_rejects_non_choice_id`; Vitest: `switch_asks_confirmation_with_preview`, `turning_off_undoes`, `choice_selects_applied_option`, `external_value_disables_toggle`, `explorer_notice_and_restart_confirmation`, `admin_items_locked_when_not_elevated`, sidebar order).
- [ ] **Step 2–4:** FAIL → implementar → PASS.
- [ ] **Step 5: Commit** — `feat(gui): personalize page with toggles, choices and explorer restart`

### Task 5: Gerenciador de inicialização

**Files:** `crates/core/src/startup/{mod,windows}.rs`, `app/src-tauri/src/commands/startup.rs`, `app/src/components/StartupList.tsx` (+ teste), mostrado como seção "Programas que iniciam com o Windows" no fim da página Jogos.

**Interfaces:**
```rust
pub enum StartupSource { RunHkcu, RunHklm, RunHklm32, StartupFolderUser, StartupFolderCommon }
pub struct StartupEntry { pub id: String, pub name: String, pub command: String, pub source: StartupSource, pub enabled: bool, pub file_exists: bool, pub needs_admin: bool }
pub trait StartupBackend { fn list(&self) -> Result<Vec<StartupEntry>, String>; fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String>; }
```
- Leitura: valores de `…\CurrentVersion\Run` (HKCU, HKLM, HKLM `WOW6432Node`) e atalhos das pastas Inicializar (usuário e comum). Estado: `…\Explorer\StartupApproved\{Run,Run32,StartupFolder}` (REG_BINARY de 12 bytes; primeiro byte par = ligado, ímpar = desligado; ausente = ligado). `set_enabled` grava só esse valor (`02 00 00 00` + 8 zeros para ligar; `03 00 00 00` + FILETIME atual para desligar) — exatamente como o Gerenciador de Tarefas; nunca apaga a entrada nem o atalho.
- HKLM e pasta comum: `needs_admin = true`.
- `file_exists`: resolve o executável do comando (aspas, argumentos, variáveis de ambiente).
- A entrada `Sentinela` aparece também, com o texto `Este é o Sentinela. Para desligar, use Ajustes → Inicialização.` e sem interruptor.
- UI: lista com nome, comando (encurtado, com dica do completo), origem, `Arquivo não encontrado` quando for o caso, interruptor (confirmação ao desligar: `Desativar "{nome}" na inicialização? O programa continua instalado.`). Atividade `Inicialização: "{nome}" desativado|ativado`.

- [ ] **Step 1: Testes que falham** (backend falso + parsers puros): `approved_binary_parsing` (02…, 03…, 06…, 07…, ausente), `set_enabled_writes_task_manager_format`, `command_executable_resolution` (aspas, `%ProgramFiles%`, argumentos), `missing_file_flagged_not_removed`, `hklm_entries_need_admin`, `sentinela_entry_has_no_toggle`; Vitest: `disable_asks_confirmation`, `locked_without_admin`, `missing_file_label`. `#[ignore]`: leitura real das listas (só leitura).
- [ ] **Step 2–4:** FAIL → implementar → PASS.
- [ ] **Step 5: Commit** — `feat: startup manager that toggles entries like Task Manager`

### Task 6: CLI e roteiro manual

**Files:** `crates/cli/src/{tweak,clean}.rs` (ajustes de personalização já aparecem em `tweak list`; `tweak apply` de um id com `choice` usa `apply_choice`), `crates/cli/src/startup.rs` (`sentinela startup list [--json]`, `startup enable|disable <ID> [--yes]`), README, `docs/testes-manuais-fase-4a.md`.

- [ ] **Step 1: Testes que falham** (sem alterar o sistema): `startup_list_runs` (código 0), `startup_disable_unknown_id_exits_2`, `tweak_list_includes_personalization`, `clean_lists_new_categories` (simulação, sem `--apply`).
- [ ] **Step 2–4:** FAIL → implementar → PASS.
- [ ] **Step 5:** Roteiro manual: cada área da Personalizar com reinício do Explorer, trocar opções e desfazer pelo Histórico, Widgets no build atual, limpeza com Steam aberta e fechada, inicialização (desligar, reiniciar, conferir no Gerenciador de Tarefas que aparece como desativado, religar).
- [ ] **Step 6: Commit** — `feat(cli): startup command; docs for phase 4a`
