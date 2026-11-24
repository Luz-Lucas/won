# Sentinela — Fase 5: Wallpaper Engine e Temas — Plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Status:** rascunho para aprovação. Nada foi implementado.

**Goal:** Abrir o projeto B (ambiente de desktop) com o que não exige injeção nem processo novo: conversar com o Wallpaper Engine (detectar, galeria, aplicar por monitor, pausar em jogos, cores a partir do wallpaper) e criar o formato de **tema** (`.sentinela-theme`) que guarda os ajustes nativos da fase 4 mais o wallpaper, com aplicar, desfazer de uma vez, exportar e importar.

**Architecture:** `core::wallpaper` define `trait WallpaperHost` (versão do Windows chama o `wallpaper32/64.exe -control …`; versão falsa para testes), lê a biblioteca da Steam (app 431960) e a pasta da Oficina (`project.json` + miniatura) e extrai a paleta da miniatura (`core::wallpaper::palette`, quantização simples, sem dependência pesada). `core::theme` define `Theme` (`theme.toml` + arquivos), faz prévia, aplicação e importação, usando o motor de tweaks (§6) para os itens nativos e o diário para desfazer num passo só. A vigia de jogos (`game_watch.rs`, da fase N2/S2) ganha o gancho de pausar e retomar o wallpaper. Páginas novas dentro de Personalizar: "Papel de parede" e "Temas".

**Tech Stack:** Rust 2024, `zip` 9 (já no workspace), `toml`, `image` (só decodificação PNG/JPG da miniatura), windows-sys 0.61, Tauri 2, React + TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-11-16-sentinela-gui-otimizacao-design.md` — §9.4, §9.5, §11 fase 5.

## Decisões a confirmar

- **Sintaxe do controle:** a linha de comando do Wallpaper Engine (`-control openWallpaper | pause | play | stop | mute | applyProperties`) precisa ser conferida na instalação real antes de fixar as flags; os testes usam o `WallpaperHost` falso, e a Tarefa 1 termina com um `#[ignore]` que roda no PC do Lucas.
- **Cores do wallpaper:** a troca é percebida observando o `config.json` do Wallpaper Engine (e não pelo processo), com atraso de poucos segundos. A cor aplicada é só a de destaque do Windows (tweak nativo já existente); barra, bordas e Windows Terminal ficam para as fases 6, 7 e 5T.
- **Temas só com o que existe hoje:** nesta fase o `theme.toml` cobre ajustes nativos (§9.1), wallpaper e cores. Campos de mods, barra, tiling e atalhos ficam reservados no formato (`version = 1`, seções desconhecidas ignoradas com aviso) mas sem efeito até as fases 6–13.
- **Importação segura:** itens que rodam comandos vêm desligados; caminhos do zip validados (sem `..`, sem caminho absoluto); limite de tamanho por entrada e no total (mesma lógica de `scan::archive`); wallpaper da Oficina ausente mostra o link para assinar.
- **Pausar em jogos:** `pause` por padrão, `stop` (libera a VRAM) como opção; reutiliza a detecção de jogos abertos da vigia, sem processo novo.

## Global Constraints

- Só Windows; CI `windows-latest`; builds locais com `CARGO_TARGET_DIR=E:/st`.
- Toda tarefa termina com `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `npm test` e `npm run build` verdes.
- Testes sem Wallpaper Engine e sem Steam (host e biblioteca falsos); leitura real em `#[ignore]`.
- A limpeza nunca toca na Oficina nem na configuração do Wallpaper Engine; o Sentinela só lê essas pastas.
- Sem o Wallpaper Engine instalado, a seção some e o tema usa o papel de parede estático.

## Review Focus

1. **Aplicar tema é reversível em um passo:** o Histórico mostra um item e "Desfazer" volta tudo, inclusive o wallpaper anterior. → Tarefa 4.
2. **Importação não escapa da pasta nem executa nada:** zip hostil (caminho com `..`, bomba, comando) é recusado ou vem desligado. → Tarefa 5.
3. **Pausa em jogos** sempre retoma (jogo fecha, app fecha, erro do host). → Tarefa 3.

---

### Task 1: Detecção e controle do Wallpaper Engine

**Files:** `crates/core/src/wallpaper/{mod,host,steam}.rs`, `lib.rs`.

- [ ] `trait WallpaperHost` (`open`, `pause`, `play`, `stop`, `mute`, `apply_properties`), `WindowsHost` e `FakeHost`.
- [ ] Detectar instalação pela biblioteca da Steam (`libraryfolders.vdf`, app 431960), versão e se está rodando.
- [ ] Testes: `finds_install_in_second_library`, `missing_install_hides_section`, `fake_host_records_commands`; `#[ignore] real_install_and_control`.
- [ ] Commit `feat(core): Wallpaper Engine detection and control`.

### Task 2: Galeria da Oficina e cores

**Files:** `wallpaper/{gallery,palette}.rs`, `Cargo.toml`.

- [ ] Listar wallpapers (`project.json`, título, tipo, miniatura), por monitor.
- [ ] Paleta da miniatura e escolha da cor de destaque; observar o `config.json` para perceber a troca.
- [ ] Testes: `reads_project_json_with_missing_fields`, `palette_picks_dominant_vivid_color`, `config_change_detected`.
- [ ] Commit `feat(core): wallpaper gallery and palette`.

### Task 3: Pausa em jogos

**Files:** `app/src-tauri/src/game_watch.rs`, `wallpaper/mod.rs`.

- [ ] Ligar a vigia de jogos ao host: `pause`/`stop` ao abrir, `play` ao fechar; opção escolhida em Ajustes.
- [ ] Testes: `pause_on_game_open_and_play_on_close`, `play_on_app_exit`, `host_error_does_not_leave_it_paused`.
- [ ] Commit `feat: pause Wallpaper Engine while a game runs`.

### Task 4: Formato de tema e aplicação

**Files:** `crates/core/src/theme/{mod,model,apply}.rs`.

- [ ] `theme.toml` v1 (nativos, wallpaper, cores), prévia do que muda, aplicar pelo motor de tweaks com um passo no diário, desfazer de uma vez.
- [ ] Temas prontos embutidos: minimalista, translúcido.
- [ ] Testes: `preview_lists_every_change`, `apply_is_one_journal_step`, `undo_restores_wallpaper`, `unknown_sections_warn_not_fail`.
- [ ] Commit `feat(core): themes`.

### Task 5: Exportar e importar

**Files:** `theme/archive.rs`.

- [ ] `.sentinela-theme` (zip): exportar o estado atual, importar com validação (caminhos, limites, itens com comando desligados).
- [ ] Testes: `zip_slip_rejected`, `oversized_entry_rejected`, `command_items_import_disabled`, `missing_workshop_wallpaper_gives_link`, `export_import_round_trip`.
- [ ] Commit `feat(core): theme export and import`.

### Task 6: Janela

**Files:** `app/src-tauri/src/commands/{wallpaper,theme}.rs`, `app/src/pages/Personalizar.tsx`, `components/{WallpaperGallery,Themes}.tsx`, `api.ts`.

- [ ] Abas "Papel de parede" e "Temas" em Personalizar; seção some sem o Wallpaper Engine.
- [ ] Testes Vitest: `gallery_hidden_without_wallpaper_engine`, `apply_theme_shows_preview_first`, `import_shows_warnings`, `pause_mode_setting`.
- [ ] Commit `feat: wallpaper and themes pages`.

### Task 7: Documentação e roteiro manual

- [ ] README; `docs/testes-manuais-fase-5.md` (Wallpaper Engine real, playlists, dois monitores, jogo em tela cheia, tema importado hostil).
- [ ] Commit `docs: phase 5 manual test script and README`.
