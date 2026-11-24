# Sentinela — Fase S2: risco por contexto, Authenticode, compactados, cache, pausa em jogos e clique direito — Plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Completar o antivírus do §15: nível "Risco" abaixo de "Suspeito" para arquivos que nada casou mas estão num contexto ruim; validação Authenticode de verdade (assinatura válida tira pontos da heurística e confirma o que é da Microsoft); zip verificado por dentro com limites contra bombas; cache de verificados para a próxima verificação não reler o que não mudou; verificação em segundo plano com pausa (do usuário e automática quando um jogo abre); e "Verificar com o Sentinela" no clique direito do Explorador.

**Architecture:** `core::scan::authenticode` chama `WinVerifyTrust` (assinatura embutida e, se não houver, a de catálogo) e devolve o editor. `core::scan::context` é um detector novo que produz achados de nível `Risk` a partir do caminho, do cabeçalho, de o arquivo iniciar sozinho e da assinatura. `core::scan::archive` abre zips e passa cada entrada pelos mesmos detectores. `core::scan::cache` guarda tamanho, datas e SHA-256 dos arquivos limpos, por versão das regras. `run_scan` ganha `autostart`, cache, pausa e modo de segundo plano. `core::scan::explorer` grava os verbos do menu de contexto em `HKCU\Software\Classes`. A vigia de jogos da janela liga e desliga a pausa automática.

**Tech Stack:** Rust 2024, windows-sys 0.61 (`WinTrust`, `Cryptography`, `Cryptography_Catalog`, `Registry`, `Threading`), `zip` 9, Tauri 2, React + TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-11-16-sentinela-gui-otimizacao-design.md` — §15.1 a §15.4, §11 fase S2.

## Decisões

- **Authenticode:** `WinVerifyTrust` com `WINTRUST_ACTION_GENERIC_VERIFY_V2`, sem consulta de revogação na rede (`WTD_REVOKE_NONE`, `WTD_CACHE_ONLY_URL_RETRIEVAL`), para não travar nem depender de internet. Sem assinatura embutida, procura a do catálogo do Windows (`CryptCATAdmin…`), que é como a maior parte do System32 e dos drivers é assinada. Resultado: `Valid { signer }`, `Invalid`, `Unsigned`. Só é consultada quando muda alguma decisão (heurística perto do limiar, regra de contexto candidata, trava da quarentena), porque custa caro.
- **Heurística de PE com assinatura:** válida tira 3 pontos; presente mas inválida soma 2; ausente soma 1 (como antes). Nunca anula hash de malware conhecido nem regra YARA.
- **Microsoft:** editor cujo nome começa com "Microsoft " (por exemplo "Microsoft Windows", "Microsoft Corporation"). Na quarentena, arquivo assinado pela Microsoft pede a mesma segunda confirmação da pasta do Windows.
- **Nível `Risk`** (entre `Info` e `Suspicious`): achados do detector `context`. Regras: (1) executável sem assinatura válida que inicia sozinho a partir de Temp ou AppData; (2) extensão dupla (`foto.jpg.exe`) ou cabeçalho `MZ` com extensão de documento ou mídia; (3) script na pasta Inicializar; (4) atalho `.lnk` que chama o PowerShell com comando codificado (`-enc`, `-encodedcommand`, `-e `); (5) Office com macro de execução automática (`AutoOpen`, `Document_Open`, `Workbook_Open`, `Auto_Open` no `vbaProject.bin` do OOXML ou no arquivo OLE antigo). Vem desmarcado; o código de saída do CLI continua sendo 1 só para Suspeito ou pior.
- **Compactados:** só `.zip` nesta versão. Limites: 10.000 entradas, 64 MB por entrada descompactada, 512 MB no total, dois níveis de zip dentro de zip, taxa de compressão até 200:1 por entrada. O que passa do limite, está protegido por senha ou usa método não suportado aparece em "Não verificados" com o motivo. O achado aponta o compactado (o caminho que vai para a quarentena) e diz em que entrada estava; a janela avisa antes que a quarentena é do compactado inteiro.
- **Cache:** `%LOCALAPPDATA%\Sentinela\scan-cache.bin` (binário simples) com caminho, tamanho, data de modificação, data de criação e SHA-256 de cada arquivo **limpo** percorrido nas pastas. A chave das regras é o SHA-256 das fontes YARA mais a versão das heurísticas: se mudou, o cache inteiro é descartado. Arquivo inalterado não é relido: só o hash guardado é conferido contra o banco de hashes (que pode ter crescido). O identificador do arquivo do NTFS fica de fora porque exige abrir cada arquivo, o que anularia o ganho. Arquivos avulsos (processos, inicialização) sempre são relidos. Só a janela usa o cache; o CLI não.
- **Segundo plano:** a verificação da janela roda num conjunto de threads com metade dos processadores (mínimo 1), cada uma em `THREAD_MODE_BACKGROUND_BEGIN` (prioridade baixa de CPU e de disco). Pausar e continuar pelo botão; a vigia de jogos pausa sozinha enquanto algum jogo detectado estiver aberto e continua quando fecham. Cancelar continua devolvendo o relatório parcial.
- **Clique direito:** verbos em `HKCU\Software\Classes\*\shell\SentinelaScan`, `Directory\shell\…` e `Drive\shell\…`, com `"sentinela-gui.exe" --scan "%1"`. No Windows 11 aparecem em "Mostrar mais opções". Registrados pelo app instalado ao abrir, com opção em Ajustes (ligada por padrão); desligar apaga as chaves. A janela abre em Segurança e começa uma verificação personalizada do item.
- **Cobertura:** sem administrador, o resultado diz que pastas ficaram de fora por acesso negado e oferece "Reiniciar como administrador".
- **Fica de fora:** reconhecer os binários do agente e da DLL de mods pelo hash da instalação (chegam na fase 11) e verificar temas e pacotes antes de aplicar (fase 5).

## Global Constraints

- Só Windows; CI `windows-latest`; builds locais com `CARGO_TARGET_DIR=E:/st`.
- Toda tarefa termina com `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `npm test` e `npm run build` verdes.
- Testes sem rede; leitura real (Authenticode de arquivos do sistema, registro) em `#[ignore]`.

## Review Focus

1. **Assinatura nunca anula hash ou YARA.** → Tarefa 1.
2. **Bomba de compactação** não consome memória sem limite. → Tarefa 3.
3. **Cache nunca esconde malware novo:** hash conferido sempre; regras novas descartam o cache. → Tarefa 4.
4. **Pausa e cancelamento** juntos não travam a verificação. → Tarefa 5.

---

### Task 1: Authenticode

**Files:** `crates/core/src/scan/authenticode.rs`, `scan/pe.rs`, `scan/guard.rs`.

- [ ] `verify(path) -> Trust`; `is_microsoft(signer)`; `pe::score_with_trust`; `Guard::MicrosoftSigned`.
- [ ] Testes: `signature_lowers_pe_score_but_never_hash`, `invalid_signature_adds_points`, `microsoft_signer_names`, `#[ignore] real_system_files_are_signed`.
- [ ] Commit `feat(core): real Authenticode validation`.

### Task 2: Nível Risco por contexto

**Files:** `scan/detector.rs` (`Severity::Risk`), `scan/context.rs`, `scan/mod.rs` (`autostart`), `scan/areas.rs`, `scan/explain.rs`, CLI `report.rs`.

- [ ] As cinco regras; `FileData.autostart`; itens a partir de `Risk`; código de saída inalterado.
- [ ] Testes: `double_extension_is_risk`, `mz_disguised_as_document`, `script_in_startup_folder`, `lnk_with_encoded_powershell`, `office_auto_exec_macro`, `unsigned_autostart_in_appdata`, `risk_does_not_change_exit_code`.
- [ ] Commit `feat(core): Risco level from file context`.

### Task 3: Compactados

**Files:** `scan/archive.rs`, `scan/mod.rs`.

- [ ] Entradas pelos detectores; limites; `ScanItem.in_archive`; motivos em "Não verificados".
- [ ] Testes: `finds_marker_inside_zip`, `nested_zip_depth_limit`, `zip_bomb_is_not_verified`, `encrypted_entry_is_reported`.
- [ ] Commit `feat(core): scan inside zip archives with bomb limits`.

### Task 4: Cache de verificados

**Files:** `scan/cache.rs`, `scan/mod.rs`.

- [ ] Carregar, consultar, gravar; chave das regras; só arquivos limpos das pastas.
- [ ] Testes: `unchanged_file_is_not_reread`, `new_hash_signature_still_detects_cached_file`, `rules_change_drops_cache`, `changed_file_is_rescanned`, `corrupt_cache_is_ignored`.
- [ ] Commit `feat(core): scanned-file cache`.

### Task 5: Segundo plano e pausa

**Files:** `scan/mod.rs`, `app/src-tauri/src/commands/scan.rs`, `game_watch.rs`.

- [ ] `ScanOptions { pause, background }`; comando `scan_pause`; pausa automática em jogos; evento `scan:paused`.
- [ ] Testes: `pause_holds_until_resumed`, `cancel_while_paused_finishes`, `game_pause_state`.
- [ ] Commit `feat: background scan with pause and automatic pause in games`.

### Task 6: Clique direito e janela

**Files:** `scan/explorer.rs`, `settings.rs`, `lib.rs`, páginas Segurança e Ajustes.

- [ ] Verbos do menu; `--scan <caminho>`; janela: nível Risco, entrada do compactado e aviso, trava da Microsoft, Pausar/Continuar e "pausada: jogo aberto", aviso de cobertura com elevar, opção do menu em Ajustes.
- [ ] Testes: `menu_keys_and_command`, `scan_arg_parsing`; Vitest `risk_level_is_listed_unchecked`, `archive_warning_before_quarantine`, `microsoft_signed_asks_again`, `pause_and_resume`, `paused_by_game`, `elevate_offer_on_access_denied`, `explorer_request_starts_custom_scan`, `explorer_menu_setting`.
- [ ] Commit `feat: Explorer right-click scan and window for phase S2`.

### Task 7: CLI e roteiro manual

- [ ] `[RISCO]` no relatório; README; `docs/testes-manuais-fase-s2.md`.
- [ ] Commit `docs: phase S2 manual test script and README`.
