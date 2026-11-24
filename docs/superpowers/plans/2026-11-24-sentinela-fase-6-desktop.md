# Sentinela — Fase 6: `sentinela-desktop` — Plano de implementação

**Spec:** `docs/superpowers/specs/2026-11-16-sentinela-gui-otimizacao-design.md` — §9.3, §9.5, §11 fase 6, §13 (testes de `desktop`).

**Escopo desta fase** (a tabela do §11 diz: atalhos globais, regras de janela, bordas e fundo, itens do sistema, saída de emergência, `desktop.toml`). Barra, tiling, áreas e lançador ficam para as fases 7, 8 e 9.

**Decisões (minhas, sem injeção):**
- Novo crate `crates/desktop` com a lib `sentinela_desktop` (tudo que é puro: `desktop.toml`, atalhos, regras; testado no Linux) e o bin `sentinela-desktop` (Windows: laço de mensagens, `RegisterHotKey`, `SetWinEventHook`, DWM). Só APIs públicas do Windows.
- `%APPDATA%\Sentinela\desktop.toml`, recarregado ao salvar; arquivo inválido **mantém a configuração anterior** e o erro vai para a janela.
- Ações de atalho: `run` (comando do próprio usuário), `toggle-topmost`, `opacity-up`, `opacity-down`, `center`, `toggle-maximize`. Regras: `topmost`, `opacity`, `no-titlebar`, `corners`, `backdrop`, `border-active`/`border-inactive`.
- Casamento de regra por executável, título e classe (curinga `*`, sem diferenciar maiúsculas). Janelas do sistema (`Progman`, `WorkerW`, `Shell_TrayWnd`) e do próprio Sentinela nunca são tocadas; o Wallpaper Engine desenha ali.
- Em jogo (janela em primeiro plano cobre o monitor inteiro) o processo **não toca em nenhuma janela e ignora atalhos que mexem em janelas**; o resto da configuração segue.
- **Saída de emergência** `Win+Ctrl+Shift+Esc`, fixa (não pode ser religada nem trocada no arquivo): desfaz tudo que mudou nas janelas e encerra o processo.
- Tudo que o processo muda numa janela é lembrado (estilo, opacidade, topmost, atributos do DWM) para a emergência e para desligar uma regra.
- Uma instância só (mutex nomeado). A janela (Tauri) inicia e para o processo; início com o Windows fica opcional e desligado.
- Itens do sistema (cursores, sons, fontes): ficam para uma fase própria; a cor de destaque já existe nos Temas (fase 5).

**Tarefas**
1. Crate `desktop` + `config.rs` (parse/validação/`Loader` que mantém a anterior) + `keys.rs`. Testes.
2. `rules.rs` (casamento, efeitos combinados, janelas protegidas) e `fullscreen` puro. Testes.
3. `win.rs` (Windows): laço, atalhos, hooks de janela, efeitos e desfazer, emergência, instância única. Verificação: clippy no alvo msvc.
4. Janela: comandos Tauri `desktop_*` (estado, iniciar/parar, ler/gravar o arquivo com validação), seção "Desktop" em Personalizar, empacotamento do exe. Vitest.
5. README e `docs/testes-manuais-fase-6.md`.

Cada tarefa fecha com `cargo fmt --check`, `clippy -D warnings` (nativo e msvc), `cargo test`, `npm test`, `npm run build`.
