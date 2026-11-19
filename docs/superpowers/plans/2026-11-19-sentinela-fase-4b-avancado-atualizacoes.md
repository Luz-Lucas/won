# Sentinela — Fase 4b: aba Avançado, GPU por jogo e Atualizações do Windows — Plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fechar a fase 4 do design: a aba "Avançado" da página Jogos com os ajustes situacionais e controversos e o perfil "FPS competitivo"; GPU de alto desempenho por jogo, com os jogos detectados; e a seção "Atualizações do Windows" com os quatro modos do §6.5, o aviso de dias sem correções e as definições do Defender mantidas em dia.

**Architecture:** Tudo continua passando pelo motor de ajustes da fase 2 (prévia, diário, desfazer). Duas extensões no motor: **ajustes gerados** em tempo de execução (um por jogo, um por adaptador de rede, um por modo de atualização com a data ou a versão do momento), aplicados por um `Catalog` montado na hora e desfeitos pelo diário como qualquer outro; e a ação **`ScheduledTask`**, que liga e desliga uma tarefa do Agendador (o modo de atualização gera uma por tarefa encontrada nas pastas permitidas). Os modos de atualização são um grupo novo, `Atualizacoes`, o único autorizado a tocar em `wuauserv`, `UsoSvc` e `BITS`.

**Tech Stack:** Rust 2024, windows-sys 0.61, Tauri 2 (+ `tauri-plugin-dialog`, já no projeto), React + TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-11-16-sentinela-gui-otimizacao-design.md` — §6.3 (aba Avançado, perfis, GPU por jogo), §6.5 (atualizações), §13 (testes), §14 (riscos), §11 fase 4.

**Fora deste plano:** Modo Jogo sob demanda (§6.4, fase 10); rede (§6.6, fases N1–N2; o Nagle daqui é só o ajuste local do §6.3); QoS por jogo (N2).

## Global Constraints

- Só Windows; CI `windows-latest`; builds locais com `CARGO_TARGET_DIR=E:/st` (o disco C: não tem espaço para o cache de build).
- Toda tarefa termina com `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `npm test` e `npm run build` verdes.
- Nada muda sem prévia e confirmação; tudo é desfazível pelo Histórico, voltando ao valor original da máquina.
- **Nenhum ajuste reduz segurança** (§2): Defender, firewall, VBS e mitigações continuam fora. `WaaSMedicSvc`, `WinDefend` e os demais de `FORBIDDEN_SERVICES` continuam proibidos para todos; `wuauserv`, `UsoSvc` e `BITS` só no grupo `Atualizacoes` (teste de catálogo falha se aparecerem em outro).
- Ajustes "Avançado" nunca entram no perfil Recomendado; os `controverso` também não entram no FPS competitivo.
- Fontes: cada ajuste novo com `source` conferida na documentação da Microsoft ou no `config/tweaks.json` / `functions/public/Invoke-WPFUpdates*.ps1` do winutil; divergências no relatório da tarefa.
- Nenhum teste altera o registro real fora de `HKCU\Software\Sentinela-test-*`, nem serviços ou tarefas reais; leitura real só em `#[ignore]`.
- Textos em pt-BR; `app/src-tauri` sem regra de negócio.

## Review Focus

1. **Trocar de modo de atualização** (Só segurança → Bloqueado → Padrão): o Padrão devolve exatamente o estado original da máquina, sem resíduo do modo anterior (políticas, serviços e tarefas). → Tarefas 2 e 5.
2. **O Windows religa o Windows Update** (serviço volta a Manual, tarefa reabilitada): o modo aparece "Parcial" com o item que voltou, sem reaplicar sozinho. → Tarefa 5.
3. **Tarefas protegidas do Agendador** (várias do `UpdateOrchestrator` negam acesso mesmo com administrador): falha legível por tarefa, o resto do modo aplicado, e o diário só com o que de fato mudou. → Tarefas 2 e 5.
4. **Jogo desinstalado depois de ligar a GPU de alto desempenho:** o ajuste continua no Histórico e é desfeito normalmente; a lista mostra "jogo não encontrado". → Tarefa 3.
5. **Bloqueado sem o Defender desatualizar:** as definições continuam chegando (pelo Sentinela) com o `wuauserv` desativado — ou o modo cai para o plano B do §6.5 (serviço em Manual, bloqueio só por política). → Tarefa 5.

---

### Task 1: Aba Avançado e perfil "FPS competitivo"

**Files:** `catalog/tweaks.json`, `crates/core/src/tweaks/{model,catalog}.rs`, `app/src/pages/Jogos.tsx` (+ teste), `crates/cli/src/tweak.rs`.

Todos com `recommended: false`. Grupo novo `Avancado` (serde `"avancado"`), mostrado na aba "Avançado" da página Jogos; cada item mostra a descrição inteira (o porquê e o risco), não só o título.

| id | evidência | admin | efeito | ação |
|---|---|---|---|---|
| `fullscreen-optimizations-off` | controverso | não | relogon | `HKCU\System\GameConfigStore`: `GameDVR_FSEBehaviorMode` = 2, `GameDVR_HonorUserFSEBehaviorMode` = 1, `GameDVR_DXGIHonorFSEWindowsCompatible` = 1 |
| `mmcss-responsiveness` | situacional | sim | reboot | `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile`: `SystemResponsiveness` = 10 (a documentação do MMCSS diz que valores abaixo de 10 viram 20, então o "0" dos guias não faz nada). `NetworkThrottlingIndex` não tem documentação da Microsoft nessa página e vai para a fase de rede (N1), onde há medição |
| `timer-resolution-global` | controverso | sim | reboot | `HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\kernel`: `GlobalTimerResolutionRequests` = 1; `laptop_warning: true` |
| `services-manual-extra` | situacional | sim | reboot | serviços para Manual: `SysMain`, `MapsBroker`, `StorSvc` (revisão da lista do winutil: só os que ficam em Manual; nada desativado) |
| `search-indexing-off` | situacional | sim | immediate | serviço `WSearch` desativado (a busca do Windows fica mais lenta: dito na descrição) |

`ALLOWED_ROOTS` ganha `(Hklm, "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Multimedia\\SystemProfile")` e `(Hklm, "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\kernel")`.

**Perfis:** `Recomendado` (como hoje) · `FPS competitivo` (Recomendado + `situacional` do grupo Avançado; `controverso` nunca é pré-marcado) · `Personalizado`. Escolher FPS competitivo abre a aba Avançado e mostra o que entrou.

- [ ] **Step 1: Testes que falham:** Rust `advanced_catalog_is_valid` (ids presentes, nenhum `recommended`, `controverso` só no grupo Avançado ou fora de qualquer perfil), `allowed_roots_cover_advanced`; Vitest `advanced_tab_shows_description_and_evidence`, `fps_profile_adds_situational_not_controversial`, `recommended_profile_ignores_advanced`.
- [ ] **Step 2–4:** FAIL → implementar → PASS.
- [ ] **Step 5: Commit** — `feat: advanced tab and competitive FPS profile`

### Task 2: Motor — ajustes gerados, tarefas agendadas e grupo Atualizações

**Files:** `crates/core/src/tweaks/{model,catalog,engine,backend,state,journal}.rs`, `crates/core/src/tweaks/windows/tasks.rs` (nova), testes no próprio módulo.

**Interfaces (produz):**
- `Action::ScheduledTask { path: String, enabled: bool }` com `Snapshot::ScheduledTask { enabled }`: **uma ação por tarefa**, gerada para cada tarefa encontrada nas pastas permitidas. *Decisão na implementação:* uma ação por pasta exigiria um snapshot composto, e uma tarefa protegida deixaria o estado "nem antes nem escrito" (conflito falso no desfazer); por tarefa, cada uma entra e sai do diário sozinha. A ação é de **melhor esforço** (`Action::best_effort`): se negar acesso, vira aviso no resultado ("aplicada em parte; não alterado: …"), o resto do ajuste segue e ela **não** entra no diário.
  - Backend do Windows: `schtasks.exe` do System32 com argumentos fixos, sem shell e sem janela (`/Query /FO CSV /NH` para listar, `/Query /TN … /XML` para ler `Settings/Enabled`, `/Change /TN … /ENABLE|/DISABLE`). *Decisão na implementação:* o `windows-sys` não traz as interfaces COM do Agendador; o XML não depende do idioma, ao contrário da coluna "Status". `ALLOWED_TASK_FOLDERS` (lista fechada, só tarefas diretamente dentro): `\Microsoft\Windows\UpdateOrchestrator`, `\Microsoft\Windows\WindowsUpdate`, `\Microsoft\Windows\InstallService`, `\Microsoft\Windows\UpdateAssistant`, `\Microsoft\Windows\WaaSMedic`, `\Microsoft\WindowsUpdate`.
- Trocar de opção no grupo `Atualizacoes` não passa pela pré-checagem de conflito: o Windows religar algo sozinho é esperado, e o valor atual é mantido e avisado.
- `Group::Atualizacoes` (serde `"atualizacoes"`); `UPDATE_SERVICES = ["wuauserv", "UsoSvc", "BITS"]` só podem aparecer em ajustes desse grupo (`validate` recusa nos outros); continuam fora de `FORBIDDEN_SERVICES` só para esse grupo.
- **Ajustes gerados:** `Catalog::with_generated(&self, extra: Vec<Tweak>) -> Catalog`, validado pelas mesmas regras (raízes, serviços, ids) antes de qualquer aplicação. Id gerado = prefixo + hash curto estável (`gpu-3f9a1c2b`, `nagle-…`, `wu-paused`). O diário guarda id e título; desfazer não depende de o ajuste existir no catálogo (já é assim).
- `validate_choices`: grupos de opções do grupo `Atualizacoes` podem ter chaves diferentes entre as opções (exceção explícita, só nesse grupo); `apply_choice` já desfaz a opção anterior antes.

- [ ] **Step 1: Testes que falham** (backend em memória): `task_apply_and_undo_restores_each_task`, `denied_task_reported_and_not_journaled`, `task_folder_outside_allowlist_rejected`, `update_services_only_in_update_group`, `generated_catalog_is_validated`, `generated_tweak_undo_after_it_disappears`, `update_choice_switch_leaves_no_residue` (opções com chaves diferentes: A → B → undo volta ao original).
- [ ] **Step 2–4:** FAIL → implementar → PASS; `#[ignore]` de leitura real das tarefas (só leitura).
- [ ] **Step 5: Commit** — `feat(core): generated tweaks, scheduled task folders and update group`

### Task 3: GPU de alto desempenho por jogo

**Files:** `crates/core/src/games/{mod,steam,epic,riot}.rs` (novas), `app/src-tauri/src/commands/games.rs`, `app/src/components/GpuPerGame.tsx` (+ teste), seção "GPU por jogo" na página Jogos.

- **Detecção** (só leitura, atrás de `trait GameSource` com versões falsas nos testes):
  - Steam: caminho em `HKCU\Software\Valve\Steam\SteamPath`; bibliotecas em `steamapps\libraryfolders.vdf`; jogos por `appmanifest_*.acf` (`name`, `installdir`); executáveis = `.exe` na raiz da pasta do jogo, sem `unins*`, `*crash*`, `*redist*`, `*setup*`, `*launcher*` (o usuário escolhe se houver mais de um).
  - Epic: `%ProgramData%\Epic\EpicGamesLauncher\Data\Manifests\*.item` (JSON: `DisplayName`, `InstallLocation`, `LaunchExecutable`).
  - Riot: `%ProgramData%\Riot Games\RiotClientInstalls.json` e os caminhos conhecidos de VALORANT e League of Legends.
  - Battle.net: o `product.db` é binário; nesta fase os jogos dele entram pelo botão **"Adicionar jogo…"** (escolher o `.exe`, que também serve para qualquer outro jogo). Divergência do §6.3 registrada.
- **Ajuste gerado por executável:** `ListEntry` em `HKCU\Software\Microsoft\DirectX\UserGpuPreferences`, valor = caminho completo do `.exe`, item `GpuPreference=2;` (o mesmo que Configurações → Sistema → Tela → Gráficos grava). Efeito: próxima abertura do jogo.
- **Página:** lista dos jogos detectados e adicionados, com origem, interruptor "Alto desempenho" (prévia e confirmação, como Personalizar) e "jogo não encontrado" quando o `.exe` sumiu (ainda desfazível). Em máquina com uma GPU só, aviso "Esta máquina tem uma GPU; a preferência não muda nada." (detecção por DXGI) e os interruptores continuam disponíveis.

- [ ] **Step 1: Testes que falham:** parsers com fixtures (`libraryfolders.vdf` com duas bibliotecas, `appmanifest`, `.item` da Epic, `RiotClientInstalls.json`), `exe_filter_drops_helpers`, `gpu_tweak_targets_exact_exe_path`, `missing_game_still_undoable`; Vitest `gpu_toggle_asks_confirmation`, `add_game_uses_file_picker`, `missing_game_label`, `single_gpu_notice`.
- [ ] **Step 2–4:** FAIL → implementar → PASS; `#[ignore]` de detecção real (só leitura).
- [ ] **Step 5: Commit** — `feat: high-performance GPU per detected game`

### Task 4: Nagle por adaptador (aba Avançado)

**Files:** `crates/core/src/tweaks/generated/nagle.rs`, catálogo gerado entra na aba Avançado.

- Um ajuste gerado `nagle-off` (situacional, admin, reboot) com, para cada interface com endereço IPv4 configurado (`HKLM\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\{GUID}` com `DhcpIPAddress` ou `IPAddress` não vazio): `TcpAckFrequency` = 1 e `TCPNoDelay` = 1. Descrição: "Só afeta jogos que usam TCP; a maioria dos jogos competitivos usa UDP." `ALLOWED_ROOTS` ganha só esse caminho de interfaces.
- Nenhuma interface → o ajuste aparece "Indisponível nesta máquina".

- [ ] **Step 1: Testes que falham:** `nagle_targets_only_configured_interfaces`, `nagle_without_interfaces_is_unavailable`, `nagle_in_fps_profile`.
- [ ] **Step 2–4:** FAIL → implementar → PASS.
- [ ] **Step 5: Commit** — `feat(core): Nagle per network adapter in the advanced tab`

### Task 5: Atualizações do Windows

**Files:** `crates/core/src/updates/{mod,defender}.rs` (novas), `app/src-tauri/src/commands/updates.rs`, `app/src/components/WindowsUpdates.tsx` (+ teste), seção na página Ajustes; `crates/core/src/clean/platform.rs` (teste).

Os modos são um `choice` gerado (`windows-update`) do grupo `Atualizacoes`, todos com `needs_admin: true`; **Padrão** = desfazer o modo ativo pelo diário (nunca o "reset" destrutivo do winutil).

| Modo | Ações |
|---|---|
| **Só segurança** (`wu-security`) | `HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsUpdate`: `TargetReleaseVersion` = 1, `ProductVersion` = "Windows 11", `TargetReleaseVersionInfo` = versão atual (`DisplayVersion` lida na hora, ex.: "24H2"), `DeferFeatureUpdates` = 1, `DeferFeatureUpdatesPeriodInDays` = 365, `DeferQualityUpdates` = 1, `DeferQualityUpdatesPeriodInDays` = 4; `…\WindowsUpdate\AU` `AUOptions` = 3 |
| **Pausado até…** (`wu-paused`) | `HKLM\SOFTWARE\Microsoft\WindowsUpdate\UX\Settings`: `PauseUpdatesStartTime`, `PauseUpdatesExpiryTime`, `PauseFeatureUpdatesStartTime`, `PauseFeatureUpdatesEndTime`, `PauseQualityUpdatesStartTime`, `PauseQualityUpdatesEndTime` (texto ISO 8601 UTC); data escolhida entre amanhã e 35 dias (o máximo da tela do Windows) |
| **Bloqueado** (`wu-blocked`) | `…\WindowsUpdate\AU` `NoAutoUpdate` = 1 e `AUOptions` = 1; serviços `wuauserv`, `UsoSvc` e `BITS` desativados; pastas de tarefas do `ALLOWED_TASK_FOLDERS` desligadas |

`ALLOWED_ROOTS` ganha `(Hklm, "SOFTWARE\\Policies\\Microsoft\\Windows\\WindowsUpdate")` e `(Hklm, "SOFTWARE\\Microsoft\\WindowsUpdate\\UX\\Settings")`.

- **Reconferência:** ao abrir a página (e ao iniciar o app), o estado do modo ativo é relido; o que voltou aparece em "Parcial", item a item. O Sentinela **não** reaplica sozinho; oferece "Aplicar de novo".
- **Defender (§6.5):** em Bloqueado e Pausado, o app roda `MpCmdRun.exe -SignatureUpdate` (caminho a partir de `%ProgramFiles%\Windows Defender`, sem shell) no máximo uma vez por dia, enquanto estiver aberto ou na bandeja, e registra na atividade. Se, na VM, a atualização falhar com o `wuauserv` desativado, o modo Bloqueado passa a deixar o `wuauserv` em **Manual** (plano B do §6.5) e o texto do modo diz isso.
- **Avisos ao escolher Bloqueado:** a Microsoft Store e alguns instaladores (pacotes `msstore` do winget, .NET, drivers) falham; a janela recomenda "Só segurança".
- **Limpeza `windows-update`:** já devolve o serviço ao estado em que o encontrou; fica garantido por teste que um serviço parado (como no Bloqueado) não é iniciado.

- [ ] **Step 1: Testes que falham** (backend em memória): `each_mode_applies_and_default_restores_original`, `switching_modes_leaves_no_residue`, `paused_dates_are_utc_iso_and_capped`, `security_mode_pins_current_display_version`, `reverted_service_shows_partial_without_reapplying`, `defender_update_runs_at_most_daily`, `clean_windows_update_does_not_start_stopped_service`; Vitest `mode_selector_shows_preview_and_warnings`, `blocked_requires_admin`, `partial_lists_what_came_back`.
- [ ] **Step 2–4:** FAIL → implementar → PASS; `#[ignore]` de leitura real do estado (só leitura).
- [ ] **Step 5: Commit** — `feat: Windows Update modes with Defender definitions kept current`

### Task 6: Aviso no Início, "Atualizar agora" e Aplicativos

**Files:** `app/src/pages/Inicio.tsx`, `app/src/pages/Aplicativos.tsx`, `app/src-tauri/src/commands/updates.rs` (+ testes).

- **Início:** em Bloqueado ou Pausado, cartão "Atualizações do Windows: bloqueadas há N dias" (a partir do `applied_at` do modo no diário) com o botão **"Atualizar agora"**.
- **Atualizar agora:** desfaz o modo (com confirmação), abre `ms-settings:windowsupdate` e mostra "Atualizações liberadas temporariamente" com **"Voltar a {modo}"**. Detectar o fim da instalação de forma confiável não é possível sem ficar consultando o serviço; nesta fase a volta é por esse botão, e o Início lembra a cada abertura enquanto o modo estiver suspenso. Divergência do §6.5 registrada.
- **Aplicativos:** com o modo Bloqueado ativo, aviso no topo de que instalações da Microsoft Store e alguns instaladores podem falhar, com o mesmo "Atualizar agora".

- [ ] **Step 1: Testes que falham:** Vitest `home_shows_days_without_updates`, `update_now_undoes_and_offers_return`, `apps_page_warns_when_blocked`; Rust `days_since_mode_from_journal`.
- [ ] **Step 2–4:** FAIL → implementar → PASS.
- [ ] **Step 5: Commit** — `feat(gui): days without updates on Home and update-now flow`

### Task 7: CLI e roteiro manual

**Files:** `crates/cli/src/{tweak,updates,games}.rs`, README, `docs/testes-manuais-fase-4b.md`.

- `sentinela updates status [--json]`, `sentinela updates mode (default|security|paused --until AAAA-MM-DD|blocked) [--yes]`.
- `sentinela games list [--json]` (jogos detectados e estado da GPU), `sentinela games gpu (on|off) <ID> [--yes]`.
- `tweak list` mostra o grupo Avançado.

- [ ] **Step 1: Testes que falham** (sem alterar o sistema): `updates_status_runs`, `updates_mode_unknown_exits_2`, `updates_paused_requires_until`, `games_list_runs`, `games_gpu_unknown_id_exits_2`, `tweak_list_includes_advanced`.
- [ ] **Step 2–4:** FAIL → implementar → PASS.
- [ ] **Step 5:** Roteiro manual em VM com snapshot: cada modo de atualização aplicado e desfeito (conferir políticas, serviços e tarefas), Windows religando o serviço (estado Parcial), Defender atualizando com o modo Bloqueado, Microsoft Store falhando e "Atualizar agora", GPU por jogo conferida em Configurações → Gráficos, aba Avançado e perfil FPS competitivo, Nagle por adaptador.
- [ ] **Step 6: Commit** — `feat(cli): updates and games commands; docs for phase 4b`
