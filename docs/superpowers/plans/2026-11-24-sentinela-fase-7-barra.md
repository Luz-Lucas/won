# Sentinela — Fase 7: barra própria — Plano de implementação

**Spec:** §9.3 (peça "Barra"), §11 fase 7. Depende da fase 6 (`sentinela-desktop`, `desktop.toml`).

**Escopo:** barra nativa em Rust (GDI, sem WebView) numa borda do monitor principal, com módulos de texto e a opção de esconder a barra do Windows. Ficam para depois: vários monitores, bandeja, volume, mídia, rede, temperatura, módulo "comando", áreas de trabalho (fase 8).

**Decisões:**
- Seção `[bar]` do `desktop.toml` (borda, altura, cores, fonte, módulos `title`, `clock`, `cpu`, `ram` em `left`/`center`/`right`, formato do relógio). Validação como o resto: arquivo inválido mantém a configuração anterior.
- A barra é uma *barra de aplicativo* (`SHAppBarMessage`): o Windows reserva a borda e janelas maximizadas não passam por baixo.
- `hide_native_taskbar` esconde `Shell_TrayWnd`/`Shell_SecondaryTrayWnd` enquanto o programa roda. É devolvida ao parar, na saída de emergência e, como socorro, por `sentinela-desktop --stop` quando nenhum processo está rodando (caso o anterior tenha caído). O Explorer, ao reiniciar, reaparece com a barra: a cada segundo ela é escondida de novo.
- Em jogo (janela em foco cobrindo o monitor) a barra se esconde.
- Puro e testado: formato do relógio, faixa da borda, posição dos grupos, uso de CPU, truncar título, validação do `[bar]`.

**Tarefas:** 1) `bar.rs` + `[bar]` no `config.rs` (testes). 2) `winbar.rs` e integração no laço (clippy msvc). 3) README e `docs/testes-manuais-fase-7.md`.
