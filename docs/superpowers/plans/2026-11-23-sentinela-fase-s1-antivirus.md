# Sentinela — Fase S1: antivírus na janela — Plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Levar o antivírus sob demanda para a janela (§15): verificação rápida pelas áreas críticas (processos, o que inicia sozinho, pastas de risco) e só pelos tipos de arquivo de risco; completa; personalizada com várias pastas e discos; achados com ação recomendada e ações por achado (quarentena, ignorar sempre, abrir pasta, detalhes, VirusTotal pelo hash); quarentena com apagar definitivamente e arquivos guardados codificados; verificação rápida semanal agendada com notificação; e assinaturas atualizadas sozinhas ao abrir o app.

**Architecture:** `core::scan::areas` monta as áreas críticas (raízes recursivas e arquivos avulsos) a partir de fontes injetáveis (processos, inicialização, tarefas, serviços, pastas), e `core::scan::risk` diz se um arquivo é de risco pelo cabeçalho ou pela extensão; `run_scan` ganha arquivos avulsos e o filtro de risco. `core::scan::exceptions` guarda os hashes ignorados. `core::quarantine` passa a guardar os arquivos codificados (o formato antigo continua restaurável) e ganha `delete`. `core::scan::guard` decide as travas antes da quarentena (pasta do Windows, processo em execução). `core::scan::schedule` gera a tarefa do Agendador. A janela chama tudo por comandos Tauri; a verificação agendada roda o mesmo executável com `--scheduled-scan`.

**Tech Stack:** Rust 2024, windows-sys 0.61 (`ToolHelp`, `Threading`), `schtasks.exe`, Tauri 2 (notification, opener), React + TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-11-16-sentinela-gui-otimizacao-design.md` — §15 (Antivírus), §11 fase S1.

## Decisões

- **Fica para a S2** (como o §11 divide): nível "Risco" por contexto, validação Authenticode, dentro de compactados, cache de verificados, pausa em jogos, clique direito no Explorador. Por isso, na S1, a segunda confirmação da quarentena vale para arquivos dentro de `%SystemRoot%` (a de "assinado pela Microsoft" chega com a validação Authenticode na S2), e compactados são verificados como um arquivo só, sem abrir.
- **Rápida:** executáveis dos processos abertos; alvos das chaves `Run`, das pastas Inicializar, das tarefas agendadas (XML em `%SystemRoot%\System32\Tasks`, que só se lê como administrador — sem ele, aparece em "Não verificados") e dos serviços (`ImagePath`); Downloads, Área de Trabalho, Documentos, `%TEMP%`, `%APPDATA%` (inclui as extensões do Firefox), `%LOCALAPPDATA%` (inclui as extensões do Chrome, Edge e Brave), `%ProgramData%`, `Users\Public` e os arquivos soltos na raiz de cada disco fixo. Nas pastas, só arquivos de risco; processos e itens de inicialização sempre são lidos. A pasta do próprio Sentinela (banco e quarentena) fica de fora em todos os modos.
- **Tipos de risco:** cabeçalho `MZ` (qualquer extensão), scripts (`.ps1 .psm1 .bat .cmd .vbs .vbe .js .jse .wsf .wsh .hta`), instaladores (`.msi .msix .msixbundle .appx .appxbundle`), atalhos (`.lnk .url .scf`), Office com macro (`.docm .xlsm .pptm .dotm .xltm .xlam .ppam .doc .xls .ppt`) e compactados (`.zip .rar .7z .iso .img .cab`), como arquivo.
- **Achados:** o resultado leva o SHA-256 de cada arquivo e um "por quê" em português por achado. Malicioso por hash vem marcado para quarentena; o resto vem desmarcado. Nada é movido sem o usuário clicar.
- **Processo em execução:** a quarentena avisa e oferece encerrar o processo antes (`TerminateProcess`); se continuar bloqueado, o item fica "em uso". Sem remoção no boot.
- **Ignorar sempre:** lista por SHA-256 em `%LOCALAPPDATA%\Sentinela\scan-exceptions.json`, editável em Ajustes. Itens ignorados somem do resultado e aparecem só como contagem.
- **VirusTotal:** abre `https://www.virustotal.com/gui/file/{sha256}` no navegador; nenhum arquivo é enviado.
- **Quarentena codificada:** novos arquivos são gravados com XOR de uma chave fixa de 32 bytes (`encoding: "xor1"` no índice); entradas antigas (sem `encoding`) restauram como antes. O hash conferido é sempre o do conteúdo original.
- **Agendamento:** tarefa `\Sentinela\Verificacao rapida semanal` do usuário atual (sem administrador), criada por `schtasks /Create /XML`, domingo 12h com "executar assim que possível se perdida", rodando `sentinela-gui.exe --scheduled-scan`. Se o app já está aberto, a instância única repassa o pedido; se não, o app abre escondido, verifica, notifica e fecha. O resultado vai para a Atividade e para uma notificação. Oferecida ao fim da primeira verificação pela janela (uma vez).
- **Quarentena automática** (Ajustes, desligada por padrão): só na verificação agendada, só malware confirmado por hash, e nunca em `%SystemRoot%` ou em processo aberto.
- **Assinaturas:** ao abrir o app, se o banco já foi baixado alguma vez e a fonte mais nova tem mais de 24 h, atualiza em segundo plano como uma operação "update" (a mesma do botão de Ajustes). Banco nunca baixado continua esperando o usuário.

## Global Constraints

- Só Windows; CI `windows-latest`; builds locais com `CARGO_TARGET_DIR=E:/st`.
- Toda tarefa termina com `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `npm test` e `npm run build` verdes.
- Honestidade (§15): é sob demanda, sem proteção em tempo real; o Defender continua ligado e a janela diz isso; "nada encontrado" nunca é garantia.
- Testes sem rede, sem mexer em tarefas reais nem em processos reais (fontes injetáveis; leitura real em `#[ignore]`).

## Review Focus

1. **Nada se move sozinho** pela janela; a quarentena confere o hash antes de mover. → Tarefas 2 e 4.
2. **Entradas antigas da quarentena** continuam restauráveis. → Tarefa 3.
3. **A rápida não lê o próprio banco** (as regras YARA casariam com elas mesmas). → Tarefa 1.

---

### Task 1: Áreas críticas e tipos de risco

**Files:** `crates/core/src/scan/risk.rs`, `scan/areas.rs`, `scan/mod.rs` (arquivos avulsos, filtro, `sha256` no item), `scan/targets.rs`, `games/running.rs` (`pid`).

- [x] `risk::is_risky(path, head)`; `areas::QuickAreas { roots, files, unreadable }` montado por `areas::collect(&Sources)`, com `Sources` injetável e `system_sources()` real.
- [x] `ScanOptions { files, risky_only }`; `ScanItem.sha256`; exclusão da pasta do Sentinela.
- [x] Testes: `risky_by_header_and_extension`, `quick_areas_dedupe_and_skip_missing`, `quick_reads_only_risky_files_in_folders`, `explicit_files_always_scanned`, `own_folder_is_excluded`.
- [x] Commit `feat(core): quick scan of critical areas and risky file types`.

### Task 2: Achados, travas e exceções

**Files:** `scan/explain.rs`, `scan/guard.rs`, `scan/exceptions.rs`, `scan/targets.rs`.

- [x] Texto do "por quê" por detector; `recommended` (só hash) no item; `guard::check(path, system_root, procs)` → `SystemFolder`, `InUse { pids }`; `kill(pids)`; exceções por hash (`load/add/remove`), filtradas no resumo (`ignored`).
- [x] Testes: `hash_hits_are_recommended`, `system_root_needs_second_confirmation`, `running_file_reports_pids`, `exceptions_round_trip_and_filter`.
- [x] Commit `feat(core): finding explanations, quarantine guards and exceptions`.

### Task 3: Quarentena codificada e apagar

**Files:** `crates/core/src/quarantine.rs`, CLI `quarantine --delete`.

- [x] `encoding` no índice; gravar codificado e conferir o hash do original; restaurar os dois formatos; `delete(dir, id)`.
- [x] Testes: `stored_bytes_are_encoded`, `legacy_entry_still_restores`, `delete_removes_file_and_entry`.
- [x] Commit `feat(core): encoded quarantine and permanent delete`.

### Task 4: Agendamento, quarentena automática e assinaturas

**Files:** `crates/core/src/scan/schedule.rs`, `settings.rs`, `app/src-tauri/src/scheduled.rs`, `lib.rs`.

- [x] XML da tarefa, `schtasks` (criar, apagar, consultar); `Settings { auto_quarantine_hash, scan_schedule_offered }`; `--scheduled-scan` (instância única, ou escondida e fecha); notificação; atualização das assinaturas com mais de 24 h.
- [x] Testes: `task_xml_runs_scheduled_scan`, `auto_quarantine_only_hash_hits_outside_windows`, `signatures_stale_after_24h`, `scheduled_arg_parsing`.
- [x] Commit `feat: weekly scheduled quick scan and signature auto-update`.

### Task 5: Janela

- [x] Segurança: botões Rápida, Completa, Personalizada (várias pastas e discos); aviso do Defender; achados com caixas, "por quê", SHA-256, Detalhes, Abrir pasta, VirusTotal, Ignorar sempre; "Colocar em quarentena" com as travas; oferta de agendamento; página Quarentena (restaurar, apagar definitivamente). Ajustes: verificação agendada, quarentena automática, exceções.
- [x] Testes Vitest: `hash_hits_come_checked`, `quarantine_asks_again_for_windows_folder`, `in_use_offers_to_end_process`, `ignore_always_adds_exception`, `virustotal_opens_hash_url`, `delete_needs_confirmation`, `schedule_offer_after_first_scan`, `exceptions_in_settings`.
- [x] Commit `feat(gui): antivirus page with actions, quarantine page and schedule`.

### Task 6: CLI e roteiro manual

- [x] `sentinela scan --quick [--json]`; `sentinela quarantine --delete <id>`; README; `docs/testes-manuais-fase-s1.md`.
- [x] Testes CLI: `scan_quick_runs`, `quarantine_delete_unknown_id_exits_2`.
- [x] Commit `feat(cli): quick scan and quarantine delete; docs for phase S1`.
