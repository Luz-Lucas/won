# Sentinela — Fase 5T: Terminal (Oh My Posh) — Plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Status:** rascunho para aprovação. Nada foi implementado.

**Goal:** Personalizar o terminal pela janela: instalar o Oh My Posh e uma Nerd Font, escolher o tema do prompt, ligar nos PowerShells, e editar as chaves gerenciadas do `settings.json` do Windows Terminal, tudo com prévia, backup e desfazer, e levar isso para os Temas e para as cores do wallpaper (fase 5).

**Architecture:** `core::terminal` com quatro partes testáveis sem Windows: `profile` (bloco marcado `# >>> sentinela >>>` no `$PROFILE`, inserir/atualizar/remover sem tocar no resto), `wt_settings` (JSON com comentários: altera só as chaves gerenciadas, preserva o resto e os comentários, recusa se não puder ler sem perda), `omp` (detecção, instalação via o `PackageManager` de `core::apps` com `JanDeDobbeleer.OhMyPosh`, galeria de temas do próprio programa, validação de tema que não executa comandos) e `backup` (cópia datada em `%LOCALAPPDATA%\Sentinela\backups\terminal\` antes da primeira escrita, entrada no histórico com desfazer, no mesmo molde do histórico de temas). Um `trait TerminalHost` (Windows + falso) executa `oh-my-posh` e o instalador de fonte. `core::theme` ganha a seção `[terminal]` (tema do prompt, fonte, esquema de cores) e a cor do wallpaper gera o esquema do Windows Terminal. Página nova "Terminal" em Personalizar.

**Tech Stack:** Rust 2024, `serde_json`, `toml`, `json5`-like parser com preservação de comentários (a decidir na Tarefa 2, ver decisões), Tauri 2, React + TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-11-16-sentinela-gui-otimizacao-design.md`, §9.7, §9.5, §11 fase 5T.

## Decisões a confirmar

- **Editar `settings.json` com comentários:** a opção mais segura é editar o texto no lugar (localizar a chave e trocar só o valor), sem reserializar o arquivo. Se uma chave gerenciada não puder ser localizada com certeza, o Sentinela recusa e mostra o diff que faria. Nenhuma dependência nova de JSON5.
- **Fonte:** o `oh-my-posh font install Meslo` é o único caminho (sem administrador, por usuário). Lista curta: Meslo (padrão), FiraCode, CascadiaCode, JetBrainsMono. O Sentinela não baixa fontes por conta própria.
- **Shells:** PowerShell 7 e Windows PowerShell ligados na janela; Git Bash e `cmd` (Clink) aparecem como "experimental", desligados e fora desta fase (só o aviso na página).
- **Extras:** `Terminal-Icons` e `PSReadLine` com sugestões por histórico instalados no escopo do usuário (`Install-Module -Scope CurrentUser`), cada um com botão próprio e confirmação.
- **Tema do prompt de terceiros:** é só um arquivo; segmentos de comando (`command`, `script`, `executable`) são desligados na importação e listados, nunca executados pelo Sentinela.
- **Prévia do tema:** renderizada pelo próprio `oh-my-posh` (`print primary` / exportação para imagem) e por amostra de texto quando não houver o programa.

## Global Constraints

- Só Windows; CI `windows-latest`; builds locais com `CARGO_TARGET_DIR=E:/st`.
- Toda tarefa termina com `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `npm test` e `npm run build` verdes.
- Sem administrador e sem injeção: só arquivos do usuário. Nada fora do bloco marcado do perfil nem das chaves gerenciadas do `settings.json` é alterado.
- Antes da primeira escrita em cada arquivo há backup datado; toda mudança tem prévia (diff) e desfazer.
- Testes com sistema de arquivos falso (tempdir) e `TerminalHost` falso; leitura e escrita reais em `#[ignore]`.

## Review Focus

1. **O resto do perfil e do `settings.json` nunca muda:** o bloco marcado e as chaves gerenciadas são a única superfície; um arquivo ilegível é recusado, não consertado. → Tarefas 1 e 2.
2. **Desfazer devolve o arquivo byte a byte** (backup da primeira escrita, e o bloco removido deixa o perfil como estava). → Tarefa 3.
3. **Tema de terceiros não executa nada:** segmentos de comando vêm desligados. → Tarefa 5.

---

### Task 1: Bloco marcado no perfil do PowerShell

**Files:** `crates/core/src/terminal/{mod,profile}.rs`, `lib.rs`.

- [ ] Descobrir os caminhos do `$PROFILE` do PowerShell 7 e do Windows PowerShell (Documentos pode estar redirecionado ao OneDrive); `profile::apply(text, block)` e `profile::remove(text)` puros.
- [ ] O bloco contém só `oh-my-posh init pwsh --config <tema> | Invoke-Expression` (e os módulos opcionais ligados).
- [ ] Testes: `block_is_inserted_once_and_updated_in_place`, `remove_restores_original_text_exactly`, `text_outside_block_untouched_crlf_and_bom`, `broken_markers_are_refused`.
- [ ] Commit `feat(core): PowerShell profile managed block`.

### Task 2: Chaves gerenciadas do `settings.json` do Windows Terminal

**Files:** `terminal/wt_settings.rs`.

- [ ] Localizar o arquivo (Loja, avulso, Preview); ler com comentários e vírgula sobrando; alterar no texto só `font.face`, `font.size`, `colorScheme`, `opacity`, `useAcrylic`, `backgroundImage`, `cursorShape`, por perfil ou em `profiles.defaults`, e acrescentar/atualizar um esquema em `schemes`.
- [ ] Diff antes de gravar; recusar se uma chave não puder ser localizada com certeza.
- [ ] Testes: `comments_and_unmanaged_keys_preserved`, `only_managed_keys_change`, `unreadable_file_is_refused_not_rewritten`, `scheme_added_and_replaced`.
- [ ] Commit `feat(core): Windows Terminal settings edits`.

### Task 3: Backup, histórico e desfazer

**Files:** `terminal/backup.rs`.

- [ ] Backup datado antes da primeira escrita em cada arquivo; registro no histórico (mesmo arquivo de histórico dos temas, tipo `terminal`); desfazer devolve o backup só se o arquivo ainda está como o Sentinela o deixou (senão pede confirmação).
- [ ] Testes: `first_write_makes_backup_once`, `undo_restores_bytes`, `undo_refuses_when_file_changed_after`.
- [ ] Commit `feat(core): terminal backups and undo`.

### Task 4: Oh My Posh e fonte

**Files:** `terminal/{omp,host}.rs`.

- [ ] `TerminalHost` (Windows + falso): rodar `oh-my-posh` (versão, `font install`, `print primary`, exportar imagem) e achar o executável.
- [ ] Instalar/atualizar pelo `PackageManager` de `core::apps`; galeria dos temas que acompanham o programa (`POSH_THEMES_PATH`); tempo medido do prompt.
- [ ] Testes: `detects_install_and_version`, `font_list_and_install_commands`, `theme_gallery_reads_json_themes`, `prompt_timing_flags_slow_segments`; `#[ignore] real_oh_my_posh`.
- [ ] Commit `feat(core): Oh My Posh install and themes`.

### Task 5: Tema do prompt seguro e editor de segmentos

**Files:** `terminal/theme_file.rs`.

- [ ] Ler tema do prompt (JSON/YAML/TOML do Oh My Posh), listar segmentos e desligar os que executam comando; editor simples (pasta, git, hora, tempo do último comando, status, bateria) que gera o arquivo.
- [ ] Testes: `command_segments_are_disabled_and_listed`, `generated_theme_is_valid_json`, `unknown_segments_kept`.
- [ ] Commit `feat(core): safe prompt themes`.

### Task 6: Integração com Temas e cores do wallpaper

**Files:** `theme/{model,apply,archive}.rs`, `terminal/scheme.rs`.

- [ ] `theme.toml` ganha `[terminal]` (tema do prompt embutido no zip, fonte, esquema); prévia e desfazer em um passo junto com o resto do tema.
- [ ] Esquema de cores do Windows Terminal gerado da paleta do wallpaper (contraste mínimo conferido).
- [ ] Testes: `terminal_section_round_trip`, `import_keeps_prompt_theme_as_file_only`, `scheme_from_palette_meets_contrast`.
- [ ] Commit `feat(core): terminal in themes`.

### Task 7: Janela

**Files:** `app/src-tauri/src/commands/terminal.rs`, `app/src/components/Terminal.tsx`, `api.ts`, `pages/Personalizar.tsx`.

- [ ] Seção "Terminal" em Personalizar: estado das peças, instalar, tema do prompt com prévia, shells, Windows Terminal com diff, extras, tempo do prompt.
- [ ] Testes Vitest: `terminal_hidden_until_checked`, `shows_diff_before_apply`, `font_and_theme_install_confirmations`, `undo_from_history`.
- [ ] Commit `feat: terminal page`.

### Task 8: Documentação e roteiro manual

- [ ] README; `docs/testes-manuais-fase-5t.md` (PowerShell 7 e 5, Windows Terminal da Loja e avulso, OneDrive redirecionando Documentos, perfil com comentários e CRLF, tema hostil, desfazer).
- [ ] Commit `docs: phase 5T manual test script and README`.
