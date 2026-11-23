# Sentinela — Fase 3: central de aplicativos — Plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Uma página Aplicativos que lista um catálogo curado de ~60 programas, mostra quais estão instalados ou têm atualização, e instala ou atualiza os escolhidos pelo winget numa fila com progresso, além de `sentinela apps list/install/upgrade` no terminal.

**Architecture:** `crates/core/src/apps/` guarda o catálogo embutido (`catalog/apps.json` via `include_str!`), os parsers da saída do winget e o `trait PackageManager` com a implementação `Winget` (subprocesso oculto atrás de um `trait CommandRunner`, falso nos testes). A janela usa um job `JobKind::Apps` que instala um item por vez e emite progresso por item. O CLI usa o mesmo módulo.

**Tech Stack:** Rust 2024, winget (App Installer), serde_json, Tauri 2, React + TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-11-16-sentinela-gui-otimizacao-design.md` — §2 (só winget, sem URL direta), §5 (tudo que a janela faz o CLI também faz), §8, §10 (barra lateral), §11 fase 3, §13 (`PackageManager` falso e fixtures reais do winget).

**Fora deste plano:** detecção do bloqueio do Windows Update (§6.5, fase 4); Wallpaper Engine (fase 5).

## Global Constraints

- Só Windows; CI só `windows-latest`; builds locais com `CARGO_TARGET_DIR=C:/st`.
- Toda tarefa termina com `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `npm test` e `npm run build` verdes.
- Aplicativos só via winget. Nunca baixar instalador por URL direta.
- Comando de instalação exato: `winget install --id <id> -e --silent --accept-package-agreements --accept-source-agreements --disable-interactivity`. Atualização: `winget upgrade --id <id> -e --silent --accept-package-agreements --accept-source-agreements --disable-interactivity`.
- O subprocesso roda sem janela (`CREATE_NO_WINDOW`) e sem shell; argumentos passados como lista, nunca concatenados em string.
- Instalar ou atualizar só depois de confirmação que diz: `Ao continuar, você aceita os termos de licença de cada aplicativo.`
- Cancelar nunca mata um winget em andamento: termina o item atual e não começa o próximo.
- Nenhum teste automatizado instala, atualiza ou desinstala nada de verdade.
- Textos da interface em pt-BR; `app/src-tauri` sem regra de negócio.

## Review Focus

1. **winget ausente ou antigo:** a página mostra a explicação e o botão da Store em vez de quebrar; o CLI dá erro claro com código 2. → Tarefas 2, 4, 5.
2. **Saída do winget em português ou inglês** (o cabeçalho da tabela muda de idioma): a detecção de atualizações funciona nos dois. → Tarefa 2.
3. **Instalação que falha no meio da fila** (hash diferente, instalador recusado, UAC negado): os outros itens continuam e a falha aparece com o motivo legível, não só o código. → Tarefas 2 e 4.
4. **Aplicativo instalado fora do winget** (instalador baixado na mão): aparece como instalado se o winget o reconhecer pelo id; se não reconhecer, aparece como não instalado, e instalar de novo não pode corromper o existente — o winget decide, e o código "já instalado" é tratado como sucesso. → Tarefa 2.
5. **Fila longa e o usuário sai da página:** o job continua; ao voltar a página mostra a fila em andamento (store global de jobs da fase 1). → Tarefa 4.

---

### Task 1: Catálogo de aplicativos

**Files:**
- Create: `catalog/apps.json`, `crates/core/src/apps/{mod,catalog}.rs`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Produces:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)] #[serde(rename_all = "snake_case")]
pub enum AppCategory { Navegadores, Comunicacao, Jogos, FerramentasJogos, Multimidia, Utilitarios, Desenvolvimento }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct App { pub id: String, pub name: String, pub description: String, pub category: AppCategory, pub winget: String, pub link: String, pub foss: bool }
pub struct AppCatalog { pub apps: Vec<App> }
impl AppCatalog { pub fn embedded() -> &'static AppCatalog; pub fn parse(json: &str) -> Result<AppCatalog, String>; pub fn get(&self, id: &str) -> Option<&App>; }
pub fn validate(c: &AppCatalog) -> Result<(), String>;
pub fn category_label(c: AppCategory) -> &'static str; // "Navegadores", "Comunicação", "Jogos", "Ferramentas para jogos", "Multimídia", "Utilitários", "Desenvolvimento"
```

`validate` falha (mensagem com o id) para: id repetido ou fora de `^[a-z0-9-]+$`; `winget` repetido ou fora de `^[A-Za-z0-9][A-Za-z0-9_+-]*(\.[A-Za-z0-9_+-]+)+$`; `link` sem `https://`; `name`/`description` vazios.

Catálogo: ~60 itens, descrição curta em pt-BR. Obrigatórios: Navegadores (Firefox, Chrome, Brave, Edge não — já vem no Windows); Comunicação (Discord fica em Jogos; WhatsApp, Telegram, Zoom, Teams); Jogos (Steam, Epic Games Launcher, GOG Galaxy, Battle.net, EA app, Ubisoft Connect, Discord); Ferramentas para jogos (MSI Afterburner, HWiNFO, CapFrameX, Display Driver Uninstaller, NVIDIA App, AMD Software Adrenalin); Multimídia (VLC, Spotify, OBS Studio, Audacity, GIMP, HandBrake); Utilitários (7-Zip, Everything, PowerToys, Notepad++, ShareX, qBittorrent, Bitwarden, Cloudflare WARP, WireGuard, Oh My Posh, Windows Terminal); Desenvolvimento (Git, VS Code, Node.js LTS, Python 3, Rust/rustup).

Cada `winget` id é conferido com `winget show --id <id> -e --accept-source-agreements` (só leitura). Os que não existirem saem do catálogo e entram no relatório.

- [ ] **Step 1: Testes que falham** (`catalog.rs`): `embedded_catalog_is_valid` (≥ 50 itens), `duplicate_id_rejected`, `duplicate_winget_rejected`, `bad_winget_id_rejected` (com espaço, sem ponto, começando com `-`), `link_must_be_https`, `required_games_are_present` (ids dos 7 de Jogos).
- [ ] **Step 2:** `cargo test -p sentinela-core apps::catalog` — FAIL.
- [ ] **Step 3:** Implementar e montar o catálogo conferindo cada id.
- [ ] **Step 4:** PASS.
- [ ] **Step 5: Commit** — `feat(core): curated apps catalog`

### Task 2: winget: estado, instalar, atualizar

**Files:**
- Create: `crates/core/src/apps/{winget,parse,runner}.rs`, `crates/core/tests/fixtures/winget/` (saídas reais)

**Interfaces:**
- Consumes: Task 1.
- Produces:

```rust
pub struct Output { pub code: i32, pub stdout: String, pub stderr: String }
pub trait CommandRunner: Send + Sync { fn run(&self, program: &str, args: &[&str]) -> Result<Output, String>; } // Err = não conseguiu iniciar
pub struct SystemRunner; // std::process::Command + CREATE_NO_WINDOW, stdout/stderr capturados como UTF-8 com perdas
#[derive(Debug, Clone, PartialEq, Eq, Serialize)] #[serde(rename_all = "snake_case")]
pub enum AppState { NotInstalled, Installed, UpdateAvailable }
#[derive(Debug, Clone, Serialize)] pub struct Availability { pub available: bool, pub version: Option<String>, pub reason: Option<String> }
#[derive(Debug, Clone, Serialize)] pub struct ItemOutcome { pub id: String, pub ok: bool, pub message: String }
pub trait PackageManager: Send + Sync {
    fn availability(&self) -> Availability;
    fn states(&self, ids: &[&str]) -> Result<HashMap<String, AppState>, String>; // chave = winget id
    fn install(&self, winget_id: &str) -> ItemOutcome;
    fn upgrade(&self, winget_id: &str) -> ItemOutcome;
}
pub struct Winget<R: CommandRunner> { pub runner: R }
pub fn parse_export(json: &str) -> Result<HashSet<String>, String>;          // winget export → ids instalados (minúsculos)
pub fn parse_upgrade_table(text: &str) -> HashMap<String, String>;           // id (minúsculo) → versão disponível
pub fn describe_exit(code: i32) -> (bool, String);                           // (ok, mensagem pt-BR)
pub const MIN_WINGET: (u32, u32) = (1, 6);
```

Regras:
- `availability`: roda `winget --version`; falha ao iniciar → `available: false, reason: "O winget (Instalador de Aplicativo) não está instalado."`; versão abaixo de `MIN_WINGET` → `reason: "Atualize o Instalador de Aplicativo na Microsoft Store."`.
- `states`: instalados via `winget export -o <arquivo temporário> --accept-source-agreements` + `parse_export` (comparação sem diferenciar maiúsculas); atualizações via `winget upgrade --include-unknown --accept-source-agreements` + `parse_upgrade_table`. `UpdateAvailable` só se também instalado.
- `parse_upgrade_table`: localiza a linha de traços (`---…`), usa a linha anterior como cabeçalho, e tira as colunas pela posição em que cada palavra do cabeçalho começa (funciona em pt-BR e en-US); a coluna do id é a 2ª, a da versão disponível é a 4ª; ignora linhas finais de resumo (que não têm o número de colunas); larguras medidas em caracteres, não bytes.
- `describe_exit` (códigos do winget como `i32`): `0` → ok "Instalado."; `0x8A15002B` (sem atualização aplicável) → ok "Já está na versão mais recente."; `0x8A150061` (já instalado) → ok "Já está instalado."; `0x8A150014` (sem pacote) → "Pacote não encontrado no winget."; `0x8A150011` (hash) → "O instalador não confere com o esperado; instalação cancelada."; `0x8A150019`/`1602`/`1223` → "Instalação cancelada (permissão negada ou cancelada pelo usuário)."; outros → `"Falhou (código 0x{:08X})."`. Confira os valores na documentação do winget (`returnCodes.md` em github.com/microsoft/winget-cli) e corrija se algum divergir, registrando no relatório.
- `install`/`upgrade` montam os argumentos exatos das Global Constraints.

- [ ] **Step 1: Fixtures reais.** Gravar em `fixtures/winget/` a saída de `winget upgrade --include-unknown --accept-source-agreements` desta máquina (pt-BR) e de `winget export` (JSON, anonimizar nada: só ids e versões). Criar à mão uma versão en-US da tabela com o mesmo conteúdo e cabeçalho `Name Id Version Available Source`.
- [ ] **Step 2: Testes que falham:** `export_ids_are_lowercased_set`, `upgrade_table_pt_br`, `upgrade_table_en_us`, `upgrade_table_ignores_summary_lines`, `upgrade_table_with_unicode_names` (nome com acento antes do id), `exit_codes_map_to_messages`, `install_uses_exact_args` (runner falso grava os argumentos), `missing_winget_is_unavailable` (runner falso devolve Err), `old_winget_is_unavailable` (`v1.5.0`), `update_requires_installed`.
- [ ] **Step 3:** `cargo test -p sentinela-core apps` — FAIL.
- [ ] **Step 4:** Implementar.
- [ ] **Step 5:** PASS.
- [ ] **Step 6: Commit** — `feat(core): winget package manager with locale-independent parsing`

### Task 3: Comandos da janela

**Files:**
- Create: `app/src-tauri/src/commands/apps.rs`
- Modify: `app/src-tauri/src/{lib,jobs}.rs` (`JobKind::Apps`), `app/src/{api,jobs}.ts` (`"apps"` em `JobKind` e em `KINDS`), `app/src-tauri/capabilities/default.json`

**Interfaces:**
- Consumes: Tasks 1–2; `spawn_job`, `JobRegistry`, `Sink`, `activity`.
- Produces:
  - `apps_page() -> Result<AppsPage, String>` com `AppsPage { availability: Availability, apps: Vec<AppItem>, categories: Vec<{ id: AppCategory, label: String }> }` e `AppItem { app: App, state: Option<AppState> }` (`None` quando o winget não está disponível ou a consulta falhou). Estado em cache por sessão; `apps_page_refresh()` força nova consulta.
  - `apps_install(ids: Vec<String>) -> Result<JobId, String>` e `apps_upgrade(ids: Vec<String>) -> Result<JobId, String>` (`JobKind::Apps`): ids vazios → `Err("Nenhum aplicativo selecionado.")`; desconhecidos → `Err("aplicativo desconhecido: {id}")`; winget indisponível → `Err(reason)`. O job instala um por vez, em ordem; antes de cada item emite `job:progress` `{ job, phase: "apps", index, total, current: "<nome>" }`; checa o cancelamento **entre** itens; resultado `Vec<ItemOutcome>` (com o `id` do catálogo, não o do winget). Depois do job, invalida o cache. Atividade: `ActivityKind::Apps` (nova, `"apps"`) com `Aplicativos instalados: {n}` / `Aplicativos atualizados: {n}` contando só os `ok` que mudaram algo.
  - Permissão do opener para abrir exatamente `ms-windows-store://pdp/?ProductId=9NBLGGH4NNS1` (Instalador de Aplicativo) e os `link` do catálogo — se o escopo de URL não aceitar lista, use `https://*` só para `link` e diga no relatório.
- [ ] **Step 1: Testes que falham** (helpers puros com `PackageManager` falso): `install_rejects_empty_and_unknown`, `unavailable_winget_is_an_error`, `job_continues_after_item_failure`, `cancel_stops_between_items_not_during`, `outcome_uses_catalog_id`, `cache_invalidated_after_job`.
- [ ] **Step 2:** `cargo test -p sentinela-gui apps` — FAIL.
- [ ] **Step 3:** Implementar (comandos `async`, trabalho em `spawn_blocking`).
- [ ] **Step 4:** PASS, `npm run build`, `cargo clippy --workspace`.
- [ ] **Step 5: Commit** — `feat(gui): apps commands with install queue`

### Task 4: Página Aplicativos

**Files:**
- Create: `app/src/pages/Aplicativos.tsx`, `app/src/pages/Aplicativos.test.tsx`
- Modify: `app/src/components/Sidebar.tsx` (`"aplicativos"` depois de Limpeza), `app/src/App.tsx`, `app/src/App.test.tsx`

Comportamento e textos:
- Topo: campo de busca (`Buscar aplicativos`, filtra por nome e descrição, sem diferenciar maiúsculas nem acentos) e filtro de categoria (`Todas` + as 7).
- Cartões em grade: nome, descrição, selo `Código aberto` quando `foss`, selo `Instalado` ou `Atualização disponível`, caixa de seleção (desabilitada quando instalado e sem atualização), link `Site` (abre `link`).
- Barra de ação fixa no rodapé quando há seleção: `{n} selecionado(s)` e botões **"Instalar"** (para não instalados) e **"Atualizar"** (para os com atualização) — cada um age só sobre os itens elegíveis da seleção.
- Confirmação: `Instalar {n} aplicativo(s)?` / `Atualizar {n} aplicativo(s)?`, lista dos nomes, e o texto fixo das Global Constraints.
- Durante o job: `Instalando {index+1} de {total}: {nome}…`, barra de progresso por itens, botão `Parar depois deste` (cancela entre itens). Depois: lista de resultados (sucesso/falha com mensagem) e recarrega o estado.
- Ação secundária no topo: **"Atualizar tudo"** (todos com atualização disponível no catálogo), com a mesma confirmação.
- Sem winget: tela no lugar da grade com `availability.reason`, o texto `O Sentinela usa o winget, o instalador oficial da Microsoft, para baixar cada aplicativo do fabricante e conferir o arquivo.` e o botão **"Abrir na Microsoft Store"**.
- Estado desconhecido (consulta falhou): grade aparece sem selos e com o aviso `Não foi possível verificar o que já está instalado.` e o botão `Tentar de novo`.

- [ ] **Step 1: Testes que falham:** `filters_by_search_ignoring_accents`, `filters_by_category`, `installed_without_update_is_not_selectable`, `install_only_after_confirmation_with_license_text`, `install_sends_only_not_installed_selected`, `update_all_targets_apps_with_updates`, `shows_queue_progress_and_stop_button`, `no_winget_shows_store_button`, `unknown_state_shows_retry`, e no App `sidebar_has_aplicativos_after_limpeza`.
- [ ] **Step 2:** `npm test` — FAIL.
- [ ] **Step 3:** Implementar.
- [ ] **Step 4:** PASS, `npm run build`.
- [ ] **Step 5: Commit** — `feat(gui): apps page with search, categories and install queue`

### Task 5: `sentinela apps` e roteiro manual

**Files:**
- Create: `crates/cli/src/apps.rs`, `docs/testes-manuais-fase-3.md`
- Modify: `crates/cli/src/{cli,main}.rs`, `crates/cli/tests/cli.rs` (só testes novos), `README.md`

```
sentinela apps list [--category <id>] [--json]
sentinela apps install <ID>... [--yes]
sentinela apps upgrade (<ID>... | --all) [--yes]
```

- `list`: `id`, nome, categoria, estado (`instalado` / `atualização disponível` / `não instalado` / `?`).
- `install`/`upgrade`: lista o que vai fazer, mostra o texto de licença das Global Constraints, pede `[s/N]` (como `clean`/`tweak`); `--yes` pula. Um por vez, com uma linha por item (`[1/3] Steam… instalado.`). Ctrl+C para depois do item atual.
- Erros: id desconhecido → código 2 `aplicativo desconhecido: {id}. Use \`sentinela apps list\`.`; winget indisponível → código 2 com o `reason`. Código 1 se algum item falhou.
- Variável `SENTINELA_WINGET` (caminho do executável) só para testes; quando apontar para algo inexistente, simula winget ausente.
- `docs/testes-manuais-fase-3.md`: instalar 2 apps leves (7-Zip e Everything) pela janela, conferir selos, `Atualizar tudo`, recusar o UAC de um instalador (falha legível), parar a fila no meio, `sentinela apps list/install` no terminal, desinstalar pelo Windows e conferir que o selo some após `Tentar de novo`/reabrir.

- [ ] **Step 1: Testes que falham** (sem instalar nada): `apps_list_shows_catalog` (com `SENTINELA_WINGET` inexistente: lista com `?` e código 0), `apps_install_unknown_id_exits_2`, `apps_install_without_winget_exits_2`, `apps_upgrade_requires_ids_or_all`.
- [ ] **Step 2:** `cargo test -p sentinela --test cli apps` — FAIL.
- [ ] **Step 3:** Implementar, README e roteiro.
- [ ] **Step 4:** PASS e `cargo test --workspace`.
- [ ] **Step 5: Commit** — `feat(cli): apps list/install/upgrade`
